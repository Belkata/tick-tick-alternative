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
use tt_core::{
    Filter, FilterRule, NewTask, ReminderKind, Repeat, RepeatMode, SavedFilter, Store, Task,
    local_tz, parse_when,
};

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
    pub repeat_after_completion: bool,
    pub checklist: Vec<ChecklistRow>,
    pub subtasks: Vec<TaskRow>,
    pub reminders: Vec<ReminderRow>,
}

pub use tt_api::{SyncOutcome, SyncStatus};
pub use tt_core::Peer;

#[frb(mirror(SyncOutcome))]
pub struct _SyncOutcome {
    pub peer_id: String,
    pub peer_name: String,
    pub error: Option<String>,
    pub sent: u32,
    pub received: u32,
    pub applied: u32,
}

#[frb(mirror(SyncStatus))]
pub struct _SyncStatus {
    pub node_id: String,
    pub peers: u32,
    pub last_sync_ms: Option<i64>,
    pub last_error: Option<String>,
}

#[frb(mirror(Peer))]
pub struct _Peer {
    pub node_id: String,
    pub name: String,
    pub added_at: i64,
    pub last_seen: Option<i64>,
    pub last_addr: Option<String>,
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
    api: Arc<Api>,
    changes: Arc<Changes>,
}

impl TaskDb {
    /// Open (or create) the database file at `path` in this process (Android, tests).
    pub fn open(path: String) -> anyhow::Result<TaskDb> {
        Ok(Self {
            api: Arc::new(Api::local(Store::open(path)?)),
            changes: Arc::default(),
        })
    }

