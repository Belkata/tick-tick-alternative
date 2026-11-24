//! Spike 2: iroh transport between Tick Alt devices.
//!
//! Spike-quality only. A node serves its open tasks over one QUIC bidirectional
//! stream; a peer dials it by ticket and reports which network path it got.
//! The real protocol (hello + version vectors + op streaming) replaces this in M3.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use iroh::endpoint::{Connection, presets};
use iroh::protocol::{AcceptError, ProtocolHandler, Router};
use iroh::{Endpoint, EndpointAddr, RelayMode, SecretKey, TransportAddr};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tt_core::Store;

/// Application protocol id. Bump it whenever the wire format changes.
pub const ALPN: &[u8] = b"tickalt/spike/0";
const TICKET_PREFIX: &str = "tt1";
const MAX_MSG: usize = 1 << 20;
/// How long the client waits for hole punching to upgrade a relay path to a direct one.
const DIRECT_WAIT: Duration = Duration::from_secs(4);

/// What the dialing side sends first.
#[derive(Debug, Serialize, Deserialize)]
struct Hello {
    device: String,
}

/// What the serving side answers with.
#[derive(Debug, Serialize, Deserialize)]
struct Reply {
    device: String,
    tasks: Vec<WireTask>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WireTask {
    pub id: String,
    pub title: String,
}

/// How the connection reached the peer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathKind {
    Direct,
    Relay,
    Unknown,
}

impl PathKind {
    pub fn as_str(self) -> &'static str {
        match self {
            PathKind::Direct => "direct",
            PathKind::Relay => "relay",
            PathKind::Unknown => "unknown",
        }
    }
}

/// The result of one pull.
#[derive(Debug, Clone)]
pub struct PullReport {
    pub peer_device: String,
    pub peer_id: String,
    pub tasks: Vec<WireTask>,
    /// Time from dialing to having the reply.
    pub elapsed: Duration,
    /// Selected path when the reply arrived.
    pub first_path: PathKind,
    /// Selected path after waiting up to [`DIRECT_WAIT`] for hole punching.
    pub final_path: PathKind,
    pub rtt: Option<Duration>,
    /// Every open path at the end, e.g. `ip 192.168.1.5:5123 (selected)`.
    pub paths: Vec<String>,
}

/// A connection the node served.
#[derive(Debug, Clone)]
pub struct Served {
    pub peer_device: String,
    pub peer_id: String,
    pub task_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Relays {
    /// n0's public relays plus n0 DNS address lookup: works across networks.
    N0,
    /// No relays and no address lookup: only addresses in the ticket are tried.
    Off,
}

pub struct SyncNode {
    router: Router,
    device: String,
    served_log: Arc<Mutex<Vec<Served>>>,
    served_tx: broadcast::Sender<Served>,
}

impl SyncNode {
    /// Bind an endpoint whose identity is the key in `key_path` (created if missing),
    /// and serve the open tasks from the database at `db_path`.
    pub async fn start(
        db_path: PathBuf,
        key_path: &Path,
        device: String,
        relays: Relays,
    ) -> Result<Self> {
        let key = load_or_create_key(key_path)?;
        let builder = match relays {
            Relays::N0 => Endpoint::builder(presets::N0),
            Relays::Off => Endpoint::builder(presets::Minimal).relay_mode(RelayMode::Disabled),
        };
        let endpoint = builder
            .secret_key(key)
            .bind()
            .await
            .context("binding iroh endpoint")?;

        let served_log = Arc::new(Mutex::new(Vec::new()));
        let (served_tx, _) = broadcast::channel(32);
        let handler = TaskServer {
            db_path,
            device: device.clone(),
            log: served_log.clone(),
            tx: served_tx.clone(),
        };
        let router = Router::builder(endpoint).accept(ALPN, handler).spawn();
        Ok(Self {
            router,
            device,
            served_log,
            served_tx,
        })
    }

    fn endpoint(&self) -> &Endpoint {
        self.router.endpoint()
    }

    pub fn id(&self) -> String {
        self.endpoint().id().to_string()
    }

    /// Wait until a home relay is connected (so the ticket carries a relay URL), up to `timeout`.
    /// Returns false on timeout; the endpoint still works on direct addresses.
    pub async fn wait_online(&self, timeout: Duration) -> bool {
        tokio::time::timeout(timeout, self.endpoint().online())
            .await
            .is_ok()
    }

