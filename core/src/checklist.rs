//! Checklist items: lightweight steps inside a task (no dates, tags or reminders).

use rusqlite::{Connection, OptionalExtension};
use uuid::Uuid;

use crate::oplog::Entity;
use crate::task::require_task;
use crate::{Error, Result, Store};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChecklistItem {
    pub id: String,
    pub task_id: String,
    pub title: String,
    pub done: bool,
}

fn require_item(conn: &Connection, id: &str) -> Result<()> {
    let ok: Option<i64> = conn
        .query_row(
            "SELECT 1 FROM checklist_item WHERE id = ?1 AND deleted = 0",
            [id],
            |r| r.get(0),
        )
        .optional()?;
    ok.map(|_| ())
        .ok_or_else(|| Error::NotFound(format!("checklist item {id}")))
}

impl Store {
    /// Append an item to a task's checklist. Returns its id.
    pub fn add_checklist_item(&self, task_id: &str, title: &str) -> Result<String> {
        let title = title.trim();
        if title.is_empty() {
            return Err(Error::Invalid("a checklist item needs a title".into()));
        }
        self.write(|w| {
            require_task(w.conn, task_id)?;
            // UUIDv7 ids sort by creation, so new items go last until reordered.
            let id = Uuid::now_v7().to_string();
            w.set(Entity::Checklist, &id, "task_id", task_id)?;
            w.set(Entity::Checklist, &id, "title", title)?;
            Ok(id)
        })
    }

    pub fn set_checklist_done(&self, id: &str, done: bool) -> Result<()> {
        self.write(|w| {
            require_item(w.conn, id)?;
            w.set(Entity::Checklist, id, "done", done)
        })
    }

    pub fn rename_checklist_item(&self, id: &str, title: &str) -> Result<()> {
        let title = title.trim();
        if title.is_empty() {
            return Err(Error::Invalid("a checklist item needs a title".into()));
        }
        self.write(|w| {
            require_item(w.conn, id)?;
            w.set(Entity::Checklist, id, "title", title)
        })
    }

    pub fn remove_checklist_item(&self, id: &str) -> Result<()> {
        self.write(|w| {
            require_item(w.conn, id)?;
            w.set(Entity::Checklist, id, "deleted", true)
        })
    }

    pub fn checklist(&self, task_id: &str) -> Result<Vec<ChecklistItem>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, task_id, title, done FROM checklist_item \
             WHERE task_id = ?1 AND deleted = 0 ORDER BY sort_key, id",
        )?;
        let rows = stmt.query_map([task_id], |r| {
            Ok(ChecklistItem {
                id: r.get(0)?,
                task_id: r.get(1)?,
                title: r.get(2)?,
                done: r.get(3)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}

#[cfg(test)]
mod tests {
    use crate::Store;

    #[test]
    fn add_tick_rename_remove() {
        let s = Store::open_in_memory().unwrap();
        let t = s.add_task("trip").unwrap();
        let a = s.add_checklist_item(&t.id, "passport").unwrap();
        let b = s.add_checklist_item(&t.id, "tickets").unwrap();
        s.set_checklist_done(&a, true).unwrap();
        s.rename_checklist_item(&b, "train tickets").unwrap();
        let items = s.checklist(&t.id).unwrap();
        assert_eq!(items.len(), 2);
        assert!(items[0].done);
        assert_eq!(items[1].title, "train tickets");
        s.remove_checklist_item(&a).unwrap();
        assert_eq!(s.checklist(&t.id).unwrap().len(), 1);
        assert!(s.set_checklist_done(&a, false).is_err());
    }
}
