//! The Dart-facing API. Keep this thin: the views live in utre-api, the rules in utre-core.
//!
//! On desktop `TaskDb.connectOrOpen` talks to `utre-daemon` over its socket when it serves
//! this database, and opens the file directly otherwise; Android always opens it in-process
//! (plan/15 M2 Desktop Log). The Dart API is the same either way.

use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::{Arc, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use flutter_rust_bridge::frb;
use utre_api::{Api, Request};
use utre_core::{
    Filter, FilterRule, NewTask, ReminderKind, Repeat, RepeatMode, SavedFilter, Store, Task,
    local_tz, parse_when,
};

use crate::changes::Changes;
use crate::frb_generated::StreamSink;
use crate::phone::{self, Phone};

pub use utre_api::{
    CalendarItem, ChecklistRow, NavItem, ReminderRow, Sidebar, TagChip, TaskDetail, TaskRow,
    TaskView, ViewKind,
};

// Dart sees the utre-api view types through these mirrors (one definition, in utre-api).

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
    pub duration_min: Option<i64>,
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
    pub start_at: Option<i64>,
    pub repeat_rule: Option<String>,
    pub repeat_after_completion: bool,
    pub checklist: Vec<ChecklistRow>,
    pub subtasks: Vec<TaskRow>,
    pub reminders: Vec<ReminderRow>,
    pub parent_title: Option<String>,
    pub in_calendar: bool,
}

#[frb(mirror(CalendarItem))]
pub struct _CalendarItem {
    pub task_id: String,
    pub title: String,
    pub start_at: i64,
    pub end_at: i64,
    pub all_day: bool,
    pub done: bool,
    pub movable: bool,
    pub repeating: bool,
    pub priority: u8,
    pub color: Option<String>,
    pub icon: Option<String>,
}

pub use utre_api::{GooglePushStatus, GoogleStatus, SyncOutcome, SyncStatus};
pub use utre_core::{GcalCalendar, GcalEvent, Peer};

#[frb(mirror(GoogleStatus))]
pub struct _GoogleStatus {
    pub configured: bool,
    pub signed_in: bool,
    pub signing_in: bool,
    pub last_sync_ms: Option<i64>,
    pub last_error: Option<String>,
}

#[frb(mirror(GooglePushStatus))]
pub struct _GooglePushStatus {
    pub enabled: bool,
    pub last_push_ms: Option<i64>,
    pub created: usize,
    pub updated: usize,
    pub deleted: usize,
    pub unchanged: usize,
    pub last_error: Option<String>,
}

#[frb(mirror(GcalCalendar))]
pub struct _GcalCalendar {
    pub id: String,
    pub summary: String,
    pub color: Option<String>,
    pub selected: bool,
    pub primary: bool,
    pub access_role: Option<String>,
}

#[frb(mirror(GcalEvent))]
pub struct _GcalEvent {
    pub calendar_id: String,
    pub id: String,
    pub summary: String,
    pub start_at: i64,
    pub end_at: i64,
    pub all_day: bool,
    pub html_link: Option<String>,
    pub color: Option<String>,
    pub location: Option<String>,
    pub description: Option<String>,
}

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
    /// The database file, for the phone's in-process sync node.
    path: String,
    /// Android: the in-process sync node (desktop syncs through utre-daemon instead).
    phone: OnceLock<Arc<Phone>>,
}

impl TaskDb {
    /// Open (or create) the database file at `path` in this process (Android, tests).
    pub fn open(path: String) -> anyhow::Result<TaskDb> {
        Ok(Self {
            api: Arc::new(Api::local(Store::open(&path)?)),
            changes: Arc::default(),
            path,
            phone: OnceLock::new(),
        })
    }

