use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, Row};
use uuid::Uuid;

use crate::oplog::Entity;
use crate::store::Writer;
use crate::{Error, Result, Store};

/// Subtasks may nest this deep: a top-level task is level 1 (plan/11 question 5).
pub const MAX_DEPTH: usize = 3;

/// Lifecycle state of a task. "Won't do" is deliberately absent (plan/11 question 9).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Status {
    Open,
    Done,
    /// In Trash.
    Deleted,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Open => "open",
            Status::Done => "done",
            Status::Deleted => "deleted",
        }
    }

    pub(crate) fn parse(s: &str) -> Self {
        match s {
            "done" => Status::Done,
            "deleted" => Status::Deleted,
            _ => Status::Open,
        }
    }
}

/// How a repeating task picks its next date.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum RepeatMode {
    /// Next date from the schedule, e.g. every Monday.
    Fixed,
    /// Next date counted from when it was completed.
    AfterCompletion,
}

impl RepeatMode {
    pub fn as_str(self) -> &'static str {
        match self {
            RepeatMode::Fixed => "fixed",
            RepeatMode::AfterCompletion => "after_completion",
        }
    }

    fn parse(s: &str) -> Self {
        match s {
            "after_completion" => RepeatMode::AfterCompletion,
            _ => RepeatMode::Fixed,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub notes: String,
    pub list_id: Option<String>,
    pub parent_id: Option<String>,
    /// 0 none, 1 low, 2 medium, 3 high.
    pub priority: u8,
    /// Epoch ms. For all-day tasks, UTC midnight of the date.
    pub start_at: Option<i64>,
    /// Epoch ms. For all-day tasks, UTC midnight of the date.
    pub due_at: Option<i64>,
    pub all_day: bool,
    /// IANA zone of a timed task; `None` for all-day (floating) dates.
    pub tz: Option<String>,
    pub duration_min: Option<i64>,
    /// RRULE body without DTSTART, e.g. `FREQ=WEEKLY;BYDAY=MO`.
    pub repeat_rule: Option<String>,
    pub repeat_mode: RepeatMode,
    pub status: Status,
    pub completed_at: Option<i64>,
    pub deleted_at: Option<i64>,
    pub sort_key: String,
    pub gcal_event_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    /// Shown on the calendar and its Unscheduled panel ("Show in Calendar").
    /// Defaults to on when a peer or daemon from before v6 leaves it out.
    #[serde(default = "yes")]
    pub in_calendar: bool,
}

fn yes() -> bool {
    true
}

impl Task {
    /// The date reminders and "has a date" checks use: start if set, else due.
    pub fn anchor(&self) -> Option<i64> {
        self.start_at.or(self.due_at)
    }

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
            all_day: row.get(8)?,
            tz: row.get(9)?,
            duration_min: row.get(10)?,
            repeat_rule: row.get(11)?,
            repeat_mode: RepeatMode::parse(&row.get::<_, String>(12)?),
            status: Status::parse(&row.get::<_, String>(13)?),
            completed_at: row.get(14)?,
            deleted_at: row.get(15)?,
            sort_key: row.get(16)?,
            gcal_event_id: row.get(17)?,
            created_at: row.get(18)?,
            updated_at: row.get(19)?,
            in_calendar: row.get(20)?,
        })
    }
}

pub(crate) const TASK_COLUMNS: &str = "task.id, task.title, task.notes, task.list_id, task.parent_id, \
     task.priority, task.start_at, task.due_at, task.all_day, task.tz, task.duration_min, \
     task.repeat_rule, task.repeat_mode, task.status, task.completed_at, task.deleted_at, \
     task.sort_key, task.gcal_event_id, task.created_at, task.updated_at, task.in_calendar";

/// When a task happens. All-day dates are floating: stored as UTC midnight of the
/// calendar date and shown on that date in any zone.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Schedule {
    pub start_at: Option<i64>,
    pub due_at: Option<i64>,
    pub all_day: bool,
    /// Required for timed tasks, ignored for all-day ones.
    pub tz: Option<String>,
    pub duration_min: Option<i64>,
}

