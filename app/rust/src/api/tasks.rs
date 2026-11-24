//! The Dart-facing API. Keep this thin: rules live in tt-core.

use std::sync::Mutex;

use flutter_rust_bridge::frb;
use tt_core::{Store, Task};

/// A task as the UI sees it.
pub struct TaskItem {
    pub id: String,
    pub title: String,
    /// 0 none, 1 low, 2 medium, 3 high.
    pub priority: u8,
    /// Epoch milliseconds.
    pub due_at: Option<i64>,
}

impl From<Task> for TaskItem {
    fn from(t: Task) -> Self {
        Self {
            id: t.id,
            title: t.title,
            priority: t.priority,
            due_at: t.due_at,
        }
    }
}

/// An open database. SQLite connections aren't `Sync`, so calls are serialised.
#[frb(opaque)]
pub struct TaskDb {
    store: Mutex<Store>,
}

impl TaskDb {
    /// Open (or create) the database file at `path`.
    pub fn open(path: String) -> anyhow::Result<TaskDb> {
        Ok(Self {
            store: Mutex::new(Store::open(path)?),
        })
    }

    pub fn inbox(&self) -> anyhow::Result<Vec<TaskItem>> {
        Ok(self.lock().inbox()?.into_iter().map(Into::into).collect())
    }

    pub fn add(&self, title: String) -> anyhow::Result<TaskItem> {
        Ok(self.lock().add_task(&title)?.into())
    }

    /// Returns false if the task was already done.
    pub fn complete(&self, id: String) -> anyhow::Result<bool> {
        Ok(self.lock().complete_task(&id)?)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Store> {
        // A panic mid-call can't leave SQLite half-written, so a poisoned lock is safe to reuse.
        self.store.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[frb(init)]
pub fn init_app() {
    flutter_rust_bridge::setup_default_user_utils();
}