    /// Android: run the sync node in this process (key at `key_path`, shown to peers as
    /// `device`), with auto-sync. Changes it brings in tick the change stream.
    pub fn start_phone_sync(&self, key_path: String, device: String) -> anyhow::Result<()> {
        if self.phone.get().is_some() {
            return Ok(());
        }
        let phone = phone::start(Path::new(&self.path), Path::new(&key_path), device)?;
        let mut events = phone.svc.subscribe();
        let changes = self.changes.clone();
        phone.rt.spawn(async move {
            while let Ok(_event) = events.recv().await {
                // A sync, pairing or trust change wrote through the node's own connection.
                changes.tick();
            }
        });
        let _ = self.phone.set(phone);
        Ok(())
    }

    /// Whether the Devices page can work: through utre-daemon, or the phone's own node.
    #[frb(sync)]
    pub fn sync_available(&self) -> bool {
        self.api.is_remote() || self.phone.get().is_some()
    }

    /// Desktop: use `utre-daemon` if it serves `path`, else open the file directly. A watcher
    /// thread keeps the change stream live either way and follows the daemon coming and
    /// going (systemd restarts it): its events while connected; while opened directly,
    /// a switch back as soon as it serves the file again, and other processes' commits.
    pub fn connect_or_open(path: String) -> anyhow::Result<TaskDb> {
        let api = Arc::new(Api::connect_or_open(Path::new(&path))?);
        let changes = Arc::<Changes>::default();
        let (watch_api, watch_changes) = (api.clone(), changes.clone());
        std::thread::Builder::new()
            .name("utre-watch".into())
            .spawn(move || watch(&watch_api, &watch_changes))?;
        Ok(Self {
            api,
            changes,
            path,
            phone: OnceLock::new(),
        })
    }

    /// True when calls go through `utre-daemon`.
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
    /// desktop also every change the daemon reports (other windows, `utre`, outside writes).
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

    /// How a view is sorted (its saved sort or the default); empty for Completed and Trash.
    pub fn view_sort(&self, view: TaskView) -> anyhow::Result<Vec<String>> {
        self.api.view_sort(&view)
    }

    /// Save how a view is sorted. Lists, tags and filters sync it; the Inbox's stays here.
    pub fn set_view_sort(&self, view: TaskView, sort: Vec<String>) -> anyhow::Result<()> {
        self.write(Request::SetViewSort { view, sort }).map(drop)
    }

    /// Manual order: put a task right before `before` in the view, or last with `None`.
    pub fn place_task(
        &self,
        view: TaskView,
        id: String,
        before: Option<String>,
    ) -> anyhow::Result<()> {
        self.write(Request::PlaceTask {
            view,
            id,
            before,
            now_ms: now_ms(),
            tz: tz_name(),
        })
        .map(drop)
    }

    pub fn task(&self, id: String) -> anyhow::Result<Option<TaskDetail>> {
        self.api.task_detail(id, now_ms(), local_tz())
    }

    // ----- Commands -----

    /// Quick add: a plain title, in the current list or tag. In a filter the task
    /// gets what the filter asks for, so it stays in view. Everything else lands in Inbox.
    pub fn add_in(&self, view: TaskView, title: String) -> anyhow::Result<String> {
        Ok(serde_json::from_value(self.write(Request::AddIn {
            view,
            title,
            now_ms: now_ms(),
            tz: tz_name(),
        })?)?)
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
        self.write(Request::AddReminder { task_id, kind }).map(drop)
    }

    /// A reminder at a date and time typed like `tomorrow 09:00`.
    pub fn add_reminder_at(&self, task_id: String, text: String) -> anyhow::Result<()> {
        let (at, all_day) = parse_when(&text, now_ms(), local_tz())?;
        if all_day {
            anyhow::bail!("give a time too, e.g. \"{} 09:00\"", text.trim());
        }
        let kind = ReminderKind::Absolute { at };
        self.write(Request::AddReminder { task_id, kind }).map(drop)
    }

    pub fn remove_reminder(&self, id: String) -> anyhow::Result<()> {
        self.write(Request::RemoveReminder { id }).map(drop)
    }

    // ----- Filters: definitions cross as JSON (a recursive rule tree; see utre_core::Filter) -----

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

    /// Arrange a saved filter: right before `before`, or last with `None`.
    pub fn place_filter(&self, id: String, before: Option<String>) -> anyhow::Result<()> {
        self.write(Request::PlaceFilter { id, before }).map(drop)
    }