impl Schedule {
    fn validate(&self) -> Result<()> {
        if let (Some(s), Some(d)) = (self.start_at, self.due_at)
            && s > d
        {
            return Err(Error::Invalid("start is after due".into()));
        }
        if self.all_day {
            let day = crate::dates::DAY_MS;
            if [self.start_at, self.due_at]
                .into_iter()
                .flatten()
                .any(|t| t.rem_euclid(day) != 0)
            {
                return Err(Error::Invalid("all-day dates must be UTC midnight".into()));
            }
        } else if self.start_at.or(self.due_at).is_some() {
            let tz = self
                .tz
                .as_deref()
                .ok_or_else(|| Error::Invalid("a timed task needs a time zone".into()))?;
            tz.parse::<chrono_tz::Tz>()
                .map_err(|_| Error::Invalid(format!("unknown time zone '{tz}'")))?;
        }
        if self.duration_min.is_some_and(|d| d <= 0) {
            return Err(Error::Invalid("duration must be positive".into()));
        }
        Ok(())
    }
}

/// Everything a new task can start with. `Default` gives an Inbox task.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct NewTask {
    pub title: String,
    pub notes: String,
    pub list_id: Option<String>,
    pub parent_id: Option<String>,
    pub priority: u8,
    pub schedule: Schedule,
    /// Tag ids.
    pub tags: Vec<String>,
}

pub(crate) fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

pub(crate) fn load_task(conn: &Connection, id: &str) -> Result<Option<Task>> {
    let sql = format!("SELECT {TASK_COLUMNS} FROM task WHERE id = ?1");
    Ok(conn.query_row(&sql, [id], Task::from_row).optional()?)
}

pub(crate) fn require_task(conn: &Connection, id: &str) -> Result<Task> {
    load_task(conn, id)?.ok_or_else(|| Error::NotFound(format!("task {id}")))
}

pub(crate) fn query_tasks(
    conn: &Connection,
    sql: &str,
    params: impl rusqlite::Params,
) -> Result<Vec<Task>> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(params, Task::from_row)?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// Level of `id` in its subtask tree (top-level = 1). Stops at loops.
fn depth(conn: &Connection, id: &str) -> Result<usize> {
    let mut level = 1;
    let mut current = id.to_string();
    while let Some(parent) = conn
        .query_row(
            "SELECT parent_id FROM task WHERE id = ?1",
            [&current],
            |r| r.get::<_, Option<String>>(0),
        )
        .optional()?
        .flatten()
    {
        level += 1;
        if level > 64 {
            break;
        }
        current = parent;
    }
    Ok(level)
}

