//! How list, tag, filter and Inbox views are sorted, and manual task order.
//!
//! A list's or tag's sort is a synced JSON array of sort keys, like `filter.sort`
//! (empty means the default: due date, then priority). The Inbox isn't stored, so its
//! sort is per device, in `meta`. Manual order is each task's synced `sort_key`:
//! one order shared by every view, arranged within whichever view a task is moved in.

use rusqlite::{Connection, OptionalExtension};

use crate::oplog::Entity;
use crate::order::{key_between, spread_keys};
use crate::organize::{require_list, require_tag};
use crate::task::{TASK_COLUMNS, query_tasks, require_task};
use crate::{Error, Result, Store, Task};

/// Every sort key the core knows, as stored in a view's sort (first wins).
pub const SORT_KEYS: &[&str] = &[
    "due_asc",
    "due_desc",
    "start_asc",
    "priority_desc",
    "priority_asc",
    "created_asc",
    "created_desc",
    "title_asc",
    "manual",
];

/// The Inbox's sort when none is chosen: oldest first, as it always was.
pub(crate) const INBOX_DEFAULT: &str = "created_asc";

/// Reject unknown keys before saving a sort.
pub(crate) fn check_sort(sort: &[String]) -> Result<()> {
    match sort.iter().find(|k| !SORT_KEYS.contains(&k.as_str())) {
        Some(k) => Err(Error::Invalid(format!("unknown sort '{k}'"))),
        None => Ok(()),
    }
}

/// A stored sort as this build can use it: keys from a newer build (or junk from a
/// peer) are dropped rather than breaking the view.
pub(crate) fn known_sort(json: &str) -> Vec<String> {
    let keys: Vec<String> = serde_json::from_str(json).unwrap_or_default();
    keys.into_iter()
        .filter(|k| SORT_KEYS.contains(&k.as_str()))
        .collect()
}

fn stored_sort(conn: &Connection, table: &str, id: &str) -> Result<Vec<String>> {
    // `table` is one of our own names, never input.
    let json: Option<String> = conn
        .query_row(
            &format!("SELECT sort FROM {table} WHERE id = ?1 AND deleted = 0"),
            [id],
            |r| r.get(0),
        )
        .optional()?;
    json.map(|j| known_sort(&j))
        .ok_or_else(|| Error::NotFound(format!("{table} {id}")))
}

impl Store {
    /// A list's saved sort; empty for the default.
    pub fn list_sort(&self, id: &str) -> Result<Vec<String>> {
        stored_sort(&self.conn, "list", id)
    }

    /// A tag's saved sort; empty for the default.
    pub fn tag_sort(&self, id: &str) -> Result<Vec<String>> {
        stored_sort(&self.conn, "tag", id)
    }

    pub fn set_list_sort(&self, id: &str, sort: &[String]) -> Result<()> {
        check_sort(sort)?;
        self.write(|w| {
            require_list(w.conn, id)?;
            w.set(Entity::List, id, "sort", serde_json::to_value(sort)?)
        })
    }

    pub fn set_tag_sort(&self, id: &str, sort: &[String]) -> Result<()> {
        check_sort(sort)?;
        self.write(|w| {
            require_tag(w.conn, id)?;
            w.set(Entity::Tag, id, "sort", serde_json::to_value(sort)?)
        })
    }

    /// Change only a saved filter's sort.
    pub fn set_filter_sort(&self, id: &str, sort: &[String]) -> Result<()> {
        check_sort(sort)?;
        if self.find_filter(id)?.is_none() {
            return Err(Error::NotFound(format!("filter {id}")));
        }
        self.write(|w| w.set(Entity::Filter, id, "sort", serde_json::to_value(sort)?))
    }

    /// This device's Inbox sort; empty for the default (oldest first).
    pub fn inbox_sort(&self) -> Result<Vec<String>> {
        let json: Option<String> = self
            .conn
            .query_row("SELECT value FROM meta WHERE key = 'inbox_sort'", [], |r| {
                r.get(0)
            })
            .optional()?;
        Ok(json.map(|j| known_sort(&j)).unwrap_or_default())
    }

    /// Set this device's Inbox sort. Local only: it isn't an op and never syncs.
    pub fn set_inbox_sort(&self, sort: &[String]) -> Result<()> {
        check_sort(sort)?;
        self.conn.execute(
            "INSERT INTO meta (key, value) VALUES ('inbox_sort', ?1) \
             ON CONFLICT (key) DO UPDATE SET value = excluded.value",
            [serde_json::to_string(sort)?],
        )?;
        Ok(())
    }

    /// Inbox tasks in the given order (empty: oldest first).
    pub fn inbox_sorted(&self, sort: &[String]) -> Result<Vec<Task>> {
        let default = [INBOX_DEFAULT.to_string()];
        let sort = if sort.is_empty() { &default[..] } else { sort };
        let sql = format!(
            "SELECT {TASK_COLUMNS} FROM task WHERE {} ORDER BY {}",
            crate::inbox::INBOX_WHERE,
            crate::filter::order_by(sort)?
        );
        query_tasks(&self.conn, &sql, [])
    }