    pub fn delete_filter(&self, id: String) -> anyhow::Result<()> {
        self.write(Request::DeleteFilter { id }).map(drop)
    }

    /// How many open tasks a rule (JSON) matches now. Errors say what's wrong with the rule.
    pub fn rule_count(&self, rule_json: String) -> anyhow::Result<u32> {
        let rule: FilterRule = serde_json::from_str(&rule_json)?;
        self.api.rule_count(&rule, now_ms(), local_tz())
    }

    // ----- Calendar (plan/18) -----

    /// Blocks overlapping `[from_ms, until_ms)` in this device's zone.
    pub fn calendar(&self, from_ms: i64, until_ms: i64) -> anyhow::Result<Vec<CalendarItem>> {
        self.api.calendar(from_ms, until_ms, local_tz())
    }

    /// The Unscheduled panel: a saved filter's tasks, or with `None` every open task
    /// with no date.
    pub fn unscheduled(&self, filter_id: Option<String>) -> anyhow::Result<Vec<TaskRow>> {
        self.api.unscheduled(filter_id, now_ms(), local_tz())
    }

    /// Open tasks `id` can move under, by title (the detail's Parent picker).
    pub fn parent_candidates(&self, id: String) -> anyhow::Result<Vec<TaskRow>> {
        self.api.parent_candidates(id, now_ms(), local_tz())
    }

    /// Make a task a subtask of `parent_id`, or top-level with `None`.
    pub fn set_parent(&self, id: String, parent_id: Option<String>) -> anyhow::Result<()> {
        self.write(Request::SetParent { id, parent_id }).map(drop)
    }

    /// Set the task's length in minutes (calendar block and Google event length), or clear it.
    pub fn set_duration(&self, id: String, minutes: Option<i64>) -> anyhow::Result<()> {
        self.write(Request::SetTaskDuration {
            id,
            duration_min: minutes,
        })
        .map(drop)
    }

    /// "Show in Calendar": keep a task on the calendar and its Unscheduled panel, or off.
    pub fn set_in_calendar(&self, id: String, visible: bool) -> anyhow::Result<()> {
        self.write(Request::SetInCalendar { id, show: visible })
            .map(drop)
    }

    /// Drop, move or resize a block. `end_ms` `None` keeps the task's duration (else
    /// 30 min); `all_day` puts it on the all-day row of `start_ms`'s day.
    pub fn place_block(
        &self,
        id: String,
        start_ms: i64,
        end_ms: Option<i64>,
        all_day: bool,
    ) -> anyhow::Result<()> {
        self.write(Request::PlaceBlock {
            id,
            start_at: start_ms,
            end_at: end_ms,
            all_day,
            tz: tz_name(),
        })
        .map(drop)
    }

    /// Take a task off the calendar (clears its dates). Refused for repeating tasks.
    pub fn unschedule(&self, id: String) -> anyhow::Result<()> {
        self.write(Request::Unschedule { id }).map(drop)
    }

    // ----- Lists and tags -----

    pub fn rename_list(&self, id: String, name: String) -> anyhow::Result<()> {
        self.write(Request::RenameList { id, name }).map(drop)
    }