/// How many levels the subtree under `id` adds below it (a leaf adds 0).
fn subtree_height(conn: &Connection, id: &str) -> Result<usize> {
    let mut stmt = conn.prepare("SELECT id FROM task WHERE parent_id = ?1")?;
    let children: Vec<String> = stmt
        .query_map([id], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    let mut height = 0;
    for c in children {
        height = height.max(1 + subtree_height(conn, &c)?);
    }
    Ok(height)
}

pub(crate) fn descendants(conn: &Connection, id: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "WITH RECURSIVE d(id) AS (SELECT id FROM task WHERE parent_id = ?1 \
           UNION SELECT t.id FROM task t JOIN d ON t.parent_id = d.id) SELECT id FROM d",
    )?;
    let ids = stmt
        .query_map([id], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(ids)
}

fn check_parent(conn: &Connection, task_id: Option<&str>, parent_id: &str) -> Result<()> {
    let parent = require_task(conn, parent_id)?;
    if parent.status == Status::Deleted {
        return Err(Error::Invalid("parent is in Trash".into()));
    }
    if let Some(id) = task_id {
        if id == parent_id || descendants(conn, id)?.iter().any(|d| d == parent_id) {
            return Err(Error::Invalid("a task can't be under itself".into()));
        }
        let height = subtree_height(conn, id)?;
        if depth(conn, parent_id)? + 1 + height > MAX_DEPTH {
            return Err(Error::Invalid(format!(
                "subtasks nest at most {MAX_DEPTH} levels"
            )));
        }
    } else if depth(conn, parent_id)? + 1 > MAX_DEPTH {
        return Err(Error::Invalid(format!(
            "subtasks nest at most {MAX_DEPTH} levels"
        )));
    }
    Ok(())
}

pub(crate) fn write_schedule(w: &mut Writer, id: &str, s: &Schedule) -> Result<()> {
    s.validate()?;
    w.set(Entity::Task, id, "start_at", s.start_at)?;
    w.set(Entity::Task, id, "due_at", s.due_at)?;
    w.set(Entity::Task, id, "all_day", s.all_day)?;
    let tz = if s.all_day { None } else { s.tz.clone() };
    w.set(Entity::Task, id, "tz", tz)?;
    w.set(Entity::Task, id, "duration_min", s.duration_min)
}

pub(crate) fn write_status(w: &mut Writer, id: &str, status: Status) -> Result<()> {
    let now = w.now();
    w.set(Entity::Task, id, "status", status.as_str())?;
    w.set(
        Entity::Task,
        id,
        "completed_at",
        (status == Status::Done).then_some(now),
    )?;
    w.set(
        Entity::Task,
        id,
        "deleted_at",
        (status == Status::Deleted).then_some(now),
    )
}

impl Store {
    /// Create a task with only a title. With no list, tag or date it lands in Inbox.
    pub fn add_task(&self, title: &str) -> Result<Task> {
        self.create_task(&NewTask {
            title: title.to_string(),
            ..Default::default()
        })
    }

    pub fn create_task(&self, new: &NewTask) -> Result<Task> {
        self.create_task_with(new, true)
    }

    /// `create_task`, with the choice of adding the automatic at-due-time reminder
    /// (`reminder::auto_due_reminder`). Imports pass `false`: they bring their own reminders.
    pub(crate) fn create_task_with(&self, new: &NewTask, auto_reminder: bool) -> Result<Task> {
        let title = new.title.trim();
        if title.is_empty() {
            return Err(Error::Invalid("a task needs a title".into()));
        }
        if new.priority > 3 {
            return Err(Error::Invalid("priority is 0–3".into()));
        }
        let id = Uuid::now_v7().to_string();
        self.write(|w| {
            if let Some(parent) = &new.parent_id {
                check_parent(w.conn, None, parent)?;
            }
            if let Some(list) = &new.list_id {
                crate::organize::require_list(w.conn, list)?;
            }
            let now = w.now();
            w.set(Entity::Task, &id, "title", title)?;
            w.set(Entity::Task, &id, "created_at", now)?;
            if !new.notes.is_empty() {
                w.set(Entity::Task, &id, "notes", new.notes.as_str())?;
            }
            if new.list_id.is_some() {
                w.set(Entity::Task, &id, "list_id", new.list_id.clone())?;
            }
            if new.parent_id.is_some() {
                w.set(Entity::Task, &id, "parent_id", new.parent_id.clone())?;
            }
            if new.priority != 0 {
                w.set(Entity::Task, &id, "priority", new.priority)?;
            }
            if new.schedule != Schedule::default() {
                write_schedule(w, &id, &new.schedule)?;
                if auto_reminder {
                    crate::reminder::auto_due_reminder(w, &id, (None, false), &new.schedule)?;
                }
            }
            for tag in &new.tags {
                crate::organize::require_tag(w.conn, tag)?;
                w.set(Entity::Task, &id, &format!("tag:{tag}"), true)?;
            }
            require_task(w.conn, &id)
        })
    }

    pub fn get_task(&self, id: &str) -> Result<Option<Task>> {
        load_task(&self.conn, id)
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
        query_tasks(&self.conn, &sql, [])
    }

    /// Direct subtasks of `parent_id` that aren't in Trash, oldest first.
    pub fn subtasks(&self, parent_id: &str) -> Result<Vec<Task>> {
        let sql = format!(
            "SELECT {TASK_COLUMNS} FROM task WHERE parent_id = ?1 AND status != 'deleted' \
             ORDER BY sort_key, created_at, id"
        );
        query_tasks(&self.conn, &sql, [parent_id])
    }

    /// Tasks in Trash, most recently deleted first.
    pub fn trash(&self) -> Result<Vec<Task>> {
        let sql = format!(
            "SELECT {TASK_COLUMNS} FROM task WHERE status = 'deleted' ORDER BY deleted_at DESC, id"
        );
        query_tasks(&self.conn, &sql, [])
    }

    pub fn set_title(&self, id: &str, title: &str) -> Result<()> {
        let title = title.trim();
        if title.is_empty() {
            return Err(Error::Invalid("a task needs a title".into()));
        }
        self.write(|w| {
            require_task(w.conn, id)?;
            w.set(Entity::Task, id, "title", title)
        })
    }

    pub fn set_notes(&self, id: &str, notes: &str) -> Result<()> {
        self.write(|w| {
            require_task(w.conn, id)?;
            w.set(Entity::Task, id, "notes", notes)
        })
    }

    pub fn set_priority(&self, id: &str, priority: u8) -> Result<()> {
        if priority > 3 {
            return Err(Error::Invalid("priority is 0–3".into()));
        }
        self.write(|w| {
            require_task(w.conn, id)?;
            w.set(Entity::Task, id, "priority", priority)
        })
    }

    /// Make `id` a subtask of `parent_id`, or top-level with `None`.
    pub fn set_parent(&self, id: &str, parent_id: Option<&str>) -> Result<()> {
        self.write(|w| {
            require_task(w.conn, id)?;
            if let Some(p) = parent_id {
                check_parent(w.conn, Some(id), p)?;
            }
            w.set(Entity::Task, id, "parent_id", parent_id)?;
            // The check above looks at the forest as it is now. Removing this task's old
            // link can also free older links that lost (oplog::resolve_parents), so
            // make sure the move really took; if not, the write rolls back.
            let effective: Option<String> =
                w.conn
                    .query_row("SELECT parent_id FROM task WHERE id = ?1", [id], |r| {
                        r.get(0)
                    })?;
            if effective.as_deref() != parent_id {
                return Err(Error::Invalid(
                    "that move would clash with other subtask moves".into(),
                ));
            }
            Ok(())
        })
    }

    /// Show the task on the calendar (and its Unscheduled panel) or keep it off.
    pub fn set_in_calendar(&self, id: &str, show: bool) -> Result<()> {
        self.write(|w| {
            require_task(w.conn, id)?;
            w.set(Entity::Task, id, "in_calendar", show)
        })
    }

    /// Open tasks that `id` could move under: not itself or its subtasks, and not so
    /// deep that its subtree would pass [MAX_DEPTH]. By title.
    pub fn parent_candidates(&self, id: &str) -> Result<Vec<Task>> {
        require_task(&self.conn, id)?;
        let below = descendants(&self.conn, id)?;
        let height = subtree_height(&self.conn, id)?;
        let open = query_tasks(
            &self.conn,
            &format!(
                "SELECT {TASK_COLUMNS} FROM task WHERE status = 'open' AND id != ?1 \
                 ORDER BY lower(title), id"
            ),
            [id],
        )?;
        let mut out = Vec::new();
        for t in open {
            if !below.contains(&t.id) && depth(&self.conn, &t.id)? + 1 + height <= MAX_DEPTH {
                out.push(t);
            }
        }
        Ok(out)
    }

    /// Set or clear when a task happens. `None` clears every date field.
    pub fn set_schedule(&self, id: &str, schedule: Option<&Schedule>) -> Result<()> {
        self.write(|w| {
            let t = require_task(w.conn, id)?;
            let new = schedule.cloned().unwrap_or_default();
            write_schedule(w, id, &new)?;
            crate::reminder::auto_due_reminder(w, id, (t.anchor(), t.all_day), &new)
        })
    }

    /// Set or clear only the due time (epoch ms), keeping the other date fields.
    pub fn set_task_due(&self, id: &str, due_at: Option<i64>) -> Result<()> {
        self.write(|w| {
            let t = require_task(w.conn, id)?;
            let mut s = Schedule {
                start_at: t.start_at,
                due_at,
                all_day: t.all_day,
                tz: t.tz.clone(),
                duration_min: t.duration_min,
            };
            if !s.all_day && s.tz.is_none() {
                s.tz = Some("UTC".into());
            }
            write_schedule(w, id, &s)?;
            crate::reminder::auto_due_reminder(w, id, (t.anchor(), t.all_day), &s)
        })
    }

    /// Mark a task done, or advance a repeating one to its next date.
    /// Returns false if no open task has that id.
    pub fn complete_task(&self, id: &str) -> Result<bool> {
        self.write(|w| crate::recur::complete(w, id))
    }

    /// Undo completion. Returns false if the task wasn't done.
    pub fn reopen_task(&self, id: &str) -> Result<bool> {
        self.write(|w| {
            let t = require_task(w.conn, id)?;
            if t.status != Status::Done {
                return Ok(false);
            }
            write_status(w, id, Status::Open)?;
            Ok(true)
        })
    }

    /// Move a task and its subtasks to Trash. Returns false if it doesn't exist or is already there.
    pub fn trash_task(&self, id: &str) -> Result<bool> {
        self.write(|w| {
            let Some(t) = load_task(w.conn, id)? else {
                return Ok(false);
            };
            if t.status == Status::Deleted {
                return Ok(false);
            }
            write_status(w, id, Status::Deleted)?;
            for d in descendants(w.conn, id)? {
                if require_task(w.conn, &d)?.status != Status::Deleted {
                    write_status(w, &d, Status::Deleted)?;
                }
            }
            Ok(true)
        })
    }

    /// Bring a task back from Trash as open, with the subtasks that were trashed
    /// along with it. Returns false if it wasn't in Trash.
    pub fn restore_task(&self, id: &str) -> Result<bool> {
        self.write(|w| {
            let t = require_task(w.conn, id)?;
            if t.status != Status::Deleted {
                return Ok(false);
            }
            write_status(w, id, Status::Open)?;
            for d in descendants(w.conn, id)? {
                let sub = require_task(w.conn, &d)?;
                if sub.status == Status::Deleted && sub.deleted_at == t.deleted_at {
                    write_status(w, &d, Status::Open)?;
                }
            }
            Ok(true)
        })
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
        assert!(t.created_at > 0);

        assert!(store.complete_task(&t.id).unwrap());
        assert!(!store.complete_task(&t.id).unwrap(), "already done");

        let done = store.get_task(&t.id).unwrap().unwrap();
        assert_eq!(done.status, Status::Done);
        assert!(done.completed_at.is_some());
        assert!(store.open_tasks().unwrap().is_empty());

        assert!(store.reopen_task(&t.id).unwrap());
        let open = store.get_task(&t.id).unwrap().unwrap();
        assert_eq!((open.status, open.completed_at), (Status::Open, None));
    }

    #[test]
    fn trash_cascades_and_restore_reopens() {
        let s = Store::open_in_memory().unwrap();
        let parent = s.add_task("parent").unwrap();
        let child = s
            .create_task(&NewTask {
                title: "child".into(),
                parent_id: Some(parent.id.clone()),
                ..Default::default()
            })
            .unwrap();
        s.complete_task(&parent.id).unwrap();
        assert!(s.trash_task(&parent.id).unwrap());
        let trashed = s.get_task(&parent.id).unwrap().unwrap();
        assert_eq!(
            (trashed.status, trashed.completed_at),
            (Status::Deleted, None)
        );
        assert!(trashed.deleted_at.is_some());
        assert_eq!(
            s.get_task(&child.id).unwrap().unwrap().status,
            Status::Deleted
        );
        assert_eq!(s.trash().unwrap().len(), 2);

        assert!(s.restore_task(&parent.id).unwrap());
        assert_eq!(
            s.get_task(&parent.id).unwrap().unwrap().status,
            Status::Open
        );
        assert_eq!(
            s.get_task(&child.id).unwrap().unwrap().status,
            Status::Open,
            "trashed together"
        );

        // A subtask trashed on its own earlier stays in Trash when the parent comes back.
        // (Trashing is matched by its millisecond stamp; people are slower than that.)
        s.trash_task(&child.id).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
        s.trash_task(&parent.id).unwrap();
        s.restore_task(&parent.id).unwrap();
        assert_eq!(
            s.get_task(&child.id).unwrap().unwrap().status,
            Status::Deleted
        );
    }

    #[test]
    fn subtasks_nest_three_levels_and_never_loop() {
        let s = Store::open_in_memory().unwrap();
        let sub = |title: &str, parent: &str| NewTask {
            title: title.into(),
            parent_id: Some(parent.into()),
            ..Default::default()
        };
        let a = s.add_task("a").unwrap();
        let b = s.create_task(&sub("b", &a.id)).unwrap();
        let c = s.create_task(&sub("c", &b.id)).unwrap();
        assert!(s.create_task(&sub("d", &c.id)).is_err(), "level 4");
        assert!(s.set_parent(&a.id, Some(&c.id)).is_err(), "loop");
        assert!(s.set_parent(&a.id, Some(&a.id)).is_err(), "self");

        let x = s.add_task("x").unwrap();
        assert!(
            s.set_parent(&a.id, Some(&x.id)).is_err(),
            "a's subtree would reach level 4"
        );
        s.set_parent(&c.id, Some(&a.id)).unwrap();
        assert_eq!(s.subtasks(&a.id).unwrap().len(), 2);
    }

    #[test]
    fn parent_candidates_leave_out_loops_and_too_deep_tasks() {
        let s = Store::open_in_memory().unwrap();
        let sub = |title: &str, parent: &str| NewTask {
            title: title.into(),
            parent_id: Some(parent.into()),
            ..Default::default()
        };
        let a = s.add_task("a").unwrap();
        let b = s.create_task(&sub("b", &a.id)).unwrap();
        let c = s.create_task(&sub("c", &b.id)).unwrap();
        let x = s.add_task("x").unwrap();
        let done = s.add_task("done").unwrap();
        s.complete_task(&done.id).unwrap();
        let titles = |id: &str| -> Vec<String> {
            s.parent_candidates(id)
                .unwrap()
                .into_iter()
                .map(|t| t.title)
                .collect()
        };
        // a has two levels below it, so only a top-level parent would fit... and none
        // would: a + 2 levels under x makes 4.
        assert!(titles(&a.id).is_empty());
        // b (one level below) can go under a top-level task, not under c.
        assert_eq!(titles(&b.id), ["a", "x"]);
        // c (a leaf) can go anywhere above level 3 except itself.
        assert_eq!(titles(&c.id), ["a", "b", "x"]);
        // Every candidate is accepted by set_parent.
        for t in s.parent_candidates(&c.id).unwrap() {
            s.set_parent(&c.id, Some(&t.id)).unwrap();
        }
        // c ended under x: x can't go under c (loop) or b (c would reach level 4).
        assert_eq!(titles(&x.id), ["a"]);
    }

    #[test]
    fn schedule_validation() {
        let s = Store::open_in_memory().unwrap();
        let t = s.add_task("t").unwrap();
        let day = 1_764_028_800_000; // 2025-11-25T00:00Z
        let all_day = Schedule {
            due_at: Some(day),
            all_day: true,
            ..Default::default()
        };
        s.set_schedule(&t.id, Some(&all_day)).unwrap();
        let got = s.get_task(&t.id).unwrap().unwrap();
        assert!(got.all_day && got.tz.is_none());

        let bad_midnight = Schedule {
            due_at: Some(day + 1),
            ..all_day.clone()
        };
        assert!(s.set_schedule(&t.id, Some(&bad_midnight)).is_err());
        let no_tz = Schedule {
            due_at: Some(day),
            ..Default::default()
        };
        assert!(s.set_schedule(&t.id, Some(&no_tz)).is_err());
        let timed = Schedule {
            tz: Some("Europe/Sofia".into()),
            ..no_tz
        };
        s.set_schedule(&t.id, Some(&timed)).unwrap();
        s.set_schedule(&t.id, None).unwrap();
        let cleared = s.get_task(&t.id).unwrap().unwrap();
        assert_eq!((cleared.due_at, cleared.all_day), (None, false));
    }

    #[test]
    fn empty_titles_are_rejected() {
        let s = Store::open_in_memory().unwrap();
        assert!(s.add_task("   ").is_err());
        let t = s.add_task("ok").unwrap();
        assert!(s.set_title(&t.id, "").is_err());
    }
}
