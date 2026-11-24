use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{OptionalExtension, Row, params};
use uuid::Uuid;

use crate::{Result, Store};

/// Lifecycle state of a task.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Open,
    Done,
    /// In Trash.
    Deleted,
}

impl Status {
    fn as_str(self) -> &'static str {
        match self {
            Status::Open => "open",
            Status::Done => "done",
            Status::Deleted => "deleted",
        }
    }

    fn parse(s: &str) -> Self {
        match s {
            "done" => Status::Done,
            "deleted" => Status::Deleted,
            _ => Status::Open,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub notes: String,
    pub list_id: Option<String>,
    pub parent_id: Option<String>,
    /// 0 none, 1 low, 2 medium, 3 high.
    pub priority: u8,
    /// Epoch milliseconds.
    pub start_at: Option<i64>,
    /// Epoch milliseconds.
    pub due_at: Option<i64>,
    pub status: Status,
    pub completed_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

pub(crate) const TASK_COLUMNS: &str = "id, title, notes, list_id, parent_id, priority, \
     start_at, due_at, status, completed_at, created_at, updated_at";

impl Task {
    pub(crate) fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            title: row.get(1)?,
            notes: row.get(2)?,
            list_id: row.get(3)?,
            parent_id: row.get(4)?,
            priority: row.get(5)?,
            start_at: row.get(6)?,
            due_at: row.get(7)?,
            status: Status::parse(&row.get::<_, String>(8)?),
            completed_at: row.get(9)?,
            created_at: row.get(10)?,
            updated_at: row.get(11)?,
        })
    }
}

pub(crate) fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

impl Store {
    /// Create a task with only a title. With no list, tag or date it lands in Inbox.
    pub fn add_task(&self, title: &str) -> Result<Task> {
        let id = Uuid::now_v7().to_string();
        let now = now_ms();
        self.conn.execute(
            "INSERT INTO task (id, title, created_at, updated_at) VALUES (?1, ?2, ?3, ?3)",
            params![id, title, now],
        )?;
        Ok(self.get_task(&id)?.expect("task just inserted"))
    }

    pub fn get_task(&self, id: &str) -> Result<Option<Task>> {
        let sql = format!("SELECT {TASK_COLUMNS} FROM task WHERE id = ?1");
        Ok(self
            .conn
            .query_row(&sql, [id], Task::from_row)
            .optional()?)
    }

    /// Resolve a short id (the end of a UUID, as the CLI prints it) to a full id.
    /// Returns `None` if nothing matches or the suffix is ambiguous.
    pub fn resolve_task_id(&self, suffix: &str) -> Result<Option<String>> {
        if suffix.is_empty() || !suffix.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
            return Ok(None);
        }
        let mut stmt = self
            .conn
            .prepare("SELECT id FROM task WHERE id LIKE '%' || ?1 LIMIT 2")?;
        let ids: Vec<String> = stmt
            .query_map([suffix.to_ascii_lowercase()], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(match ids.as_slice() {
            [one] => Some(one.clone()),
            _ => None,
        })
    }

    /// Open tasks, oldest first.
    pub fn open_tasks(&self) -> Result<Vec<Task>> {
        let sql = format!(
            "SELECT {TASK_COLUMNS} FROM task WHERE status = 'open' ORDER BY created_at, id"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([], Task::from_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Mark a task done. Returns false if no open task has that id.
    pub fn complete_task(&self, id: &str) -> Result<bool> {
        self.set_status(id, Status::Done)
    }

    /// Move a task to Trash. Returns false if the task doesn't exist or is already there.
    pub fn trash_task(&self, id: &str) -> Result<bool> {
        self.set_status(id, Status::Deleted)
    }

    fn set_status(&self, id: &str, status: Status) -> Result<bool> {
        let now = now_ms();
        let completed_at = (status == Status::Done).then_some(now);
        let changed = self.conn.execute(
            "UPDATE task SET status = ?2, completed_at = ?3, updated_at = ?4 \
             WHERE id = ?1 AND status != ?2",
            params![id, status.as_str(), completed_at, now],
        )?;
        Ok(changed == 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_get_complete() {
        let store = Store::open_in_memory().unwrap();
        let t = store.add_task("Buy milk").unwrap();
        assert_eq!(t.title, "Buy milk");
        assert_eq!(t.status, Status::Open);

        assert!(store.complete_task(&t.id).unwrap());
        assert!(!store.complete_task(&t.id).unwrap(), "already done");

        let done = store.get_task(&t.id).unwrap().unwrap();
        assert_eq!(done.status, Status::Done);
        assert!(done.completed_at.is_some());
        assert!(store.open_tasks().unwrap().is_empty());
    }

    #[test]
    fn trash_clears_completed_at() {
        let store = Store::open_in_memory().unwrap();
        let t = store.add_task("x").unwrap();
        store.complete_task(&t.id).unwrap();
        assert!(store.trash_task(&t.id).unwrap());
        let trashed = store.get_task(&t.id).unwrap().unwrap();
        assert_eq!(trashed.status, Status::Deleted);
        assert_eq!(trashed.completed_at, None);
    }
}
