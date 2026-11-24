//! tt-core: the shared Rust core of Tick Alt.
//!
//! Owns the data model and the SQLite database. UIs (Flutter, the Glance
//! widget, the `tt` CLI) only call into this crate.

mod store;

pub use store::Store;

/// Errors returned by the core.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
