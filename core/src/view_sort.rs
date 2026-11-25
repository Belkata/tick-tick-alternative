//! How task views are sorted, and manual task order.
//!
//! One sort per device, shared by every view (Inbox, lists, tags, filters), so a
//! choice stays when you switch views (decided 2026-11-25). It's a JSON array of
//! sort keys in `meta`, never synced. Manual order is each task's synced `sort_key`:
//! one order shared by every view, arranged within whichever view a task is moved in.
//! (v0.5.0 kept a sort per list and tag; schema v7's `list.sort` and `tag.sort` are
//! still applied from peers but no longer read.)

use rusqlite::OptionalExtension;

use crate::oplog::Entity;
use crate::order::{key_between, spread_keys};
use crate::task::{TASK_COLUMNS, query_tasks, require_task};
use crate::{Error, Result, Store, Task};

/// Every sort key the core knows (first wins).
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

/// The sort until one is chosen: due date, then priority.
pub const DEFAULT_SORT: &[&str] = &["due_asc", "priority_desc"];

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

impl Store {
    /// How this device sorts every view: the chosen sort, or [`DEFAULT_SORT`].
    pub fn device_sort(&self) -> Result<Vec<String>> {
        // v0.5.0 kept the Inbox's choice under `inbox_sort`; it carries over.
        let json: Option<String> = self
            .conn
            .query_row(
                "SELECT value FROM meta WHERE key IN ('sort', 'inbox_sort') \
                 ORDER BY key = 'sort' DESC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .optional()?;
        let sort = json.map(|j| known_sort(&j)).unwrap_or_default();
        Ok(if sort.is_empty() {
            DEFAULT_SORT.iter().map(|k| k.to_string()).collect()
        } else {
            sort
        })
    }

    /// Choose how this device sorts every view. Local only: it isn't an op and never syncs.
    pub fn set_device_sort(&self, sort: &[String]) -> Result<()> {
        check_sort(sort)?;
        self.conn.execute(
            "INSERT INTO meta (key, value) VALUES ('sort', ?1) \
             ON CONFLICT (key) DO UPDATE SET value = excluded.value",
            [serde_json::to_string(sort)?],
        )?;
        self.conn
            .execute("DELETE FROM meta WHERE key = 'inbox_sort'", [])?;
        Ok(())
    }

    /// Inbox tasks in the given order (empty: [`DEFAULT_SORT`]).
    pub fn inbox_sorted(&self, sort: &[String]) -> Result<Vec<Task>> {
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
    fn one_local_sort_for_every_view() {
        let s = Store::open_in_memory().unwrap();
        s.add_task("b").unwrap();
        s.add_task("a").unwrap();
        assert_eq!(s.device_sort().unwrap(), super::DEFAULT_SORT);
        assert_eq!(
            titles(s.inbox_sorted(&s.device_sort().unwrap()).unwrap()),
            ["b", "a"]
        );
        let ops = s.ops().unwrap().len();
        s.set_device_sort(&["title_asc".into()]).unwrap();
        assert_eq!(s.device_sort().unwrap(), ["title_asc"]);
        assert_eq!(
            titles(s.inbox_sorted(&s.device_sort().unwrap()).unwrap()),
            ["a", "b"]
        );
        assert_eq!(s.ops().unwrap().len(), ops, "no op: it doesn't sync");
        assert!(s.set_device_sort(&["sideways".into()]).is_err());
    }

    #[test]
    fn the_v0_5_0_inbox_sort_carries_over() {
        let s = Store::open_in_memory().unwrap();
        s.conn
            .execute(
                "INSERT INTO meta (key, value) VALUES ('inbox_sort', '[\"priority_desc\",\"colour\"]')",
                [],
            )
            .unwrap();
        assert_eq!(
            s.device_sort().unwrap(),
            ["priority_desc"],
            "unknown keys skipped"
        );
        s.set_device_sort(&["manual".into()]).unwrap();
        assert_eq!(s.device_sort().unwrap(), ["manual"]);
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