    /// A pasteable address: `tt1` + hex of the endpoint address as JSON.
    pub fn ticket(&self) -> Result<String> {
        encode_ticket(&self.endpoint().addr())
    }

    /// Addresses currently in the ticket, for display.
    pub fn addrs(&self) -> Vec<String> {
        self.endpoint()
            .addr()
            .addrs
            .iter()
            .map(describe_addr)
            .collect()
    }

    /// Connections served so far (newest last).
    pub fn served(&self) -> Vec<Served> {
        self.served_log
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn subscribe_served(&self) -> broadcast::Receiver<Served> {
        self.served_tx.subscribe()
    }

    /// Dial the peer in `ticket` and fetch its open tasks.
    pub async fn pull(&self, ticket: &str) -> Result<PullReport> {
        let addr = decode_ticket(ticket)?;
        let started = Instant::now();
        let conn = self
            .endpoint()
            .connect(addr, ALPN)
            .await
            .context("connecting to peer")?;

        let (mut send, mut recv) = conn.open_bi().await.context("opening stream")?;
        let hello = serde_json::to_vec(&Hello {
            device: self.device.clone(),
        })?;
        send.write_all(&hello).await.context("sending hello")?;
        send.finish().context("finishing hello")?;
        let bytes = recv.read_to_end(MAX_MSG).await.context("reading reply")?;
        let reply: Reply = serde_json::from_slice(&bytes).context("decoding reply")?;
        let elapsed = started.elapsed();

        let first_path = selected_path(&conn).0;
        let deadline = Instant::now() + DIRECT_WAIT;
        while selected_path(&conn).0 != PathKind::Direct && Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        let (final_path, rtt) = selected_path(&conn);
        let paths = describe_paths(&conn);
        conn.close(0u32.into(), b"done");

        Ok(PullReport {
            peer_device: reply.device,
            peer_id: conn.remote_id().to_string(),
            tasks: reply.tasks,
            elapsed,
            first_path,
            final_path,
            rtt,
            paths,
        })
    }

    pub async fn shutdown(self) {
        if let Err(e) = self.router.shutdown().await {
            eprintln!("router shutdown: {e}");
        }
    }
}

#[derive(Debug, Clone)]
struct TaskServer {
    db_path: PathBuf,
    device: String,
    log: Arc<Mutex<Vec<Served>>>,
    tx: broadcast::Sender<Served>,
}

impl TaskServer {
    async fn serve(&self, conn: &Connection) -> Result<Served> {
        let (mut send, mut recv) = conn.accept_bi().await?;
        let hello: Hello = serde_json::from_slice(&recv.read_to_end(MAX_MSG).await?)?;

        // Each connection opens its own SQLite handle; WAL lets it read beside the app's.
        let db_path = self.db_path.clone();
        let tasks = tokio::task::spawn_blocking(move || -> Result<Vec<WireTask>> {
            let store = Store::open(&db_path)?;
            Ok(store
                .open_tasks()?
                .into_iter()
                .map(|t| WireTask {
                    id: t.id,
                    title: t.title,
                })
                .collect())
        })
        .await??;

        let served = Served {
            peer_device: hello.device,
            peer_id: conn.remote_id().to_string(),
            task_count: tasks.len(),
        };
        let reply = serde_json::to_vec(&Reply {
            device: self.device.clone(),
            tasks,
        })?;
        send.write_all(&reply).await?;
        send.finish()?;
        Ok(served)
    }
}

impl ProtocolHandler for TaskServer {
    async fn accept(&self, conn: Connection) -> Result<(), AcceptError> {
        let served = self
            .serve(&conn)
            .await
            .map_err(|e| AcceptError::from_boxed(e.into()))?;
        self.log
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(served.clone());
        let _ = self.tx.send(served);
        // The client closes once it has measured the path.
        conn.closed().await;
        Ok(())
    }
}

fn selected_path(conn: &Connection) -> (PathKind, Option<Duration>) {
    let paths = conn.paths();
    for p in paths.iter() {
        if p.is_selected() {
            let kind = if p.is_relay() {
                PathKind::Relay
            } else if p.is_ip() {
                PathKind::Direct
            } else {
                PathKind::Unknown
            };
            return (kind, Some(p.rtt()));
        }
    }
    (PathKind::Unknown, None)
}

fn describe_paths(conn: &Connection) -> Vec<String> {
    let paths = conn.paths();
    paths
        .iter()
        .map(|p| {
            let mark = if p.is_selected() { " (selected)" } else { "" };
            format!(
                "{}, rtt {} ms{mark}",
                describe_addr(p.remote_addr()),
                p.rtt().as_millis()
            )
        })
        .collect()
}

fn describe_addr(addr: &TransportAddr) -> String {
    match addr {
        TransportAddr::Ip(a) => format!("ip {a}"),
        TransportAddr::Relay(u) => format!("relay {u}"),
        other => format!("{other:?}"),
    }
}

fn load_or_create_key(path: &Path) -> Result<SecretKey> {
    match std::fs::read(path) {
        Ok(bytes) => {
            let arr: [u8; 32] = bytes
                .as_slice()
                .try_into()
                .with_context(|| format!("{} is not a 32-byte key", path.display()))?;
            Ok(SecretKey::from_bytes(&arr))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let key = SecretKey::generate();
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            write_private(path, &key.to_bytes())?;
            Ok(key)
        }
        Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
    }
}

#[cfg(unix)]
fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .with_context(|| format!("creating {}", path.display()))?;
    f.write_all(bytes)?;
    Ok(())
}

#[cfg(not(unix))]
fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    std::fs::write(path, bytes).with_context(|| format!("creating {}", path.display()))
}

