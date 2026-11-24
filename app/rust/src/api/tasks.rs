//! The Dart-facing API. Keep this thin: rules live in tt-core.
//!
//! M2 shape: views, a task detail, commands and a change stream. This is the
//! contract the daemon's socket API copies in slice 2 (plan/15 M2 Desktop Log),
//! so the UI doesn't change when the desktop backend moves to the socket.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use chrono_tz::Tz;
use flutter_rust_bridge::frb;
use tt_core::{
    FilterRule, NewTask, ReminderKind, Schedule, Status, Store, Task, is_overdue, local_day,
    local_tz, offset_label, parse_when, when_label,
};

use crate::frb_generated::StreamSink;

/// A task as the M0 Inbox page sees it.
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

/// Which kind of view the task list shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewKind {
    Inbox,
    List,
    Tag,
    Filter,
    Completed,
    Trash,
}

/// What the task list shows. `id` names the list, tag or filter; it's unused otherwise.
/// (A plain struct rather than a data enum, so Dart needs no `freezed`.)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskView {
    pub kind: ViewKind,
    pub id: Option<String>,
}

impl TaskView {
    fn id(&self) -> anyhow::Result<&str> {
        self.id
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("{:?} view needs an id", self.kind))
    }
}

/// A tag as a chip on a task card.
pub struct TagChip {
    pub id: String,
    pub name: String,
    pub path: String,
    /// `#rrggbb`, or none for the default.
    pub color: Option<String>,
    pub icon: Option<String>,
}

/// One task card.
pub struct TaskRow {
    pub id: String,
    pub title: String,
    pub priority: u8,
    pub due_at: Option<i64>,
    pub all_day: bool,
    /// `today`, `Fri 14:30`, `3 Dec`…
    pub due_label: Option<String>,
    pub overdue: bool,
    pub due_today: bool,
    pub done: bool,
    pub deleted: bool,
    pub repeating: bool,
    pub list_name: Option<String>,
    pub tags: Vec<TagChip>,
    pub checklist_done: u32,
    pub checklist_total: u32,
    pub subtasks: u32,
    pub parent_id: Option<String>,
    /// When it was completed (Completed view) or trashed (Trash view).
    pub when_label: Option<String>,
}

/// A sidebar entry: a list, a saved filter or a tag.
pub struct NavItem {
    pub id: String,
    pub name: String,
    pub color: Option<String>,
    pub icon: Option<String>,
    /// Open tasks it shows (a tag counts its children's too).
    pub count: u32,
    /// Nesting level for tags; 0 for lists and filters.
    pub depth: u32,
    pub parent_id: Option<String>,
}

pub struct Sidebar {
    pub inbox: u32,
    pub lists: Vec<NavItem>,
    pub filters: Vec<NavItem>,
    pub tags: Vec<NavItem>,
}

pub struct ChecklistRow {
    pub id: String,
    pub title: String,
    pub done: bool,
}

pub struct ReminderRow {
    pub id: String,
    /// `15m before`, `today 09:00`…
    pub label: String,
}

/// Everything the detail pane shows.
pub struct TaskDetail {
    pub row: TaskRow,
    pub notes: String,
    pub list_id: Option<String>,
    pub start_label: Option<String>,
    /// The RRULE body, e.g. `FREQ=WEEKLY;BYDAY=MO`.
    pub repeat_rule: Option<String>,
    pub checklist: Vec<ChecklistRow>,
    pub subtasks: Vec<TaskRow>,
    pub reminders: Vec<ReminderRow>,
}

/// An open database. SQLite connections aren't `Sync`, so calls are serialised.
#[frb(opaque)]
pub struct TaskDb {
    store: Mutex<Store>,
    sinks: Mutex<Vec<StreamSink<u32>>>,
    version: AtomicU32,
}

impl TaskDb {
    /// Open (or create) the database file at `path`.
    pub fn open(path: String) -> anyhow::Result<TaskDb> {
        Ok(Self {
            store: Mutex::new(Store::open(path)?),
            sinks: Mutex::new(Vec::new()),
            version: AtomicU32::new(0),
        })
    }

    // ----- M0 Inbox page (kept until the M2 shell replaces it everywhere) -----

    pub fn inbox(&self) -> anyhow::Result<Vec<TaskItem>> {
        Ok(self.lock().inbox()?.into_iter().map(Into::into).collect())
    }

    pub fn add(&self, title: String) -> anyhow::Result<TaskItem> {
        let t = self.lock().add_task(&title)?;
        self.changed();
        Ok(t.into())
    }

