//! The daemon side: one thread owns the store; the socket listener and the daemon's own
//! tray and reminder loops send it requests. Every write, and every commit another
//! process makes to the file, bumps the change version and is broadcast to subscribers.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{broadcast, oneshot};
use tt_core::Store;

use crate::proto::{self, Event, Hello, Reply, Request};

/// How often the DB thread checks for commits by other processes.
const POLL: Duration = Duration::from_secs(1);

struct Job {
    req: Request,
    reply: oneshot::Sender<Result<Value, String>>,
}

/// A handle to the DB thread. Cheap to clone.
#[derive(Clone)]
pub struct Db {
    jobs: mpsc::Sender<Job>,
    events: broadcast::Sender<u64>,
    version: Arc<AtomicU64>,
    path: Arc<PathBuf>,
}

impl Db {
    /// Open the store at `path` and start its thread.
    pub fn open(path: &Path) -> anyhow::Result<Db> {
        let store = Store::open(path)?;
        let path = std::fs::canonicalize(path)?;
        let (jobs, rx) = mpsc::channel::<Job>();
        let (events, _) = broadcast::channel(64);
        let version = Arc::new(AtomicU64::new(0));
        let db = Db {
            jobs,
            events: events.clone(),
            version: version.clone(),
            path: Arc::new(path),
        };
        std::thread::Builder::new()
            .name("tt-db".into())
            .spawn(move || db_thread(store, rx, events, version))?;
        Ok(db)
    }

    /// Run a request on the DB thread.
    pub async fn call(&self, req: Request) -> Result<Value, String> {
        let (reply, rx) = oneshot::channel();
        self.jobs
            .send(Job { req, reply })
            .map_err(|_| "the database thread stopped".to_string())?;
        rx.await
            .map_err(|_| "the database thread stopped".to_string())?
    }

    /// Typed convenience for the daemon's own loops.
    pub async fn get<T: serde::de::DeserializeOwned>(&self, req: Request) -> anyhow::Result<T> {
        let v = self.call(req).await.map_err(anyhow::Error::msg)?;
        Ok(serde_json::from_value(v)?)
    }

    /// Change versions, one per write (or batch of outside writes).
    pub fn subscribe(&self) -> broadcast::Receiver<u64> {
        self.events.subscribe()
    }

    pub fn hello(&self) -> Hello {
        Hello {
            db: self.path.to_string_lossy().into_owned(),
            version: self.version.load(Ordering::SeqCst),
            pid: std::process::id(),
        }
    }
}

fn db_thread(
    store: Store,
    jobs: mpsc::Receiver<Job>,
    events: broadcast::Sender<u64>,
    version: Arc<AtomicU64>,
) {
    let changed = || {
        let v = version.fetch_add(1, Ordering::SeqCst) + 1;
        let _ = events.send(v); // No subscribers is fine.
    };
    let mut seen = store.data_version().unwrap_or(0);
    loop {
        match jobs.recv_timeout(POLL) {
            Ok(Job { req, reply }) => {
                let write = req.is_write();
                let result = proto::handle(&store, req).map_err(|e| e.to_string());
                if write && result.is_ok() {
                    changed();
                }
                let _ = reply.send(result);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
        }
        // Another process committed (e.g. `tt` opened the file while no daemon ran).
        if let Ok(dv) = store.data_version()
            && dv != seen
        {
            seen = dv;
            changed();
        }
    }
}

/// Bind the socket at `path`. A stale socket file from a crashed daemon is replaced;
/// a live daemon already listening there is an error.
pub fn bind(path: &Path) -> anyhow::Result<UnixListener> {
    if path.exists() {
        if std::os::unix::net::UnixStream::connect(path).is_ok() {
            anyhow::bail!("a daemon is already listening on {}", path.display());
        }
        std::fs::remove_file(path)?;
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let listener = UnixListener::bind(path)?;
    // Only this user may talk to it (the runtime dir is 0700 already; the macOS fallback isn't).
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(listener)
}

/// Accept connections until the listener fails.
pub async fn serve(listener: UnixListener, db: Db) -> anyhow::Result<()> {
    loop {
        let (stream, _) = listener.accept().await?;
        let db = db.clone();
        tokio::spawn(async move {
            if let Err(e) = connection(stream, db).await {
                eprintln!("tt-daemon: connection: {e:#}");
            }
        });
    }
}

async fn connection(stream: UnixStream, db: Db) -> anyhow::Result<()> {
    let (read, mut write) = stream.into_split();
    let mut lines = BufReader::new(read).lines();
    while let Some(line) = lines.next_line().await? {
        let reply = match serde_json::from_str::<Request>(&line) {
            Err(e) => Reply::Err(format!("bad request: {e}")),
            Ok(Request::Hello) => Reply::Ok(serde_json::to_value(db.hello())?),
            Ok(Request::Subscribe) => return events(write, db).await,
            Ok(req) => match db.call(req).await {
                Ok(v) => Reply::Ok(v),
                Err(e) => Reply::Err(e),
            },
        };
        let mut out = serde_json::to_string(&reply)?;
        out.push('\n');
        write.write_all(out.as_bytes()).await?;
    }
    Ok(())
}

/// A subscribed connection: push one line per change until the client hangs up.
async fn events(mut write: tokio::net::unix::OwnedWriteHalf, db: Db) -> anyhow::Result<()> {
    let mut rx = db.subscribe();
    loop {
        let version = match rx.recv().await {
            Ok(v) => v,
            // Fell behind: just report the latest; clients only re-query anyway.
            Err(broadcast::error::RecvError::Lagged(_)) => db.version.load(Ordering::SeqCst),
            Err(broadcast::error::RecvError::Closed) => return Ok(()),
        };
        let mut out = serde_json::to_string(&Event::Changed { version })?;
        out.push('\n');
        if write.write_all(out.as_bytes()).await.is_err() {
            return Ok(()); // The client went away.
        }
    }
}