    /// Arrange a list: right before `before`, or last with `None`.
    pub fn place_list(&self, id: String, before: Option<String>) -> anyhow::Result<()> {
        self.write(Request::PlaceList { id, before }).map(drop)
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

    /// Arrange a tag among its siblings: right before `before`, or last with `None`.
    pub fn place_tag(&self, id: String, before: Option<String>) -> anyhow::Result<()> {
        self.write(Request::PlaceTag { id, before }).map(drop)
    }

    /// A new tag named `name` under `parent_id` (or at the top); an existing sibling with
    /// that name is reused. Returns its id.
    pub fn create_tag(&self, name: String, parent_id: Option<String>) -> anyhow::Result<String> {
        Ok(serde_json::from_value(
            self.write(Request::CreateTag { name, parent_id })?,
        )?)
    }

    /// Find or create every level of a path like `home/garden`. Returns the leaf's id.
    pub fn ensure_tag_path(&self, path: String) -> anyhow::Result<String> {
        Ok(serde_json::from_value(
            self.write(Request::EnsureTagPath { path })?,
        )?)
    }

    /// Re-nest a tag (with the tags under it) under `parent_id`, or at the top with `None`,
    /// right before `before` among its new siblings, or last with `None`.
    pub fn nest_tag(
        &self,
        id: String,
        parent_id: Option<String>,
        before: Option<String>,
    ) -> anyhow::Result<()> {
        self.write(Request::NestTag {
            id,
            parent_id,
            before,
        })
        .map(drop)
    }

    /// Delete a tag and the tags under it; tasks lose them.
    pub fn delete_tag(&self, id: String) -> anyhow::Result<()> {
        self.write(Request::DeleteTag { id }).map(drop)
    }

    // ----- Google Calendar (M5b, plan/19). Desktop: fetching runs in utre-daemon. Android: in this process -----

    /// The phone's own Google service (Kotlin installs it at start), wired to the change stream.
    fn phone_google(&self) -> Option<Arc<crate::google::PhoneGoogle>> {
        let g = crate::google::get()?;
        g.watch(self.changes.clone());
        Some(g)
    }

    /// Whether the Google page can work: through utre-daemon, or the phone's own service.
    #[frb(sync)]
    pub fn google_available(&self) -> bool {
        self.api.is_remote() || crate::google::get().is_some()
    }

    pub fn google_status(&self) -> anyhow::Result<GoogleStatus> {
        match self.phone_google() {
            Some(g) => Ok(g.status()),
            None => self.api.google_status(),
        }
    }

    /// Desktop: opens the browser sign-in; returns its URL (to show if the browser didn't
    /// open). Android signs in through its consent screen (Kotlin), not through here.
    pub fn google_sign_in(&self) -> anyhow::Result<String> {
        if self.phone_google().is_some() {
            anyhow::bail!("the phone signs in through Google's consent screen");
        }
        self.api.google_sign_in()
    }

    pub fn google_sign_out(&self) -> anyhow::Result<()> {
        match self.phone_google() {
            Some(g) => g.sign_out().map_err(anyhow::Error::msg),
            None => self.api.google_sign_out(),
        }
    }

    /// Fetch Google's calendars and events now.
    pub fn google_sync(&self) -> anyhow::Result<()> {
        match self.phone_google() {
            Some(g) => crate::google::runtime()
                .block_on(g.sync_now())
                .map_err(anyhow::Error::msg),
            None => self.api.google_sync(),
        }
    }

    /// Android, when the calendar opens: fetch unless one succeeded less than `max_age_secs`
    /// ago. Does nothing on desktop (utre-daemon polls) or when signed out.
    pub fn google_sync_if_stale(&self, max_age_secs: u32) -> anyhow::Result<()> {
        if let Some(g) = self.phone_google() {
            let max_age = std::time::Duration::from_secs(max_age_secs.into());
            crate::google::runtime()
                .block_on(g.sync_if_stale(max_age))
                .map_err(anyhow::Error::msg)?;
        }
        Ok(())
    }

    /// Whether this device sends tasks with a date to Google's "Tasks" calendar.
    pub fn google_push_enabled(&self) -> anyhow::Result<bool> {
        self.api.gcal_push_enabled()
    }

    /// Turn sending tasks to Google on or off (the daemon pushes after the change).
    pub fn google_push_set(&self, enabled: bool) -> anyhow::Result<()> {
        self.write(Request::GcalPushSet { enabled }).map(drop)
    }

    /// What the last push to Google did.
    pub fn google_push_status(&self) -> anyhow::Result<GooglePushStatus> {
        self.api.google_push_status()
    }

    /// Push tasks to Google's "Tasks" calendar now.
    pub fn google_push_now(&self) -> anyhow::Result<GooglePushStatus> {
        self.api.google_push_now()
    }

    /// Google's calendars as last fetched, with which ones are shown.
    pub fn gcal_calendars(&self) -> anyhow::Result<Vec<GcalCalendar>> {
        self.api.gcal_calendars()
    }

    /// Show or hide a Google calendar's events (hiding drops its cache on this device).
    pub fn gcal_select(&self, calendar_id: String, selected: bool) -> anyhow::Result<()> {
        self.write(Request::GcalSelect {
            calendar_id,
            selected,
        })
        .map(drop)
    }

    /// Cached Google events overlapping the range, read-only, in this device's zone.
    pub fn gcal_events(&self, from_ms: i64, until_ms: i64) -> anyhow::Result<Vec<GcalEvent>> {
        self.api.gcal_events(from_ms, until_ms, local_tz())
    }

    // ----- Devices and sync (answered by utre-daemon's sync node; desktop only) -----

    /// The daemon's sync node: this device's id, peer count, last sync. Errors without a daemon.
    pub fn sync_status(&self) -> anyhow::Result<SyncStatus> {
        match self.local_phone() {
            Some(p) => Ok(p.svc.status()),
            None => self.api.sync_status(),
        }
    }

    pub fn peers(&self) -> anyhow::Result<Vec<Peer>> {
        self.api.peers()
    }

    /// A one-time pairing ticket (`tt2…`, 5 minutes) for another device to join with.
    pub fn sync_pair(&self) -> anyhow::Result<String> {
        match self.local_phone() {
            Some(p) => p.svc.node.pairing_ticket(),
            None => self.api.sync_pair(),
        }
    }

    /// Pair with the device that made `ticket`, then sync.
    pub fn sync_join(&self, ticket: String) -> anyhow::Result<SyncOutcome> {
        match self.local_phone() {
            Some(p) => p.rt.block_on(async {
                let r = p.svc.node.join(ticket.trim()).await?;
                p.svc.refresh_peers().await;
                Ok(utre_sync::service::outcome(r))
            }),
            None => self.api.sync_join(ticket.trim()),
        }
    }

    /// Sync with every paired device now.
    pub fn sync_now(&self) -> anyhow::Result<Vec<SyncOutcome>> {
        match self.local_phone() {
            Some(p) => Ok(p.rt.block_on(p.svc.round())),
            None => self.api.sync_now(),
        }
    }

    /// The phone's node, when calls don't go through utre-daemon.
    fn local_phone(&self) -> Option<&Arc<Phone>> {
        if self.api.is_remote() {
            None
        } else {
            self.phone.get()
        }
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
            if let Some(p) = self.phone.get() {
                p.edited();
            }
        }
        Ok(v)
    }
}