    /// Returns false if the task was already done.
    pub fn complete(&self, id: String) -> anyhow::Result<bool> {
        let done = self.lock().complete_task(&id)?;
        self.changed();
        Ok(done)
    }

    // ----- Change stream -----

    /// Ticks with a new version number after every write made through this handle.
    /// The socket backend (M2 slice 2) will also tick for writes from other processes.
    pub fn changes(&self, sink: StreamSink<u32>) {
        let v = self.version.load(Ordering::SeqCst);
        if sink.add(v).is_ok() {
            self.sinks().push(sink);
        }
    }

    // ----- Queries -----

    pub fn sidebar(&self) -> anyhow::Result<Sidebar> {
        let s = self.lock();
        let (now, tz) = (now_ms(), local_tz());
        let count = |rule: serde_json::Value| -> anyhow::Result<u32> {
            let rule: FilterRule = serde_json::from_value(rule)?;
            Ok(s.run_rule(&rule, &[], false, now, tz)?.len() as u32)
        };
        let lists = s
            .lists(false)?
            .into_iter()
            .map(|l| {
                Ok(NavItem {
                    count: count(serde_json::json!({"field": "list", "op": "is", "value": l.id}))?,
                    id: l.id,
                    name: l.name,
                    color: l.color,
                    icon: l.icon,
                    depth: 0,
                    parent_id: None,
                })
            })
            .collect::<anyhow::Result<_>>()?;
        let filters = s
            .filters()?
            .into_iter()
            .map(|f| {
                Ok(NavItem {
                    count: s.run_filter(&f.id, now, tz)?.len() as u32,
                    id: f.id,
                    name: f.filter.name,
                    color: None,
                    icon: f.filter.icon,
                    depth: 0,
                    parent_id: None,
                })
            })
            .collect::<anyhow::Result<_>>()?;
        let tags = s
            .tags()?
            .into_iter()
            .map(|t| {
                Ok(NavItem {
                    count: count(serde_json::json!({"field": "tag", "op": "has", "value": t.id}))?,
                    depth: t.path.matches('/').count() as u32,
                    id: t.id,
                    name: t.name,
                    color: t.color,
                    icon: t.icon,
                    parent_id: t.parent_id,
                })
            })
            .collect::<anyhow::Result<_>>()?;
        Ok(Sidebar {
            inbox: s.inbox()?.len() as u32,
            lists,
            filters,
            tags,
        })
    }

    pub fn tasks(&self, view: TaskView) -> anyhow::Result<Vec<TaskRow>> {
        let s = self.lock();
        let (now, tz) = (now_ms(), local_tz());
        let run = |rule: serde_json::Value| -> anyhow::Result<Vec<Task>> {
            let rule: FilterRule = serde_json::from_value(rule)?;
            Ok(s.run_rule(&rule, &[], false, now, tz)?)
        };
        let tasks = match view.kind {
            ViewKind::Inbox => s.inbox()?,
            ViewKind::List => {
                run(serde_json::json!({"field": "list", "op": "is", "value": view.id()?}))?
            }
            ViewKind::Tag => {
                run(serde_json::json!({"field": "tag", "op": "has", "value": view.id()?}))?
            }
            ViewKind::Filter => s.run_filter(view.id()?, now, tz)?,
            ViewKind::Trash => s.trash()?,
            ViewKind::Completed => {
                // Completion history: done tasks plus each occurrence of repeating ones.
                let mut rows = Vec::new();
                for c in s.completed(200)? {
                    let Some(t) = s.get_task(&c.task_id)? else {
                        continue;
                    };
                    let mut row = task_row(&s, t, now, tz)?;
                    row.done = true;
                    row.when_label = Some(when_label(c.completed_at, false, now, tz));
                    rows.push(row);
                }
                return Ok(rows);
            }
        };
        tasks
            .into_iter()
            .map(|t| {
                let deleted_at = t.deleted_at;
                let mut row = task_row(&s, t, now, tz)?;
                if view.kind == ViewKind::Trash {
                    row.when_label = deleted_at.map(|d| when_label(d, false, now, tz));
                }
                Ok(row)
            })
            .collect()
    }