pub fn encode_ticket(addr: &EndpointAddr) -> Result<String> {
    let json = serde_json::to_vec(addr)?;
    let mut s = String::with_capacity(TICKET_PREFIX.len() + json.len() * 2);
    s.push_str(TICKET_PREFIX);
    for b in json {
        s.push_str(&format!("{b:02x}"));
    }
    Ok(s)
}

pub fn decode_ticket(ticket: &str) -> Result<EndpointAddr> {
    let hex = ticket
        .trim()
        .strip_prefix(TICKET_PREFIX)
        .context("not a Tick Alt ticket (expected tt1…)")?;
    if hex.len() % 2 != 0 {
        bail!("ticket has odd length; was it cut off?");
    }
    let bytes = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16))
        .collect::<Result<Vec<u8>, _>>()
        .context("ticket is not hex")?;
    serde_json::from_slice(&bytes).context("ticket does not hold an endpoint address")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("tt-sync-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn key_is_persistent() {
        let dir = temp_dir("key");
        let a = load_or_create_key(&dir.join("k")).unwrap();
        let b = load_or_create_key(&dir.join("k")).unwrap();
        assert_eq!(a.public(), b.public());
    }

    #[test]
    fn ticket_round_trips() {
        let id = SecretKey::generate().public();
        let addr = EndpointAddr::new(id).with_ip_addr("192.168.1.5:4433".parse().unwrap());
        assert_eq!(decode_ticket(&encode_ticket(&addr).unwrap()).unwrap(), addr);
        assert!(decode_ticket("nope").is_err());
    }

    /// Two nodes on loopback, no relays: B pulls A's open tasks.
    #[tokio::test]
    async fn pull_over_loopback() {
        let dir = temp_dir("pull");
        let db_a = dir.join("a.db");
        {
            let store = Store::open(&db_a).unwrap();
            store.add_task("Buy milk").unwrap();
            store.add_task("Renew passport").unwrap();
        }
        let a = SyncNode::start(db_a, &dir.join("a.key"), "mac".into(), Relays::Off)
            .await
            .unwrap();
        let b = SyncNode::start(
            dir.join("b.db"),
            &dir.join("b.key"),
            "phone".into(),
            Relays::Off,
        )
        .await
        .unwrap();
        let mut served = a.subscribe_served();

        let report = b.pull(&a.ticket().unwrap()).await.unwrap();
        let mut titles: Vec<_> = report.tasks.iter().map(|t| t.title.as_str()).collect();
        titles.sort();
        assert_eq!(titles, ["Buy milk", "Renew passport"]);
        assert_eq!(report.peer_device, "mac");
        assert_eq!(report.peer_id, a.id());
        assert_eq!(report.final_path, PathKind::Direct);

        let s = tokio::time::timeout(Duration::from_secs(5), served.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(s.peer_device, "phone");
        assert_eq!(s.peer_id, b.id());
        assert_eq!(s.task_count, 2);

        a.shutdown().await;
        b.shutdown().await;
    }
}
