//! The Inbox is a derived view, not a stored flag. See plan/04 Data Model.md.
//!
//! A task is in Inbox while it is open, top-level, and has no list, no tag,
//! no start and no due date. Giving it any of those triages it out.

use crate::task::{TASK_COLUMNS, query_tasks};
use crate::{Result, Store, Task};

/// A priority alone doesn't triage a task (plan/11 question 11). Tags that were
/// deleted on another device don't count; tags not synced yet do.
pub(crate) const INBOX_WHERE: &str = "task.status = 'open' \
     AND task.parent_id IS NULL \
     AND task.list_id IS NULL \
     AND task.start_at IS NULL \
     AND task.due_at IS NULL \
     AND NOT EXISTS (SELECT 1 FROM task_tag tt LEFT JOIN tag g ON g.id = tt.tag_id \
                     WHERE tt.task_id = task.id AND coalesce(g.deleted, 0) = 0)";

impl Store {
    /// Tasks currently in Inbox, oldest first.
    pub fn inbox(&self) -> Result<Vec<Task>> {
        let sql =
            format!("SELECT {TASK_COLUMNS} FROM task WHERE {INBOX_WHERE} ORDER BY created_at, id");
        query_tasks(&self.conn, &sql, [])
    }
}

#[cfg(test)]
mod tests {
    use crate::Store;

    fn inbox_titles(store: &Store) -> Vec<String> {
        store
            .inbox()
            .unwrap()
            .into_iter()
            .map(|t| t.title)
            .collect()
    }

    #[test]
    fn new_task_lands_in_inbox() {
        let s = Store::open_in_memory().unwrap();
        s.add_task("a").unwrap();
        assert_eq!(inbox_titles(&s), ["a"]);
    }

    #[test]
    fn tag_list_or_due_moves_task_out() {
        let s = Store::open_in_memory().unwrap();
        let tagged = s.add_task("tagged").unwrap();
        let listed = s.add_task("listed").unwrap();
        let dated = s.add_task("dated").unwrap();
        s.add_task("stays").unwrap();

        let tag = s.create_tag("home", None).unwrap();
        let list = s.create_list("Work").unwrap();
        s.tag_task(&tagged.id, &tag).unwrap();
        s.set_task_list(&listed.id, Some(&list)).unwrap();
        s.set_task_due(&dated.id, Some(1_800_000_000_000)).unwrap();

        assert_eq!(inbox_titles(&s), ["stays"]);
    }

    #[test]
    fn removing_the_only_tag_returns_task_to_inbox() {
        let s = Store::open_in_memory().unwrap();
        let t = s.add_task("t").unwrap();
        let tag = s.create_tag("x", None).unwrap();
        s.tag_task(&t.id, &tag).unwrap();
        assert!(inbox_titles(&s).is_empty());
        s.untag_task(&t.id, &tag).unwrap();
        assert_eq!(inbox_titles(&s), ["t"]);
    }

    #[test]
    fn priority_alone_keeps_task_in_inbox() {
        let s = Store::open_in_memory().unwrap();
        let t = s.add_task("urgent").unwrap();
        s.set_priority(&t.id, 3).unwrap();
        assert_eq!(inbox_titles(&s), ["urgent"]);
    }

    #[test]
    fn subtasks_are_not_in_inbox() {
        let s = Store::open_in_memory().unwrap();
        let p = s.add_task("parent").unwrap();
        s.create_task(&crate::NewTask {
            title: "child".into(),
            parent_id: Some(p.id),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(inbox_titles(&s), ["parent"]);
    }

    #[test]
    fn done_and_trashed_tasks_leave_inbox() {
        let s = Store::open_in_memory().unwrap();
        let done = s.add_task("done").unwrap();
        let trashed = s.add_task("trashed").unwrap();
        s.complete_task(&done.id).unwrap();
        s.trash_task(&trashed.id).unwrap();
        assert!(inbox_titles(&s).is_empty());
    }
}
