//! M3 sync protocol (plan/05): pairing, trusted peers and op-log replication over one
//! QUIC bidirectional stream per connection.
//!
//! Frames are JSON lines. A session:
//! 1. **Hello** both ways: protocol version, device name, the sender's address and its
//!    version vector. A server answers an untrusted node's Hello with **Denied**.
//! 2. Each side streams the ops the other is missing in **Ops** batches, then **Done**,
//!    sending and receiving at the same time so big syncs can't deadlock on flow control.
//! 3. Each applies what it got (one transaction per batch) and sends **Ack**.
//!
//! Pairing (`tt2…` tickets) runs first on the same stream: **Pair** with the ticket's
//! one-time secret, answered by **Paired**; both sides then trust each other and sync.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use iroh::EndpointAddr;
use iroh::endpoint::{Connection, RecvStream, SendStream};
use iroh::protocol::{AcceptError, ProtocolHandler};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::sync::broadcast;
use utre_core::hlc::Hlc;
use utre_core::oplog::Op;
use utre_core::{Peer, Store, TrustList, VersionVector};

/// Application protocol id for real sync. Bump it when the frames change incompatibly.
pub const SYNC_ALPN: &[u8] = b"tickalt/sync/1";
/// 2: sessions end with a `Trust` frame (trust sharing, plan/05 pairing step 4).
const PROTO: u32 = 2;
const PAIR_PREFIX: &str = "tt2";
/// Ops per frame, at most.
const BATCH: usize = 500;
/// A frame's ops stop growing once their JSON passes this (a frame over [`MAX_FRAME`]
/// is refused by the receiver, so 500 ops carrying long notes must be split).
const BATCH_BYTES: usize = 1 << 20;
/// Longest frame accepted (an untrusted peer gets one frame at most).
const MAX_FRAME: u64 = 8 << 20;
/// How long the serving side waits for a connection's first frame.
const FIRST_FRAME_WAIT: Duration = Duration::from_secs(30);
/// How long a pairing ticket stays valid.
pub const PAIR_TTL: Duration = Duration::from_secs(300);

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "frame", rename_all = "snake_case")]
enum Frame {
    Hello {
        proto: u32,
        name: String,
        /// The sender's EndpointAddr as JSON, so the other side can dial it later.
        addr: Option<String>,
        /// The version vector as its HLCs (each names its device). A list rather than a
        /// map: serde can't read integer map keys inside an internally tagged enum.
        vv: Vec<Hlc>,
    },
    Pair {
        secret: String,
        name: String,
        addr: Option<String>,
    },
    Paired {
        name: String,
    },
    Denied {
        reason: String,
    },
    Ops {
        ops: Vec<Op>,
    },
    Done,
    Ack {
        applied: usize,
    },
    /// Who the sender trusts and who it forgot; merged by the receiver.
    Trust {
        list: TrustList,
    },
}

/// What one sync with one peer did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SyncReport {
    pub peer_id: String,
    pub peer_name: String,
    /// Ops sent to the peer.
    pub sent: usize,
    /// Ops received from the peer.
    pub received: usize,
    /// Received ops that changed a field here (the rest were already known or superseded).
    pub applied: usize,
    /// Devices this one now trusts because the peer did (names).
    pub learned: Vec<String>,
    /// Devices dropped because the peer had forgotten them (names).
    pub dropped: Vec<String>,
}

/// Something that happened, for the daemon's tray and logs.
#[derive(Debug, Clone)]
pub enum SyncEvent {
    Synced(SyncReport),
    Paired {
        peer_id: String,
        name: String,
    },
    Refused {
        peer_id: String,
        reason: String,
    },
    /// A peer's trust list added or removed devices here (names).
    TrustChanged {
        added: Vec<String>,
        removed: Vec<String>,
    },
}

/// The database, opened per call on the blocking pool (SQLite's WAL lets this run beside
/// the daemon's own connection, which notices these writes through `data_version`).
#[derive(Debug, Clone)]
pub struct FileReplica {
    db: PathBuf,
}

impl FileReplica {
    pub fn new(db: PathBuf) -> Self {
        Self { db }
    }

    async fn with<T: Send + 'static>(
        &self,
        f: impl FnOnce(&Store) -> utre_core::Result<T> + Send + 'static,
    ) -> Result<T> {
        let db = self.db.clone();
        tokio::task::spawn_blocking(move || {
            let store = Store::open(&db).with_context(|| format!("opening {}", db.display()))?;
            Ok(f(&store)?)
        })
        .await?
    }

    pub async fn peers(&self) -> Result<Vec<Peer>> {
        self.with(|s| s.peers()).await
    }

    pub async fn remove_peer(&self, node_id: String) -> Result<bool> {
        self.with(move |s| s.remove_peer(&node_id)).await
    }
}