    /// Desktop: use `tt-daemon` if it serves `path`, else open the file directly. A watcher
    /// thread keeps the change stream live either way and follows the daemon coming and
    /// going (systemd restarts it): its events while connected; while opened directly,
    /// a switch back as soon as it serves the file again, and other processes' commits.
    pub fn connect_or_open(path: String) -> anyhow::Result<TaskDb> {
        let api = Arc::new(Api::connect_or_open(Path::new(&path))?);
        let changes = Arc::<Changes>::default();
        let (watch_api, watch_changes) = (api.clone(), changes.clone());
        std::thread::Builder::new()
            .name("tt-watch".into())
            .spawn(move || watch(&watch_api, &watch_changes))?;
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
        self.write(Request::SetDueText {
            id,
            text,
            now_ms: now_ms(),
            tz: tz_name(),
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

    /// Set the start date from text (like `set_due`), or clear it.
    pub fn set_start(&self, id: String, text: Option<String>) -> anyhow::Result<()> {
        self.write(Request::SetStartText {
            id,
            text,
            now_ms: now_ms(),
            tz: tz_name(),
        })
        .map(drop)
    }

    /// Repeat with an RRULE body (`FREQ=WEEKLY;BYDAY=MO`), or stop with `None`.
    /// `after_completion` counts the next date from when it's done. The task needs a date.
    pub fn set_repeat(
        &self,
        id: String,
        rule: Option<String>,
        after_completion: bool,
    ) -> anyhow::Result<()> {
        let repeat = rule.map(|rule| Repeat {
            rule,
            mode: if after_completion {
                RepeatMode::AfterCompletion
            } else {
                RepeatMode::Fixed
            },
        });
        self.write(Request::SetRepeat { id, repeat }).map(drop)
    }

    /// A reminder `minutes` before the task's start (else due); 0 = at that time.
    pub fn add_reminder_before(&self, task_id: String, minutes: i64) -> anyhow::Result<()> {
        let kind = ReminderKind::Relative {
            offset_min: minutes,
        };
        self.write(Request::AddReminder { task_id, kind })
            .map(drop)
    }

    /// A reminder at a date and time typed like `tomorrow 09:00`.
    pub fn add_reminder_at(&self, task_id: String, text: String) -> anyhow::Result<()> {
        let (at, all_day) = parse_when(&text, now_ms(), local_tz())?;
        if all_day {
            anyhow::bail!("give a time too, e.g. \"{} 09:00\"", text.trim());
        }
        let kind = ReminderKind::Absolute { at };
        self.write(Request::AddReminder { task_id, kind })
            .map(drop)
    }

    pub fn remove_reminder(&self, id: String) -> anyhow::Result<()> {
        self.write(Request::RemoveReminder { id }).map(drop)
    }

    // ----- Filters: definitions cross as JSON (a recursive rule tree; see tt_core::Filter) -----

    /// A saved filter's definition as JSON (`name`, `icon`, `rule`, `sort`, `show_completed`).
    pub fn filter_json(&self, id: String) -> anyhow::Result<Option<String>> {
        let f: Option<SavedFilter> = self.api.find_filter(id)?;
        f.map(|f| Ok(serde_json::to_string(&f.filter)?)).transpose()
    }

    /// Save a new filter from its JSON definition. Returns its id.
    pub fn save_filter(&self, json: String) -> anyhow::Result<String> {
        let filter: Filter = serde_json::from_str(&json)?;
        Ok(serde_json::from_value(
            self.write(Request::SaveFilter { filter })?,
        )?)
    }

    pub fn update_filter(&self, id: String, json: String) -> anyhow::Result<()> {
        let filter: Filter = serde_json::from_str(&json)?;
        self.write(Request::UpdateFilter { id, filter }).map(drop)
    }

    pub fn delete_filter(&self, id: String) -> anyhow::Result<()> {
        self.write(Request::DeleteFilter { id }).map(drop)
    }

    /// How many open tasks a rule (JSON) matches now. Errors say what's wrong with the rule.
    pub fn rule_count(&self, rule_json: String) -> anyhow::Result<u32> {
        let rule: FilterRule = serde_json::from_str(&rule_json)?;
        self.api.rule_count(&rule, now_ms(), local_tz())
    }

    // ----- Lists and tags -----

    pub fn rename_list(&self, id: String, name: String) -> anyhow::Result<()> {
        self.write(Request::RenameList { id, name }).map(drop)
    }

    /// Delete a list. Its tasks stay, with no list.
    pub fn delete_list(&self, id: String) -> anyhow::Result<()> {
        self.write(Request::DeleteList { id }).map(drop)
    }

    pub fn set_list_style(
        &self,
        id: String,
        color: Option<String>,
        icon: Option<String>,
    ) -> anyhow::Result<()> {
        self.write(Request::SetListStyle { id, color, icon })
            .map(drop)
    }

    /// Rename a tag (its last path part).
    pub fn rename_tag(&self, id: String, name: String) -> anyhow::Result<()> {
        self.write(Request::RenameTag { id, name }).map(drop)
    }

    /// Delete a tag and the tags under it; tasks lose them.
    pub fn delete_tag(&self, id: String) -> anyhow::Result<()> {
        self.write(Request::DeleteTag { id }).map(drop)
    }

    // ----- Devices and sync (answered by tt-daemon's sync node; desktop only) -----

    /// The daemon's sync node: this device's id, peer count, last sync. Errors without a daemon.
    pub fn sync_status(&self) -> anyhow::Result<SyncStatus> {
        self.api.sync_status()
    }

    pub fn peers(&self) -> anyhow::Result<Vec<Peer>> {
        self.api.peers()
    }

    /// A one-time pairing ticket (`tt2…`, 5 minutes) for another device to join with.
    pub fn sync_pair(&self) -> anyhow::Result<String> {
        self.api.sync_pair()
    }

    /// Pair with the device that made `ticket`, then sync.
    pub fn sync_join(&self, ticket: String) -> anyhow::Result<SyncOutcome> {
        self.api.sync_join(ticket.trim())
    }

    /// Sync with every paired device now.
    pub fn sync_now(&self) -> anyhow::Result<Vec<SyncOutcome>> {
        self.api.sync_now()
    }

    /// Unpair a device.
    pub fn forget_peer(&self, node_id: String) -> anyhow::Result<bool> {
        Ok(serde_json::from_value(
            self.write(Request::RemovePeer { node_id })?,
        )?)
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

/// The desktop watcher loop (see `TaskDb::connect_or_open`). Runs for the app's lifetime.
fn watch(api: &Api, changes: &Changes) {
    let socket = tt_api::paths::socket();
    loop {
        if api.is_remote() {
            // Blocks while the daemon is up, relaying its change events.
            let _ = tt_api::client::subscribe(&socket, |_| {
                changes.tick();
                true
            });
            // It went away: reload, which makes the next call recover (reconnect or open the file).
            changes.tick();
        } else if api.upgrade() || api.outside_change() {
            changes.tick();
            continue;
        }
        std::thread::sleep(std::time::Duration::from_secs(2));
    }
}

fn tz_name() -> tt_api::TzName {
    tt_api::TzName(local_tz().name().to_string())
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
    // Capturing a Rust backtrace segfaults inside the Flutter process: libflutter_linux_gtk.so
    // bundles its own libunwind, so `_Unwind_GetIP` resolves there while `_Unwind_Backtrace`
    // comes from libgcc_s. anyhow captures one for every error when RUST_BACKTRACE is set,
    // and setup_default_user_utils sets it to 1 if it's unset; so set it to 0 first.
    // (plan/15, slice 2: the app crashed on start whenever no daemon was running.)
    for var in ["RUST_BACKTRACE", "RUST_LIB_BACKTRACE"] {
        if std::env::var_os(var).is_none() {
            // SAFETY: runs once from RustLib.init, before this library starts threads of its
            // own; flutter_rust_bridge's setup below sets RUST_BACKTRACE the same way.
            unsafe { std::env::set_var(var, "0") };
        }
    }
    flutter_rust_bridge::setup_default_user_utils();
}
