//! tt-core: the shared Rust core of Tick Alt.
//!
//! Owns the data model and the SQLite database. UIs (Flutter, the Glance
//! widget, the `tt` CLI) only call into this crate. Every write becomes ops in
//! the op-log first (see `oplog`), so sync can replay them on other devices.

mod checklist;
mod dates;
pub mod filter;
pub mod hlc;
mod inbox;
pub mod oplog;
mod organize;
mod recur;
mod reminder;
mod replica;
mod store;
mod task;

pub use checklist::ChecklistItem;
pub use dates::{Day, is_overdue, local_day, local_tz, offset_label, parse_when, when_label};
pub use filter::{Cond, Filter, FilterRule, SavedFilter};
pub use organize::{List, Tag};
pub use recur::{Completion, Repeat};
pub use reminder::{Reminder, ReminderFire, ReminderKind};
pub use replica::{Peer, Removal, SharedPeer, TrustChange, TrustList, VersionVector};
pub use store::Store;
pub use task::{MAX_DEPTH, NewTask, RepeatMode, Schedule, Status, Task};

/// Errors returned by the core.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("{0}")]
    Invalid(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("corrupt data: {0}")]
    Corrupt(String),
    #[error("bad JSON: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