    pub fn task(&self, id: String) -> anyhow::Result<Option<TaskDetail>> {
        let s = self.lock();
        let (now, tz) = (now_ms(), local_tz());
        let Some(t) = s.get_task(&id)? else {
            return Ok(None);
        };
        let (notes, list_id, repeat_rule) = (t.notes.clone(), t.list_id.clone(), t.repeat_rule.clone());
        let start_label = t.start_at.map(|ms| when_label(ms, t.all_day, now, tz));
        let checklist = s
            .checklist(&id)?
            .into_iter()
            .map(|c| ChecklistRow {
                id: c.id,
                title: c.title,
                done: c.done,
            })
            .collect();
        let subtasks = s
            .subtasks(&id)?
            .into_iter()
            .map(|t| task_row(&s, t, now, tz))
            .collect::<anyhow::Result<_>>()?;
        let reminders = s
            .reminders(&id)?
            .into_iter()
            .map(|r| ReminderRow {
                label: match r.kind {
                    ReminderKind::Relative { offset_min } => offset_label(offset_min),
                    ReminderKind::Absolute { at } => when_label(at, false, now, tz),
                },
                id: r.id,
            })
            .collect();
        Ok(Some(TaskDetail {
            row: task_row(&s, t, now, tz)?,
            notes,
            list_id,
            start_label,
            repeat_rule,
            checklist,
            subtasks,
            reminders,
        }))
    }

    // ----- Commands -----

    /// Quick add: a plain title, in the current list or tag; everything else lands in Inbox.
    pub fn add_in(&self, view: TaskView, title: String) -> anyhow::Result<String> {
        let id = {
            let s = self.lock();
            let mut new = NewTask {
                title,
                ..Default::default()
            };
            match view.kind {
                ViewKind::List => new.list_id = view.id,
                ViewKind::Tag => new.tags = view.id.into_iter().collect(),
                _ => {}
            }
            s.create_task(&new)?.id
        };
        self.changed();
        Ok(id)
    }

    pub fn add_subtask(&self, parent_id: String, title: String) -> anyhow::Result<String> {
        let id = self
            .lock()
            .create_task(&NewTask {
                title,
                parent_id: Some(parent_id),
                ..Default::default()
            })?
            .id;
        self.changed();
        Ok(id)
    }

    /// Complete an open task, or reopen a done one. Returns the new done state.
    pub fn toggle_done(&self, id: String) -> anyhow::Result<bool> {
        let done = {
            let s = self.lock();
            let t = s.get_task(&id)?.ok_or_else(|| anyhow::anyhow!("no task {id}"))?;
            if t.status == Status::Done {
                s.reopen_task(&id)?;
                false
            } else {
                s.complete_task(&id)?;
                // A repeating task moves to its next date and stays open.
                s.get_task(&id)?.is_some_and(|t| t.status == Status::Done)
            }
        };
        self.changed();
        Ok(done)
    }

    pub fn trash(&self, id: String) -> anyhow::Result<()> {
        self.write(|s| s.trash_task(&id).map(drop))
    }

    pub fn restore(&self, id: String) -> anyhow::Result<()> {
        self.write(|s| s.restore_task(&id).map(drop))
    }

    pub fn set_title(&self, id: String, title: String) -> anyhow::Result<()> {
        self.write(|s| s.set_title(&id, &title))
    }

    pub fn set_notes(&self, id: String, notes: String) -> anyhow::Result<()> {
        self.write(|s| s.set_notes(&id, &notes))
    }

    pub fn set_priority(&self, id: String, priority: u8) -> anyhow::Result<()> {
        self.write(|s| s.set_priority(&id, priority))
    }

    /// Set the due date from text like `fri`, `tomorrow 14:30` or `2026-12-01`, or clear it.
    /// Switching between all-day and timed drops the start date rather than mixing kinds.
    pub fn set_due(&self, id: String, text: Option<String>) -> anyhow::Result<()> {
        let (now, tz) = (now_ms(), local_tz());
        let text = text.map(|t| t.trim().to_string()).filter(|t| !t.is_empty());
        let parsed = text.map(|t| parse_when(&t, now, tz)).transpose()?;
        self.write(|s| {
            let t = s.get_task(&id)?.ok_or_else(|| tt_core::Error::NotFound(format!("task {id}")))?;
            let mut sch = Schedule {
                start_at: t.start_at,
                due_at: None,
                all_day: t.all_day,
                tz: t.tz.clone(),
                duration_min: t.duration_min,
            };
            if let Some((ms, all_day)) = parsed {
                if all_day != t.all_day {
                    sch.start_at = None;
                }
                sch.due_at = Some(ms);
                sch.all_day = all_day;
            }
            sch.tz = (!sch.all_day).then(|| tz.name().to_string());
            s.set_schedule(&id, Some(&sch))
        })
    }

