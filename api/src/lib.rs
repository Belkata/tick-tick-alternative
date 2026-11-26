//! utre-api: the one way into a Utre database, local or through the daemon.
//!
//! - [`views`]: what the UIs show (sidebar, task rows, detail), computed from the core.
//! - [`proto`]: the request protocol and [`proto::handle`], which runs a request on a store.
//! - [`Api`]: typed calls that go either straight to an open [`Store`] or over the
//!   daemon's socket ([`client`]). The method names and arguments follow `Store`'s.
//! - `server` (feature `server`): the daemon side, a DB thread plus the socket listener.
//!
//! See plan/15 M2 Desktop Log for why: on desktop the daemon owns the database (plan/03
//! rule 3); the app and `utre` are its clients and fall back to opening the file directly.

pub mod client;
pub mod paths;
pub mod proto;
#[cfg(feature = "server")]
pub mod server;
pub mod views;

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use serde::de::DeserializeOwned;
use utre_core::Store;

pub use client::{Client, Disconnected};
pub use proto::{
    Event, GooglePushStatus, GoogleStatus, Hello, Request, SyncOutcome, SyncStatus, TzName,
};
pub use views::{
    CalendarItem, ChecklistRow, NavItem, ReminderRow, Sidebar, TagChip, TaskDetail, TaskRow,
    TaskView, ViewKind,
};

/// Calls into the core, in-process or through the daemon.
///
/// Made with [`Api::connect_or_open`], it survives the daemon coming and going (systemd
/// restarts it on failure): a lost daemon means reconnecting, or opening the file directly
/// when none is back; [`Api::upgrade`] moves back to the daemon when it returns.
pub struct Api {
    backend: Mutex<Backend>,
    /// The file behind it, for recovering; `None` for [`Api::local`] (nothing to recover).
    db: Option<PathBuf>,
}

enum Backend {
    Local { store: Store, data_version: i64 },
    Remote(Client),
}

impl Backend {
    fn local(store: Store) -> Backend {
        let data_version = store.data_version().unwrap_or(0);
        Backend::Local {
            store,
            data_version,
        }
    }
}

impl Api {
    /// Use an open store directly.
    pub fn local(store: Store) -> Api {
        Api {
            backend: Mutex::new(Backend::local(store)),
            db: None,
        }
    }

    pub fn remote(client: Client) -> Api {
        Api {
            backend: Mutex::new(Backend::Remote(client)),
            db: None,
        }
    }

    /// The daemon if it serves `db`, else the file opened directly.
    pub fn connect_or_open(db: &Path) -> anyhow::Result<Api> {
        let backend = match Client::for_db(db) {
            Some(c) => Backend::Remote(c),
            None => Backend::local(Store::open(db)?),
        };
        Ok(Api {
            backend: Mutex::new(backend),
            db: Some(db.to_path_buf()),
        })
    }

    pub fn is_remote(&self) -> bool {
        matches!(*lock(&self.backend), Backend::Remote(_))
    }

    /// Send any request and decode its reply.
    ///
    /// If the daemon is gone, recover (reconnect, else open the file) and retry when that's
    /// safe: always for reads, and for writes only if the request never reached it. A write
    /// the daemon may have applied before dying returns the error instead of risking a repeat.
    pub fn call<T: DeserializeOwned>(&self, req: Request) -> anyhow::Result<T> {
        let mut backend = lock(&self.backend);
        let value = match &*backend {
            Backend::Local { store, .. } => proto::handle(store, req)?,
            Backend::Remote(client) => match client.call(&req) {
                Ok(v) => v,
                Err(e) => {
                    let Some(gone) = e.downcast_ref::<Disconnected>().copied() else {
                        return Err(e);
                    };
                    let Some(db) = &self.db else { return Err(e) };
                    *backend = recover(db)?;
                    if gone.sent && req.is_write() {
                        return Err(e);
                    }
                    match &*backend {
                        Backend::Local { store, .. } => proto::handle(store, req)?,
                        Backend::Remote(client) => client.call(&req)?,
                    }
                }
            },
        };
        Ok(serde_json::from_value(value)?)
    }

    // ----- Sync (answered by the daemon's sync node) -----

    /// A one-time pairing ticket (`tt2…`) for this device, valid for 5 minutes.
    pub fn sync_pair(&self) -> anyhow::Result<String> {
        self.call(Request::SyncPair)
    }

    /// Pair with the device that made `ticket`, then sync with it.
    pub fn sync_join(&self, ticket: impl Into<String>) -> anyhow::Result<SyncOutcome> {
        self.call(Request::SyncJoin {
            ticket: ticket.into(),
        })
    }

    /// Sync with every paired device now.
    pub fn sync_now(&self) -> anyhow::Result<Vec<SyncOutcome>> {
        self.call(Request::SyncNow)
    }

    pub fn sync_status(&self) -> anyhow::Result<SyncStatus> {
        self.call(Request::SyncStatus)
    }

    // ----- Google (answered by the daemon's Google service) -----

    pub fn google_status(&self) -> anyhow::Result<GoogleStatus> {
        self.call(Request::GoogleStatus)
    }

    /// Opens the browser sign-in; returns its URL (to show if the browser didn't open).
    pub fn google_sign_in(&self) -> anyhow::Result<String> {
        self.call(Request::GoogleSignIn)
    }

    pub fn google_sign_out(&self) -> anyhow::Result<()> {
        self.call(Request::GoogleSignOut)
    }

    /// Fetch Google's calendars and events now.
    pub fn google_sync(&self) -> anyhow::Result<()> {
        self.call(Request::GoogleSync)
    }

    /// What pushing tasks to Google did last.
    pub fn google_push_status(&self) -> anyhow::Result<GooglePushStatus> {
        self.call(Request::GooglePushStatus)
    }

    /// Push tasks to Google's "Tasks" calendar now.
    pub fn google_push_now(&self) -> anyhow::Result<GooglePushStatus> {
        self.call(Request::GooglePushNow)
    }

    /// Opened directly but the daemon now serves this file: switch to it. True if it switched.
    pub fn upgrade(&self) -> bool {
        let Some(db) = &self.db else { return false };
        let mut backend = lock(&self.backend);
        if matches!(*backend, Backend::Remote(_)) {
            return false;
        }
        match Client::for_db(db) {
            Some(c) => {
                *backend = Backend::Remote(c);
                true
            }
            None => false,
        }
    }

    /// Opened directly: did another process commit to the file since the last check?
    /// (Through the daemon, its events say so instead.)
    pub fn outside_change(&self) -> bool {
        match &mut *lock(&self.backend) {
            Backend::Local {
                store,
                data_version,
            } => match store.data_version() {
                Ok(v) if v != *data_version => {
                    *data_version = v;
                    true
                }
                _ => false,
            },
            Backend::Remote(_) => false,
        }
    }
}

/// After losing the daemon: a restarted one if it's back already, else the file itself.
fn recover(db: &Path) -> anyhow::Result<Backend> {
    Ok(match Client::for_db(db) {
        Some(c) => Backend::Remote(c),
        None => Backend::local(Store::open(db)?),
    })
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    // A panic mid-call can't leave SQLite half-written, so a poisoned lock is safe to reuse.
    m.lock().unwrap_or_else(|e| e.into_inner())
}
