//! The Kotlin-facing API (UniFFI), used by the Android home-screen widget.
//!
//! It lives in the same library as the Dart API so the app ships one copy of
//! the core and SQLite. The widget and the Flutter UI each open their own
//! connection to the same database file; SQLite in WAL mode handles that.

use std::sync::{Mutex, MutexGuard};

use tt_core::{Store, Task};

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum CoreError {
    #[error("{msg}")]
    Failed { msg: String },
}

impl From<tt_core::Error> for CoreError {
    fn from(e: tt_core::Error) -> Self {
        CoreError::Failed { msg: e.to_string() }
    }
}

/// A task as the widget sees it.
#[derive(uniffi::Record)]
pub struct WidgetTask {
    pub id: String,
    pub title: String,
    /// 0 none, 1 low, 2 medium, 3 high.
    pub priority: u8,
    /// Epoch milliseconds.
    pub due_at: Option<i64>,
}

impl From<Task> for WidgetTask {
    fn from(t: Task) -> Self {
        Self {
            id: t.id,
            title: t.title,
            priority: t.priority,
            due_at: t.due_at,
        }
    }
}

#[derive(uniffi::Object)]
pub struct WidgetDb {
    store: Mutex<Store>,
}

#[uniffi::export]
impl WidgetDb {
    /// Open (or create) the database file at `path`.
    #[uniffi::constructor]
    pub fn open(path: String) -> Result<Self, CoreError> {
        Ok(Self {
            store: Mutex::new(Store::open(path)?),
        })
    }

    pub fn inbox(&self) -> Result<Vec<WidgetTask>, CoreError> {
        Ok(self.lock().inbox()?.into_iter().map(Into::into).collect())
    }

    pub fn add(&self, title: String) -> Result<WidgetTask, CoreError> {
        Ok(self.lock().add_task(&title)?.into())
    }

    /// Returns false if the task was already done.
    pub fn complete(&self, id: String) -> Result<bool, CoreError> {
        Ok(self.lock().complete_task(&id)?)
    }
}

impl WidgetDb {
    fn lock(&self) -> MutexGuard<'_, Store> {
        self.store.lock().unwrap_or_else(|e| e.into_inner())
    }
}