/// An outstanding pairing offer (one at a time; used once).
struct Offer {
    secret: String,
    expires: Instant,
}

/// The sync side of a node: answers SYNC_ALPN connections and dials peers.
#[derive(Clone)]
pub struct Replicator {
    pub(crate) replica: FileReplica,
    pub(crate) name: String,
    /// This node's endpoint id (never added or removed through a peer's trust list).
    self_id: String,
    offer: Arc<Mutex<Option<Offer>>>,
    events: broadcast::Sender<SyncEvent>,
}

impl std::fmt::Debug for Replicator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Replicator")
            .field("name", &self.name)
            .finish()
    }
}

impl Replicator {
    pub fn new(replica: FileReplica, name: String, self_id: String) -> Self {
        let (events, _) = broadcast::channel(32);
        Self {
            replica,
            name,
            self_id,
            offer: Arc::default(),
            events,
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<SyncEvent> {
        self.events.subscribe()
    }

    /// Make a one-time pairing ticket for `addr` (this node), valid for [`PAIR_TTL`].
    pub fn offer_pairing(&self, addr: &EndpointAddr) -> Result<String> {
        let mut bytes = [0u8; 16];
        getrandom::fill(&mut bytes).map_err(|e| anyhow::anyhow!("no randomness: {e}"))?;
        let secret = hex(&bytes);
        *self.offer.lock().unwrap_or_else(|e| e.into_inner()) = Some(Offer {
            secret: secret.clone(),
            expires: Instant::now() + PAIR_TTL,
        });
        let json = serde_json::to_vec(&PairTicket {
            addr: addr.clone(),
            secret,
            name: self.name.clone(),
        })?;
        Ok(format!("{PAIR_PREFIX}{}", hex(&json)))
    }

    /// Take the offer if `secret` matches and it hasn't expired.
    fn redeem(&self, secret: &str) -> bool {
        let mut offer = self.offer.lock().unwrap_or_else(|e| e.into_inner());
        match &*offer {
            Some(o) if o.secret == secret && Instant::now() < o.expires => {
                *offer = None;
                true
            }
            _ => false,
        }
    }

    /// Dialing side, after pairing (if any): run a session on a fresh stream.
    pub(crate) async fn client_session(
        &self,
        conn: &Connection,
        my_addr: Option<String>,
        pair: Option<(String, EndpointAddr)>,
    ) -> Result<SyncReport> {
        let peer_id = conn.remote_id().to_string();
        let (mut send, recv) = conn.open_bi().await.context("opening a stream")?;
        let mut recv = BufReader::new(recv);
        if let Some((secret, addr)) = pair {
            write_frame(
                &mut send,
                &Frame::Pair {
                    secret,
                    name: self.name.clone(),
                    addr: my_addr.clone(),
                },
            )
            .await?;
            match read_frame(&mut recv).await? {
                Frame::Paired { name } => {
                    let addr = serde_json::to_string(&addr)?;
                    let id = peer_id.clone();
                    let n = name.clone();
                    self.replica
                        .with(move |s| s.add_peer(&id, &n, Some(&addr), now_ms()))
                        .await?;
                    let _ = self.events.send(SyncEvent::Paired {
                        peer_id: peer_id.clone(),
                        name,
                    });
                }
                Frame::Denied { reason } => bail!("pairing refused: {reason}"),
                other => bail!("unexpected answer to pairing: {other:?}"),
            }
        }
        let result = self
            .session(&mut send, &mut recv, &peer_id, my_addr, None)
            .await;
        send.finish().ok();
        conn.close(0u32.into(), b"done");
        result
    }

    /// Serving side: the first frame decides between pairing, a trusted sync and refusal.
    async fn serve(&self, conn: &Connection) -> Result<SyncReport> {
        let peer_id = conn.remote_id().to_string();
        let (mut send, recv) = conn.accept_bi().await?;
        let mut recv = BufReader::new(recv);
        let my_addr = None; // The dialer already knows how to reach us.
        // Anyone who can reach the endpoint may connect: don't wait forever for a first frame.
        let first = tokio::time::timeout(FIRST_FRAME_WAIT, read_frame(&mut recv))
            .await
            .context("no first frame from the peer in time")??;
        let theirs = match first {
            Frame::Pair { secret, name, addr } => {
                if !self.redeem(&secret) {
                    return self
                        .refuse(
                            &mut send,
                            &peer_id,
                            "the pairing ticket is wrong, used or expired",
                        )
                        .await;
                }
                let (id, n) = (peer_id.clone(), name.clone());
                self.replica
                    .with(move |s| s.add_peer(&id, &n, addr.as_deref(), now_ms()))
                    .await?;
                write_frame(
                    &mut send,
                    &Frame::Paired {
                        name: self.name.clone(),
                    },
                )
                .await?;
                let _ = self.events.send(SyncEvent::Paired {
                    peer_id: peer_id.clone(),
                    name,
                });
                None // Their Hello comes next.
            }
            hello @ Frame::Hello { .. } => {
                let id = peer_id.clone();
                if !self.replica.with(move |s| s.is_trusted(&id)).await? {
                    return self
                        .refuse(&mut send, &peer_id, "this device isn't paired")
                        .await;
                }
                Some(hello)
            }
            other => bail!("unexpected first frame {other:?}"),
        };
        let report = self
            .session(&mut send, &mut recv, &peer_id, my_addr, theirs)
            .await?;
        send.finish().ok();
        // Let the dialer close, so our last frames aren't cut off.
        conn.closed().await;
        Ok(report)
    }

    async fn refuse(
        &self,
        send: &mut SendStream,
        peer_id: &str,
        reason: &str,
    ) -> Result<SyncReport> {
        write_frame(
            send,
            &Frame::Denied {
                reason: reason.into(),
            },
        )
        .await?;
        send.finish().ok();
        let _ = self.events.send(SyncEvent::Refused {
            peer_id: peer_id.into(),
            reason: reason.into(),
        });
        bail!("refused {peer_id}: {reason}")
    }

    /// Hello, then ops both ways at once, then acks.
    async fn session(
        &self,
        send: &mut SendStream,
        recv: &mut BufReader<RecvStream>,
        peer_id: &str,
        my_addr: Option<String>,
        theirs: Option<Frame>,
    ) -> Result<SyncReport> {
        let vv = self.replica.with(|s| s.version_vector()).await?;
        let hello = Frame::Hello {
            proto: PROTO,
            name: self.name.clone(),
            addr: my_addr,
            vv: vv.into_values().collect(),
        };
        let theirs = match theirs {
            // We read theirs first (serving side): answer with ours.
            Some(t) => {
                write_frame(send, &hello).await?;
                t
            }
            None => {
                write_frame(send, &hello).await?;
                read_frame(recv).await?
            }
        };
        let Frame::Hello {
            proto,
            name: peer_name,
            addr: peer_addr,
            vv: their_vv,
        } = theirs
        else {
            match theirs {
                Frame::Denied { reason } => bail!("refused: {reason}"),
                other => bail!("expected hello, got {other:?}"),
            }
        };
        if proto != PROTO {
            bail!("peer speaks sync protocol {proto}, this build speaks {PROTO}");
        }

        let their_vv: VersionVector = their_vv.into_iter().map(|h| (h.device, h)).collect();
        let missing = self.replica.with(move |s| s.ops_missing(&their_vv)).await?;
        let sent = missing.len();
        let replica = self.replica.clone();
        let sending = async {
            for chunk in batches(&missing) {
                write_frame(
                    send,
                    &Frame::Ops {
                        ops: chunk.to_vec(),
                    },
                )
                .await?;
            }
            write_frame(send, &Frame::Done).await?;
            Ok::<_, anyhow::Error>(())
        };
        let receiving = async {
            let (mut received, mut applied) = (0, 0);
            loop {
                match read_frame(recv).await? {
                    Frame::Ops { ops } => {
                        received += ops.len();
                        applied += replica.with(move |s| s.apply_remote(&ops)).await?;
                    }
                    Frame::Done => return Ok::<_, anyhow::Error>((received, applied)),
                    other => bail!("expected ops, got {other:?}"),
                }
            }
        };
        let (s, r) = tokio::join!(sending, receiving);
        s?;
        let (received, applied) = r?;

        write_frame(send, &Frame::Ack { applied }).await?;
        match read_frame(recv).await? {
            Frame::Ack { .. } => {}
            other => bail!("expected ack, got {other:?}"),
        }

        // Trust sharing: each side tells the other who it trusts and who it forgot.
        let mine = self.replica.with(|s| s.trust_list()).await?;
        write_frame(send, &Frame::Trust { list: mine }).await?;
        let theirs = match read_frame(recv).await? {
            Frame::Trust { list } => list,
            other => bail!("expected trust, got {other:?}"),
        };
        let (me, from) = (self.self_id.clone(), peer_id.to_string());
        let change = self
            .replica
            .with(move |s| s.merge_trust(&theirs, &me, &from))
            .await?;
        if !change.added.is_empty() || !change.removed.is_empty() {
            let _ = self.events.send(SyncEvent::TrustChanged {
                added: change.added.clone(),
                removed: change.removed.clone(),
            });
        }

        let (id, n) = (peer_id.to_string(), peer_name.clone());
        self.replica
            .with(move |s| s.peer_seen(&id, &n, peer_addr.as_deref(), now_ms()))
            .await?;
        let report = SyncReport {
            peer_id: peer_id.to_string(),
            peer_name,
            sent,
            received,
            applied,
            learned: change.added,
            dropped: change.removed,
        };
        let _ = self.events.send(SyncEvent::Synced(report.clone()));
        Ok(report)
    }
}

impl ProtocolHandler for Replicator {
    async fn accept(&self, conn: Connection) -> Result<(), AcceptError> {
        match self.serve(&conn).await {
            Ok(_) => Ok(()),
            Err(e) => {
                // Give the dialer a moment to read a Denied frame and hang up itself.
                let _ = tokio::time::timeout(Duration::from_secs(3), conn.closed()).await;
                conn.close(1u32.into(), b"sync failed");
                Err(AcceptError::from_boxed(e.into()))
            }
        }
    }
}

/// What a `tt2…` pairing ticket holds.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct PairTicket {
    pub addr: EndpointAddr,
    pub secret: String,
    pub name: String,
}

pub(crate) fn decode_pair_ticket(ticket: &str) -> Result<PairTicket> {
    let hex = ticket
        .trim()
        .strip_prefix(PAIR_PREFIX)
        .context("not a Utre pairing ticket (expected tt2…)")?;
    serde_json::from_slice(&unhex(hex)?).context("pairing ticket is damaged")
}

async fn write_frame(send: &mut SendStream, frame: &Frame) -> Result<()> {
    let mut line = serde_json::to_vec(frame)?;
    line.push(b'\n');
    send.write_all(&line).await.context("sending")?;
    Ok(())
}

async fn read_frame(recv: &mut BufReader<RecvStream>) -> Result<Frame> {
    let mut line = Vec::new();
    let n = (&mut *recv)
        .take(MAX_FRAME)
        .read_until(b'\n', &mut line)
        .await
        .context("receiving")?;
    if n == 0 {
        bail!("the peer closed the stream");
    }
    if line.last() != Some(&b'\n') {
        bail!("frame too long or cut off");
    }
    serde_json::from_slice(&line).context("bad frame")
}

/// Split ops into frames of at most [`BATCH`] ops and about [`BATCH_BYTES`] of JSON.
/// A single op bigger than that still goes alone.
fn batches(ops: &[Op]) -> Vec<&[Op]> {
    let mut out = Vec::new();
    let (mut start, mut bytes) = (0, 0);
    for (i, op) in ops.iter().enumerate() {
        let size = op.value.to_string().len() + op.entity_id.len() + op.field.len() + 100;
        if i > start && (i - start >= BATCH || bytes + size > BATCH_BYTES) {
            out.push(&ops[start..i]);
            (start, bytes) = (i, 0);
        }
        bytes += size;
    }
    if start < ops.len() {
        out.push(&ops[start..]);
    }
    out
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub(crate) fn unhex(s: &str) -> Result<Vec<u8>> {
    if !s.is_ascii() {
        bail!("not hex"); // Also keeps the byte slicing below on character boundaries.
    }
    if !s.len().is_multiple_of(2) {
        bail!("odd length; was it cut off?");
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).context("not hex"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use utre_core::oplog::Entity;

    fn op(i: u64, note: &str) -> Op {
        Op {
            hlc: Hlc {
                wall: i,
                counter: 0,
                device: 1,
            },
            entity: Entity::Task,
            entity_id: "t".into(),
            field: "notes".into(),
            value: serde_json::json!(note),
        }
    }

    #[test]
    fn batches_respect_count_and_size() {
        let small: Vec<Op> = (0..1200).map(|i| op(i, "x")).collect();
        let b = batches(&small);
        assert_eq!(b.iter().map(|c| c.len()).collect::<Vec<_>>(), [500, 500, 200]);

        // 40 ops of 100 KB: far over MAX_FRAME if sent as one frame, so they must split.
        let note = "n".repeat(100_000);
        let big: Vec<Op> = (0..40).map(|i| op(i, &note)).collect();
        let b = batches(&big);
        assert!(b.len() > 1);
        assert_eq!(b.iter().map(|c| c.len()).sum::<usize>(), 40);
        for chunk in &b {
            let frame = serde_json::to_vec(&Frame::Ops { ops: chunk.to_vec() }).unwrap();
            assert!((frame.len() as u64) < MAX_FRAME);
        }

        // One op over the limit goes alone; nothing yields no frames.
        let huge = [op(1, &"n".repeat(2 << 20)), op(2, "x")];
        assert_eq!(batches(&huge).iter().map(|c| c.len()).collect::<Vec<_>>(), [1, 1]);
        assert!(batches(&[]).is_empty());
    }
}
