//! The Dart-facing API. Keep this thin: the views live in tt-api, the rules in tt-core.
//!
//! On desktop `TaskDb.connectOrOpen` talks to `tt-daemon` over its socket when it serves
//! this database, and opens the file directly otherwise; Android always opens it in-process
//! (plan/15 M2 Desktop Log). The Dart API is the same either way.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{SystemTime, UNIX_EPOCH};

use flutter_rust_bridge::frb;
use tt_api::{Api, Request};
use tt_core::{NewTask, Store, Task, local_tz};

use crate::changes::Changes;
use crate::frb_generated::StreamSink;

pub use tt_api::{
    ChecklistRow, NavItem, ReminderRow, Sidebar, TagChip, TaskDetail, TaskRow, TaskView, ViewKind,
};

// Dart sees the tt-api view types through these mirrors (one definition, in tt-api).

#[frb(mirror(ViewKind))]
pub enum _ViewKind {
    Inbox,
    List,
    Tag,
    Filter,
    Completed,
    Trash,
}

#[frb(mirror(TaskView))]
pub struct _TaskView {
    pub kind: ViewKind,
    pub id: Option<String>,
}

#[frb(mirror(TagChip))]
pub struct _TagChip {
    pub id: String,
    pub name: String,
    pub path: String,
    pub color: Option<String>,
    pub icon: Option<String>,
}

#[frb(mirror(TaskRow))]
pub struct _TaskRow {
    pub id: String,
    pub title: String,
    pub priority: u8,
    pub due_at: Option<i64>,
    pub all_day: bool,
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
    pub when_label: Option<String>,
}

#[frb(mirror(NavItem))]
pub struct _NavItem {
    pub id: String,
    pub name: String,
    pub color: Option<String>,
    pub icon: Option<String>,
    pub count: u32,
    pub depth: u32,
    pub parent_id: Option<String>,
}

#[frb(mirror(Sidebar))]
pub struct _Sidebar {
    pub inbox: u32,
    pub lists: Vec<NavItem>,
    pub filters: Vec<NavItem>,
    pub tags: Vec<NavItem>,
}

#[frb(mirror(ChecklistRow))]
pub struct _ChecklistRow {
    pub id: String,
    pub title: String,
    pub done: bool,
}

#[frb(mirror(ReminderRow))]
pub struct _ReminderRow {
    pub id: String,
    pub label: String,
}

#[frb(mirror(TaskDetail))]
pub struct _TaskDetail {
    pub row: TaskRow,
    pub notes: String,
    pub list_id: Option<String>,
    pub start_label: Option<String>,
    pub repeat_rule: Option<String>,
    pub checklist: Vec<ChecklistRow>,
    pub subtasks: Vec<TaskRow>,
    pub reminders: Vec<ReminderRow>,
}

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

/// The task database, in-process or through the daemon.
#[frb(opaque)]
pub struct TaskDb {
    api: Api,
    changes: Arc<Changes>,
}

impl TaskDb {
    /// Open (or create) the database file at `path` in this process (Android, tests).
    pub fn open(path: String) -> anyhow::Result<TaskDb> {
        Ok(Self {
            api: Api::local(Store::open(path)?),
            changes: Arc::default(),
        })
    }

    /// Desktop: use `tt-daemon` if it serves `path`, else open the file directly.
    pub fn connect_or_open(path: String) -> anyhow::Result<TaskDb> {
        let api = Api::connect_or_open(Path::new(&path))?;
        let changes = Arc::<Changes>::default();
        if api.is_remote() {
            // Relay the daemon's change events: they cover this app's writes and everyone else's.
            let changes = changes.clone();
            std::thread::Builder::new()
                .name("tt-events".into())
                .spawn(move || {
                    let socket = tt_api::paths::socket();
                    if let Err(e) = tt_api::client::subscribe(&socket, |_| {
                        changes.tick();
                        true
                    }) {
                        eprintln!("tick-alt: lost the daemon's events: {e:#}");
                    }
                })?;
        }
        Ok(Self { api, changes })
    }

    /// True when calls go through `tt-daemon`.
    #[frb(sync)]
    pub fn is_remote(&self) -> bool {
        self.api.is_remote()
    }

    // ----- M0 Inbox page -----

    pub fn inbox(&self) -> anyhow::Result<Vec<TaskItem>> {
        Ok(self.api.inbox()?.into_iter().map(Into::into).collect())
    }

    pub fn add(&self, title: String) -> anyhow::Result<TaskItem> {
        let t = self.write(Request::AddTask { title })?;
        Ok(serde_json::from_value::<Task>(t)?.into())
    }

    /// Returns false if the task was already done.
    pub fn complete(&self, id: String) -> anyhow::Result<bool> {
        Ok(serde_json::from_value(
            self.write(Request::CompleteTask { id })?,
        )?)
    }

    // ----- Change stream -----

    /// Ticks with a new version number after every change: this handle's writes, and on
    /// desktop also every change the daemon reports (other windows, `tt`, outside writes).
    pub fn changes(&self, sink: StreamSink<u32>) {
        let v = self.changes.version.load(Ordering::SeqCst);
        if sink.add(v).is_ok() {
            self.changes.sinks().push(sink);
        }
    }

