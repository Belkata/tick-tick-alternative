//! The daemon's sync node (M3 slice 2, plan/16): serves paired devices, syncs with them
//! periodically and soon after local edits, and answers `tt sync` and the app's Devices
//! page through the socket (`tt_api::server::SyncControl`). Only this process uses the
//! device key while the daemon runs.

use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Result;
use serde_json::Value;
use tt_api::server::SyncControl;
use tt_api::{Request, SyncOutcome, SyncStatus};
use tt_sync::replicate::{SyncEvent, SyncReport};
use tt_sync::{Relays, SyncNode};

/// How often to try every paired device.
pub const PERIOD: Duration = Duration::from_secs(90);
/// How long after a local edit to sync (edits in quick succession share one round).
pub const AFTER_EDIT: Duration = Duration::from_secs(3);

#[derive(Default)]
struct State {
    last_sync_ms: Option<i64>,
    last_error: Option<String>,
    peers: usize,
}

pub struct SyncService {
    node: SyncNode,
    state: Mutex<State>,
    /// One round at a time (periodic, after an edit, or `tt sync now`).
    round: tokio::sync::Mutex<()>,
}

impl SyncService {
    /// Start the node on `db`, with its key next to it (`tt.db` → `tt.key`, as `tt sync`).
    pub async fn start(db: &Path, device: String, relays: Relays) -> Result<Arc<SyncService>> {
        let node =
            SyncNode::start(db.to_path_buf(), &db.with_extension("key"), device, relays).await?;
        let peers = node.peers().await.map(|p| p.len()).unwrap_or(0);
        let svc = Arc::new(SyncService {
            node,
            state: Mutex::new(State {
                peers,
                ..State::default()
            }),
            round: tokio::sync::Mutex::new(()),
        });
        // Syncs a peer started also count as "synced".
        let mut events = svc.node.subscribe_sync();
        let weak = Arc::downgrade(&svc);
        tokio::spawn(async move {
            while let Ok(ev) = events.recv().await {
                let Some(svc) = weak.upgrade() else { break };
                match ev {
                    SyncEvent::Synced(r) => {
                        svc.note_success();
                        eprintln!(
                            "tt-daemon: synced with {}: sent {}, received {}",
                            r.peer_name, r.sent, r.received
                        );
                    }
                    SyncEvent::Paired { name, .. } => {
                        svc.refresh_peers().await;
                        eprintln!("tt-daemon: paired with {name}");
                    }
                    SyncEvent::Refused { peer_id, reason } => {
                        eprintln!(
                            "tt-daemon: refused {}: {reason}",
                            &peer_id[..peer_id.len().min(10)]
                        );
                    }
                }
            }
        });
        Ok(svc)
    }

    /// Try every paired device. Rounds don't overlap.
    pub async fn round(&self) -> Vec<SyncOutcome> {
        let _one_at_a_time = self.round.lock().await;
        let results = self.node.sync_all().await;
        let outcomes: Vec<SyncOutcome> = results
            .into_iter()
            .map(|(peer, r)| match r {
                Ok(r) => outcome(r),
                Err(e) => SyncOutcome {
                    peer_id: peer.node_id,
                    peer_name: peer.name,
                    error: Some(format!("{e:#}")),
                    sent: 0,
                    received: 0,
                    applied: 0,
                },
            })
            .collect();
        {
            let mut st = lock(&self.state);
            st.peers = outcomes.len();
            if outcomes.iter().any(|o| o.error.is_none()) {
                st.last_sync_ms = Some(now_ms());
                st.last_error = None;
            } else if let Some(o) = outcomes.first() {
                st.last_error = Some(format!(
                    "{}: {}",
                    o.peer_name,
                    o.error.clone().unwrap_or_default()
                ));
            }
        }
        outcomes
    }

    pub fn status(&self) -> SyncStatus {
        let st = lock(&self.state);
        SyncStatus {
            node_id: self.node.id(),
            peers: st.peers as u32,
            last_sync_ms: st.last_sync_ms,
            last_error: st.last_error.clone(),
        }
    }

    /// A short line for the tray: "Synced 3 min ago", "No paired devices"…
    pub fn status_line(&self) -> String {
        let st = lock(&self.state);
        if st.peers == 0 {
            return "No paired devices".into();
        }
        match (st.last_sync_ms, &st.last_error) {
            (Some(t), None) => format!("Synced {}", ago(now_ms() - t)),
            (Some(t), Some(_)) => format!("Synced {} (last try failed)", ago(now_ms() - t)),
            (None, Some(_)) => "Sync failed: devices offline?".into(),
            (None, None) => "Not synced yet".into(),
        }
    }

    fn note_success(&self) {
        let mut st = lock(&self.state);
        st.last_sync_ms = Some(now_ms());
        st.last_error = None;
    }

    async fn refresh_peers(&self) {
        if let Ok(p) = self.node.peers().await {
            lock(&self.state).peers = p.len();
        }
    }

    pub async fn shutdown(self: Arc<Self>) {
        if let Ok(svc) = Arc::try_unwrap(self) {
            svc.node.shutdown().await;
        }
    }
}

impl SyncControl for SyncService {
    fn handle(
        &self,
        req: Request,
    ) -> Pin<Box<dyn Future<Output = Result<Value, String>> + Send + '_>> {
        Box::pin(async move {
            let v = match req {
                Request::SyncPair => {
                    serde_json::to_value(self.node.pairing_ticket().map_err(|e| format!("{e:#}"))?)
                }
                Request::SyncJoin { ticket } => {
                    let r = self
                        .node
                        .join(&ticket)
                        .await
                        .map_err(|e| format!("{e:#}"))?;
                    self.refresh_peers().await;
                    serde_json::to_value(outcome(r))
                }
                Request::SyncNow => serde_json::to_value(self.round().await),
                Request::SyncStatus => serde_json::to_value(self.status()),
                other => return Err(format!("not a sync request: {other:?}")),
            };
            v.map_err(|e| e.to_string())
        })
    }
}

fn outcome(r: SyncReport) -> SyncOutcome {
    SyncOutcome {
        peer_id: r.peer_id,
        peer_name: r.peer_name,
        error: None,
        sent: r.sent as u32,
        received: r.received as u32,
        applied: r.applied as u32,
    }
}

fn ago(ms: i64) -> String {
    match ms / 1000 {
        s if s < 60 => "just now".into(),
        s if s < 3600 => format!("{} min ago", s / 60),
        s if s < 86_400 => format!("{} h ago", s / 3600),
        s => format!("{} days ago", s / 86_400),
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}
