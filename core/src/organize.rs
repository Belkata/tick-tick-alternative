//! Lists, tags and the task fields that move a task out of Inbox.

use rusqlite::params;
use uuid::Uuid;

use crate::task::now_ms;
use crate::{Result, Store};

impl Store {
    /// Create a flat list. Returns its id.
    pub fn create_list(&self, name: &str) -> Result<String> {
        let id = Uuid::now_v7().to_string();
        self.conn
            .execute("INSERT INTO list (id, name) VALUES (?1, ?2)", params![id, name])?;
        Ok(id)
    }

    /// Create a tag, optionally nested under `parent_id`. Returns its id.
    pub fn create_tag(&self, name: &str, parent_id: Option<&str>) -> Result<String> {
        let id = Uuid::now_v7().to_string();
        self.conn.execute(
            "INSERT INTO tag (id, name, parent_id) VALUES (?1, ?2, ?3)",
            params![id, name, parent_id],
        )?;
        Ok(id)
    }

    pub fn tag_task(&self, task_id: &str, tag_id: &str) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO task_tag (task_id, tag_id) VALUES (?1, ?2)",
            params![task_id, tag_id],
        )?;
        self.touch(task_id)
    }

    pub fn untag_task(&self, task_id: &str, tag_id: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM task_tag WHERE task_id = ?1 AND tag_id = ?2",
            params![task_id, tag_id],
        )?;
        self.touch(task_id)
    }

    /// Move a task into a list, or out of every list with `None`.
    pub fn set_task_list(&self, task_id: &str, list_id: Option<&str>) -> Result<()> {
        self.conn.execute(
            "UPDATE task SET list_id = ?2, updated_at = ?3 WHERE id = ?1",
            params![task_id, list_id, now_ms()],
        )?;
        Ok(())
    }

    /// Set or clear the due time (epoch ms).
    pub fn set_task_due(&self, task_id: &str, due_at: Option<i64>) -> Result<()> {
        self.conn.execute(
            "UPDATE task SET due_at = ?2, updated_at = ?3 WHERE id = ?1",
            params![task_id, due_at, now_ms()],
        )?;
        Ok(())
    }

    fn touch(&self, task_id: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE task SET updated_at = ?2 WHERE id = ?1",
            params![task_id, now_ms()],
        )?;
        Ok(())
    }
}
