//! tt-api: the one way into a Tick Alt database, local or through the daemon.
//!
//! - [`views`]: what the UIs show (sidebar, task rows, detail), computed from the core.
//! - [`proto`]: the request protocol and [`proto::handle`], which runs a request on a store.
//! - [`Api`]: typed calls that go either straight to an open [`Store`] or over the
//!   daemon's socket ([`client`]). The method names and arguments follow `Store`'s.
//! - `server` (feature `server`): the daemon side, a DB thread plus the socket listener.
//!
//! See plan/15 M2 Desktop Log for why: on desktop the daemon owns the database (plan/03
//! rule 3); the app and `tt` are its clients and fall back to opening the file directly.

pub mod client;
pub mod paths;
pub mod proto;
#[cfg(feature = "server")]
pub mod server;
pub mod views;

use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use serde::de::DeserializeOwned;
use tt_core::Store;

pub use client::Client;
pub use proto::{Event, Hello, Request, TzName};
pub use views::{
    ChecklistRow, NavItem, ReminderRow, Sidebar, TagChip, TaskDetail, TaskRow, TaskView, ViewKind,
};

/// Calls into the core, in-process or through the daemon.
pub struct Api {
    backend: Backend,
}

enum Backend {
    Local(Mutex<Store>),
    Remote(Client),
}

impl Api {
    /// Use an open store directly.
    pub fn local(store: Store) -> Api {
        Api {
            backend: Backend::Local(Mutex::new(store)),
        }
    }

    pub fn remote(client: Client) -> Api {
        Api {
            backend: Backend::Remote(client),
        }
    }

    /// The daemon if it serves `db`, else the file opened directly.
    pub fn connect_or_open(db: &Path) -> anyhow::Result<Api> {
        Ok(match Client::for_db(db) {
            Some(c) => Api::remote(c),
            None => Api::local(Store::open(db)?),
        })
    }

    pub fn is_remote(&self) -> bool {
        matches!(self.backend, Backend::Remote(_))
    }

    /// Send any request and decode its reply.
    pub fn call<T: DeserializeOwned>(&self, req: Request) -> anyhow::Result<T> {
        let value = match &self.backend {
            Backend::Local(store) => proto::handle(&lock(store), req)?,
            Backend::Remote(client) => client.call(&req)?,
        };
        Ok(serde_json::from_value(value)?)
    }
}

fn lock(store: &Mutex<Store>) -> MutexGuard<'_, Store> {
    // A panic mid-call can't leave SQLite half-written, so a poisoned lock is safe to reuse.
    store.lock().unwrap_or_else(|e| e.into_inner())
}