    /// Arrange a task in manual order: right before `before`, or last with `None`,
    /// among `shown`, the ids of the view it's moved in, in their manual order.
    /// Usually one op; if the view's tasks were never arranged (or tie), they're all
    /// numbered in the order they show now.
    pub fn place_task(&self, id: &str, before: Option<&str>, shown: &[String]) -> Result<()> {
        if before == Some(id) {
            return Ok(());
        }
        self.write(|w| {
            require_task(w.conn, id)?;
            let mut others = Vec::new();
            for t in shown.iter().filter(|t| *t != id) {
                let key: Option<String> = w
                    .conn
                    .query_row("SELECT sort_key FROM task WHERE id = ?1", [t], |r| r.get(0))
                    .optional()?;
                if let Some(key) = key {
                    others.push((t.clone(), key));
                }
            }
            let at = match before {
                None => others.len(),
                Some(b) => others
                    .iter()
                    .position(|(t, _)| t == b)
                    .ok_or_else(|| Error::Invalid(format!("task {b} isn't in this view")))?,
            };
            let arranged = others.iter().all(|(_, k)| !k.is_empty())
                && others.windows(2).all(|p| p[0].1 < p[1].1);
            if arranged {
                let a = at.checked_sub(1).map(|i| others[i].1.as_str());
                let b = others.get(at).map(|(_, k)| k.as_str());
                let key = key_between(a, b);
                // A key this module didn't make (e.g. from a future version) may not fit.
                if a.is_none_or(|a| a < key.as_str()) && b.is_none_or(|b| key.as_str() < b) {
                    return w.set(Entity::Task, id, "sort_key", key.as_str());
                }
            }
            others.insert(at, (id.to_string(), String::new()));
            for ((t, old), key) in others.iter().zip(spread_keys(others.len())) {
                if *old != key {
                    w.set(Entity::Task, t, "sort_key", key.as_str())?;
                }
            }
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::{NewTask, Store};

    fn titles(tasks: Vec<crate::Task>) -> Vec<String> {
        tasks.into_iter().map(|t| t.title).collect()
    }

    #[test]
    fn list_and_tag_sorts_round_trip() {
        let s = Store::open_in_memory().unwrap();
        let l = s.create_list("Home").unwrap();
        let l = crate::List {
            id: l,
            name: String::new(),
            color: None,
            icon: None,
            archived: false,
        };
        assert!(s.list_sort(&l.id).unwrap().is_empty(), "default");
        s.set_list_sort(&l.id, &["title_asc".into()]).unwrap();
        assert_eq!(s.list_sort(&l.id).unwrap(), ["title_asc"]);
        assert!(s.set_list_sort(&l.id, &["sideways".into()]).is_err());

        let t = s.ensure_tag_path("garden").unwrap();
        s.set_tag_sort(&t, &["manual".into()]).unwrap();
        assert_eq!(s.tag_sort(&t).unwrap(), ["manual"]);
        assert!(s.tag_sort("nope").is_err());

        // A key from a newer build is skipped, not fatal.
        s.conn
            .execute(
                "UPDATE list SET sort = '[\"colour_asc\",\"title_asc\"]' WHERE id = ?1",
                [&l.id],
            )
            .unwrap();
        assert_eq!(s.list_sort(&l.id).unwrap(), ["title_asc"]);
    }

    #[test]
    fn inbox_sort_is_local() {
        let s = Store::open_in_memory().unwrap();
        s.add_task("b").unwrap();
        s.add_task("a").unwrap();
        assert_eq!(
            titles(s.inbox_sorted(&s.inbox_sort().unwrap()).unwrap()),
            ["b", "a"]
        );
        let ops = s.ops().unwrap().len();
        s.set_inbox_sort(&["title_asc".into()]).unwrap();
        assert_eq!(s.inbox_sort().unwrap(), ["title_asc"]);
        assert_eq!(
            titles(s.inbox_sorted(&s.inbox_sort().unwrap()).unwrap()),
            ["a", "b"]
        );
        assert_eq!(s.ops().unwrap().len(), ops, "no op: it doesn't sync");
    }

    #[test]
    fn placing_tasks_in_manual_order() {
        let s = Store::open_in_memory().unwrap();
        let ids: Vec<String> = ["a", "b", "c"]
            .iter()
            .map(|t| {
                s.create_task(&NewTask {
                    title: (*t).into(),
                    ..Default::default()
                })
                .unwrap()
                .id
            })
            .collect();
        let manual = || titles(s.inbox_sorted(&["manual".into()]).unwrap());
        let shown = || -> Vec<String> {
            s.inbox_sorted(&["manual".into()])
                .unwrap()
                .into_iter()
                .map(|t| t.id)
                .collect()
        };
        assert_eq!(manual(), ["c", "b", "a"], "never arranged: newest first");

        // First move numbers them all.
        s.place_task(&ids[0], Some(&ids[2]), &shown()).unwrap();
        assert_eq!(manual(), ["a", "c", "b"]);
        // Then a move is one op.
        let before = s.ops().unwrap().len();
        s.place_task(&ids[1], Some(&ids[0]), &shown()).unwrap();
        assert_eq!(manual(), ["b", "a", "c"]);
        assert_eq!(s.ops().unwrap().len(), before + 1);
        s.place_task(&ids[1], None, &shown()).unwrap();
        assert_eq!(manual(), ["a", "c", "b"]);

        // A new task goes on top.
        s.add_task("d").unwrap();
        assert_eq!(manual(), ["d", "a", "c", "b"]);
        assert!(s.place_task(&ids[0], Some("not-shown"), &shown()).is_err());
    }
}
