//! In-process sync for the phone app (M4, plan/16): Android has no utre-daemon, so the app
//! runs the sync node itself. One per process, shared by the Flutter side (`TaskDb`) and
//! the Kotlin background job (`kotlin::background_sync`), so only one node ever uses the
//! device key. Outside `api/` so flutter_rust_bridge doesn't scan it.

use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

use tokio::sync::broadcast;
use utre_sync::Relays;
use utre_sync::service::SyncService;

pub struct Phone {
    pub rt: tokio::runtime::Runtime,
    pub svc: Arc<SyncService>,
    edits: broadcast::Sender<()>,
}

impl Phone {
    /// A local edit happened: sync a few seconds from now.
    pub fn edited(&self) {
        let _ = self.edits.send(());
    }
}

fn slot() -> &'static Mutex<Option<Arc<Phone>>> {
    static SLOT: OnceLock<Mutex<Option<Arc<Phone>>>> = OnceLock::new();
    SLOT.get_or_init(Mutex::default)
}

/// Start the node on `db` with the key at `key` (or return the one already running) and
/// its auto-sync: shortly after starting, every 90 s, and a few seconds after edits.
pub fn start(db: &Path, key: &Path, device: String) -> anyhow::Result<Arc<Phone>> {
    let mut slot = slot().lock().unwrap_or_else(|e| e.into_inner());
    if let Some(p) = &*slot {
        return Ok(p.clone());
    }
    // iroh needs a tokio runtime; frb's and Kotlin's threads don't provide one.
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    let svc = rt.block_on(SyncService::start_with_key(db, key, device, Relays::N0))?;
    let (edits, _) = broadcast::channel(16);
    {
        let _in_runtime = rt.enter();
        svc.spawn_auto(edits.subscribe(), || {});
    }
    let phone = Arc::new(Phone { rt, svc, edits });
    *slot = Some(phone.clone());
    Ok(phone)
}
