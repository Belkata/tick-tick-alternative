//! A running sync node with its status and auto-sync (plan/16): serves paired devices and
//! syncs with them periodically and soon after local edits. Used by `utre-daemon` on desktop
//! and in-process by the phone app (no daemon there). One per device key at a time.

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Result;
use tokio::sync::broadcast;
use utre_api::{SyncOutcome, SyncStatus};

use crate::replicate::{SyncEvent, SyncReport};
use crate::{Relays, SyncNode};

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
    pub node: SyncNode,
    state: Mutex<State>,
    /// One round at a time (periodic, after an edit, or `utre sync now`).
    round: tokio::sync::Mutex<()>,
}

impl SyncService {
    /// Start the node on `db`, with its key next to it (`utre.db` → `utre.key`, as `utre sync`).
    pub async fn start(db: &Path, device: String, relays: Relays) -> Result<Arc<SyncService>> {
        Self::start_with_key(db, &db.with_extension("key"), device, relays).await
    }

    /// Start the node on `db` with the key at `key`. Must run inside a tokio runtime.
    pub async fn start_with_key(
        db: &Path,
        key: &Path,
        device: String,
        relays: Relays,
    ) -> Result<Arc<SyncService>> {
        let node = SyncNode::start(db.to_path_buf(), key, device, relays).await?;
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
                            "utre-daemon: synced with {}: sent {}, received {}",
                            r.peer_name, r.sent, r.received
                        );
                    }
                    SyncEvent::Paired { name, .. } => {
                        svc.refresh_peers().await;
                        eprintln!("utre-daemon: paired with {name}");
                    }
                    SyncEvent::TrustChanged { added, removed } => {
                        svc.refresh_peers().await;
                        eprintln!(
                            "utre-daemon: trust shared by a peer: added {added:?}, removed {removed:?}"
                        );
                    }
                    SyncEvent::Refused { peer_id, reason } => {
                        eprintln!(
                            "utre-daemon: refused {}: {reason}",
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

    /// Pairing, sync and trust events (e.g. to refresh a UI after a peer synced).
    pub fn subscribe(&self) -> broadcast::Receiver<SyncEvent> {
        self.node.subscribe_sync()
    }

    /// Auto-sync: shortly after starting, every [`PERIOD`], and [`AFTER_EDIT`] after each
    /// local edit announced on `edits` (edits in quick succession share one round).
    /// `after_round` runs after every round (e.g. to refresh a tray or a UI).
    pub fn spawn_auto(
        self: &Arc<Self>,
        mut edits: broadcast::Receiver<()>,
        after_round: impl Fn() + Send + 'static,
    ) {
        let svc = Arc::downgrade(self);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(5)).await;
            let mut period = tokio::time::interval(PERIOD);
            loop {
                tokio::select! {
                    _ = period.tick() => {}
                    r = edits.recv() => {
                        if matches!(r, Err(broadcast::error::RecvError::Closed)) {
                            break;
                        }
                        tokio::time::sleep(AFTER_EDIT).await;
                        while edits.try_recv().is_ok() {}
                    }
                }
                let Some(svc) = svc.upgrade() else { break };
                svc.round().await;
                after_round();
            }
        });
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

    pub async fn refresh_peers(&self) {
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

pub fn outcome(r: SyncReport) -> SyncOutcome {
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