    pub fn set_list(&self, id: String, list_id: Option<String>) -> anyhow::Result<()> {
        self.write(|s| s.set_task_list(&id, list_id.as_deref()))
    }

    /// Tag a task by path (`home/garden`); missing tags are created.
    pub fn tag(&self, id: String, path: String) -> anyhow::Result<()> {
        self.write(|s| {
            let tag = s.ensure_tag_path(&path)?;
            s.tag_task(&id, &tag)
        })
    }

    pub fn untag(&self, id: String, tag_id: String) -> anyhow::Result<()> {
        self.write(|s| s.untag_task(&id, &tag_id))
    }

    pub fn checklist_add(&self, task_id: String, title: String) -> anyhow::Result<()> {
        self.write(|s| s.add_checklist_item(&task_id, &title).map(drop))
    }

    pub fn checklist_set(&self, item_id: String, done: bool) -> anyhow::Result<()> {
        self.write(|s| s.set_checklist_done(&item_id, done))
    }

    pub fn checklist_remove(&self, item_id: String) -> anyhow::Result<()> {
        self.write(|s| s.remove_checklist_item(&item_id))
    }

    pub fn create_list(&self, name: String) -> anyhow::Result<String> {
        let id = self.lock().create_list(&name)?;
        self.changed();
        Ok(id)
    }

    pub fn set_tag_style(
        &self,
        id: String,
        color: Option<String>,
        icon: Option<String>,
    ) -> anyhow::Result<()> {
        self.write(|s| s.set_tag_style(&id, color.as_deref(), icon.as_deref()))
    }

    // ----- Internals -----

    fn write(&self, f: impl FnOnce(&Store) -> tt_core::Result<()>) -> anyhow::Result<()> {
        f(&self.lock())?;
        self.changed();
        Ok(())
    }

    fn changed(&self) {
        let v = self.version.fetch_add(1, Ordering::SeqCst) + 1;
        // A sink whose Dart stream was cancelled fails to add; drop it.
        self.sinks().retain(|sink| sink.add(v).is_ok());
    }

    fn lock(&self) -> MutexGuard<'_, Store> {
        // A panic mid-call can't leave SQLite half-written, so a poisoned lock is safe to reuse.
        self.store.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn sinks(&self) -> MutexGuard<'_, Vec<StreamSink<u32>>> {
        self.sinks.lock().unwrap_or_else(|e| e.into_inner())
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn task_row(s: &Store, t: Task, now: i64, tz: Tz) -> anyhow::Result<TaskRow> {
    let tags = s
        .task_tags(&t.id)?
        .into_iter()
        .map(|g| TagChip {
            id: g.id,
            name: g.name,
            path: g.path,
            color: g.color,
            icon: g.icon,
        })
        .collect();
    let checklist = s.checklist(&t.id)?;
    let list_name = match &t.list_id {
        Some(l) => s.find_list(l)?.map(|l| l.name),
        None => None,
    };
    let today = local_day(now, false, tz);
    Ok(TaskRow {
        due_label: t.due_at.map(|d| when_label(d, t.all_day, now, tz)),
        overdue: t.status == Status::Open
            && t.due_at.is_some_and(|d| is_overdue(d, t.all_day, now, tz)),
        due_today: t.due_at.is_some_and(|d| local_day(d, t.all_day, tz) == today),
        done: t.status == Status::Done,
        deleted: t.status == Status::Deleted,
        repeating: t.repeat_rule.is_some(),
        checklist_done: checklist.iter().filter(|c| c.done).count() as u32,
        checklist_total: checklist.len() as u32,
        subtasks: s.subtasks(&t.id)?.len() as u32,
        list_name,
        tags,
        id: t.id,
        title: t.title,
        priority: t.priority,
        due_at: t.due_at,
        all_day: t.all_day,
        parent_id: t.parent_id,
        when_label: None,
    })
}

/// The desktop database: the same file `tt` and `tt-daemon` use
/// (`~/.local/share/tick-alt/tt.db` on Linux, `~/Library/Application Support/tick-alt/tt.db`
/// on macOS), so the app, the CLI and the tray share one store until the socket lands.
#[frb(sync)]
pub fn desktop_db_path() -> anyhow::Result<String> {
    let dir = dirs::data_dir()
        .ok_or_else(|| anyhow::anyhow!("no platform data directory"))?
        .join("tick-alt");
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join("tt.db").to_string_lossy().into_owned())
}

#[frb(init)]
pub fn init_app() {
    flutter_rust_bridge::setup_default_user_utils();
}