    // ----- Queries -----

    pub fn sidebar(&self) -> anyhow::Result<Sidebar> {
        self.api.sidebar(now_ms(), local_tz())
    }

    pub fn tasks(&self, view: TaskView) -> anyhow::Result<Vec<TaskRow>> {
        self.api.tasks(&view, now_ms(), local_tz())
    }

    pub fn task(&self, id: String) -> anyhow::Result<Option<TaskDetail>> {
        self.api.task_detail(id, now_ms(), local_tz())
    }

    // ----- Commands -----

    /// Quick add: a plain title, in the current list or tag; everything else lands in Inbox.
    pub fn add_in(&self, view: TaskView, title: String) -> anyhow::Result<String> {
        Ok(serde_json::from_value(
            self.write(Request::AddIn { view, title })?,
        )?)
    }

    pub fn add_subtask(&self, parent_id: String, title: String) -> anyhow::Result<String> {
        let new = NewTask {
            title,
            parent_id: Some(parent_id),
            ..Default::default()
        };
        let t: Task = serde_json::from_value(self.write(Request::CreateTask { new })?)?;
        Ok(t.id)
    }

    /// Complete an open task, or reopen a done one. Returns the new done state.
    pub fn toggle_done(&self, id: String) -> anyhow::Result<bool> {
        Ok(serde_json::from_value(
            self.write(Request::ToggleDone { id })?,
        )?)
    }

    pub fn trash(&self, id: String) -> anyhow::Result<()> {
        self.write(Request::TrashTask { id }).map(drop)
    }

    pub fn restore(&self, id: String) -> anyhow::Result<()> {
        self.write(Request::RestoreTask { id }).map(drop)
    }

    pub fn set_title(&self, id: String, title: String) -> anyhow::Result<()> {
        self.write(Request::SetTitle { id, title }).map(drop)
    }

    pub fn set_notes(&self, id: String, notes: String) -> anyhow::Result<()> {
        self.write(Request::SetNotes { id, notes }).map(drop)
    }

    pub fn set_priority(&self, id: String, priority: u8) -> anyhow::Result<()> {
        self.write(Request::SetPriority { id, priority }).map(drop)
    }

    /// Set the due date from text like `fri`, `tomorrow 14:30` or `2026-12-01`, or clear it.
    pub fn set_due(&self, id: String, text: Option<String>) -> anyhow::Result<()> {
        let tz = tt_api::TzName(local_tz().name().to_string());
        self.write(Request::SetDueText {
            id,
            text,
            now_ms: now_ms(),
            tz,
        })
        .map(drop)
    }

    pub fn set_list(&self, id: String, list_id: Option<String>) -> anyhow::Result<()> {
        self.write(Request::SetTaskList {
            task_id: id,
            list_id,
        })
        .map(drop)
    }

    /// Tag a task by path (`home/garden`); missing tags are created.
    pub fn tag(&self, id: String, path: String) -> anyhow::Result<()> {
        self.write(Request::TagByPath { task_id: id, path })
            .map(drop)
    }

    pub fn untag(&self, id: String, tag_id: String) -> anyhow::Result<()> {
        self.write(Request::UntagTask {
            task_id: id,
            tag_id,
        })
        .map(drop)
    }

    pub fn checklist_add(&self, task_id: String, title: String) -> anyhow::Result<()> {
        self.write(Request::AddChecklistItem { task_id, title })
            .map(drop)
    }

    pub fn checklist_set(&self, item_id: String, done: bool) -> anyhow::Result<()> {
        self.write(Request::SetChecklistDone { id: item_id, done })
            .map(drop)
    }

    pub fn checklist_remove(&self, item_id: String) -> anyhow::Result<()> {
        self.write(Request::RemoveChecklistItem { id: item_id })
            .map(drop)
    }

    pub fn create_list(&self, name: String) -> anyhow::Result<String> {
        Ok(serde_json::from_value(
            self.write(Request::CreateList { name })?,
        )?)
    }

    pub fn set_tag_style(
        &self,
        id: String,
        color: Option<String>,
        icon: Option<String>,
    ) -> anyhow::Result<()> {
        self.write(Request::SetTagStyle { id, color, icon })
            .map(drop)
    }

    /// Run a write. In-process, tick the change stream here; through the daemon its
    /// event does that (and reaches every other client too).
    fn write(&self, req: Request) -> anyhow::Result<serde_json::Value> {
        let v = self.api.call(req)?;
        if !self.api.is_remote() {
            self.changes.tick();
        }
        Ok(v)
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// The desktop database: the same file `tt` and `tt-daemon` use (`$TT_DB`, else
/// `~/.local/share/tick-alt/tt.db` on Linux, `~/Library/Application Support/tick-alt/tt.db`
/// on macOS).
#[frb(sync)]
pub fn desktop_db_path() -> anyhow::Result<String> {
    Ok(tt_api::paths::default_db()?.to_string_lossy().into_owned())
}

#[frb(init)]
pub fn init_app() {
    flutter_rust_bridge::setup_default_user_utils();
}