/// The desktop watcher loop (see `TaskDb::connect_or_open`). Runs for the app's lifetime.
fn watch(api: &Api, changes: &Changes) {
    let socket = utre_api::paths::socket();
    loop {
        if api.is_remote() {
            // Blocks while the daemon is up, relaying its change events.
            let _ = utre_api::client::subscribe(&socket, |_| {
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

fn tz_name() -> utre_api::TzName {
    utre_api::TzName(local_tz().name().to_string())
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// The desktop database: the same file `utre` and `utre-daemon` use (`$UTRE_DB`, else
/// `~/.local/share/utre/utre.db` on Linux, `~/Library/Application Support/utre/utre.db`
/// on macOS).
#[frb(sync)]
pub fn desktop_db_path() -> anyhow::Result<String> {
    Ok(utre_api::paths::default_db()?
        .to_string_lossy()
        .into_owned())
}

/// Android: tell the core the device's IANA zone (e.g. `Europe/Sofia`). There's no
/// /etc/localtime there, so "today" would otherwise be UTC. Call once at startup.
#[frb(sync)]
pub fn set_time_zone(name: String) {
    if name.parse::<chrono_tz::Tz>().is_ok() {
        // SAFETY: called once at startup from Dart, before sync or other threads read it,
        // like init_app's own environment setup.
        unsafe { std::env::set_var("UTRE_TZ", name) };
    }
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
