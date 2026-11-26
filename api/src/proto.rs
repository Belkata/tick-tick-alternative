//! The request protocol: one JSON object per line, tagged by `cmd`.
//!
//! Every core call the UIs and `utre` need is one [`Request`] variant, generated from the
//! table below together with its typed [`Api`](crate::Api) method and its dispatch arm in
//! [`handle`]. The same `handle` serves the daemon's socket and in-process callers, so the
//! two paths can't drift apart.
//!
//! ```text
//! → {"cmd":"set_title","id":"0195…","title":"Buy oat milk"}
//! ← {"ok":null}
//! → {"cmd":"tasks","view":{"kind":"inbox"},"now_ms":1795…,"tz":"Europe/Sofia"}
//! ← {"ok":[{"id":"0195…","title":"Call the plumber",…}]}
//! → {"cmd":"subscribe"}                    (that connection then only receives events)
//! ← {"event":"changed","version":7}
//! ```

use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utre_core::{
    ChecklistItem, Completion, Filter, FilterRule, GcalCalendar, GcalEvent, List, NewTask, Peer,
    Reminder, ReminderFire, ReminderKind, Repeat, SavedFilter, Schedule, Store, Tag, Task,
};

use crate::views::{self, CalendarItem, Sidebar, TaskDetail, TaskRow, TaskView};

/// A zone on the wire, by IANA name. The caller's zone decides "today", not the daemon's.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TzName(pub String);

impl TzName {
    pub fn tz(&self) -> Tz {
        self.0.parse().unwrap_or(Tz::UTC)
    }
}

/// How an `Api` argument becomes its wire field: `&str` → `String`, `&Repeat` → `Repeat`…
pub trait IntoWire<W> {
    fn into_wire(self) -> W;
}

impl<T: Into<String>> IntoWire<String> for T {
    fn into_wire(self) -> String {
        self.into()
    }
}

impl IntoWire<Option<String>> for Option<&str> {
    fn into_wire(self) -> Option<String> {
        self.map(str::to_string)
    }
}

impl IntoWire<Option<String>> for Option<String> {
    fn into_wire(self) -> Option<String> {
        self
    }
}

impl IntoWire<Option<String>> for Option<&String> {
    fn into_wire(self) -> Option<String> {
        self.cloned()
    }
}

impl IntoWire<Vec<String>> for &[String] {
    fn into_wire(self) -> Vec<String> {
        self.to_vec()
    }
}

impl<const N: usize> IntoWire<Vec<String>> for &[String; N] {
    fn into_wire(self) -> Vec<String> {
        self.to_vec()
    }
}

impl IntoWire<Vec<String>> for Vec<String> {
    fn into_wire(self) -> Vec<String> {
        self
    }
}

impl IntoWire<TzName> for Tz {
    fn into_wire(self) -> TzName {
        TzName(self.name().to_string())
    }
}

macro_rules! wire_by_ref {
    ($($t:ty),*) => {$(
        impl IntoWire<$t> for &$t {
            fn into_wire(self) -> $t {
                self.clone()
            }
        }
        impl IntoWire<Option<$t>> for Option<&$t> {
            fn into_wire(self) -> Option<$t> {
                self.cloned()
            }
        }
    )*};
}
wire_by_ref!(Repeat, Schedule, Filter, FilterRule, NewTask, TaskView);

macro_rules! wire_by_value {
    ($($t:ty),*) => {$(
        impl IntoWire<$t> for $t {
            fn into_wire(self) -> $t {
                self
            }
        }
    )*};
}
wire_by_value!(u8, bool, i64, usize, ReminderKind, Option<i64>);

/// The table: `read|write Variant name(field: WireType, …) -> Ret => |store| expression;`
macro_rules! api {
    ($( $rw:ident $variant:ident $name:ident ( $($f:ident : $w:ty),* ) -> $ret:ty => |$s:ident| $call:expr ; )*) => {
        /// One call to the core.
        #[derive(Debug, Clone, Serialize, Deserialize)]
        #[serde(tag = "cmd", rename_all = "snake_case")]
        pub enum Request {
            /// Which database this daemon serves, its change version and pid.
            Hello,
            /// Turn this connection into an event stream (`{"event":"changed","version":n}`).
            Subscribe,
            /// Sync control, answered by the daemon's sync node (see `server::SyncControl`):
            /// a one-time pairing ticket ([`String`]).
            SyncPair,
            /// Pair with the device that made `ticket`, then sync ([`SyncOutcome`]).
            SyncJoin { ticket: String },
            /// Sync with every paired device now (`Vec<SyncOutcome>`).
            SyncNow,
            /// The sync node's state ([`SyncStatus`]).
            SyncStatus,
            /// Google control, answered by the daemon's Google service: its state
            /// ([`GoogleStatus`]).
            GoogleStatus,
            /// Start the browser sign-in; answers with the URL once the browser was asked to
            /// open it ([`String`]). The sign-in finishes in the background.
            GoogleSignIn,
            /// Forget this device's Google tokens and cached events.
            GoogleSignOut,
            /// Fetch Google's calendars and events now.
            GoogleSync,
            /// What pushing tasks to Google did last ([`GooglePushStatus`]).
            GooglePushStatus,
            /// Push tasks to Google's "Tasks" calendar now ([`GooglePushStatus`]).
            GooglePushNow,
            $( $variant { $($f: $w),* }, )*
        }

        impl Request {
            /// Does it change the database (so subscribers hear about it)?
            pub fn is_write(&self) -> bool {
                match self {
                    Request::Hello | Request::Subscribe => false,
                    // Sync writes reach the file through the sync node's own connection.
                    Request::SyncPair | Request::SyncJoin { .. } | Request::SyncNow | Request::SyncStatus => false,
                    Request::GoogleStatus | Request::GoogleSignIn | Request::GoogleSignOut | Request::GoogleSync | Request::GooglePushStatus | Request::GooglePushNow => false,
                    $( Request::$variant { .. } => api!(@is_write $rw), )*
                }
            }
        }

        /// Run a request against the store. `Hello` and `Subscribe` belong to the server.
        pub fn handle(store: &Store, req: Request) -> utre_core::Result<Value> {
            Ok(match req {
                Request::Hello | Request::Subscribe => {
                    return Err(utre_core::Error::Invalid("only the daemon answers hello and subscribe".into()))
                }
                Request::SyncPair | Request::SyncJoin { .. } | Request::SyncNow | Request::SyncStatus => {
                    return Err(utre_core::Error::Invalid("sync runs in utre-daemon, which isn't serving this database".into()))
                }
                Request::GoogleStatus | Request::GoogleSignIn | Request::GoogleSignOut | Request::GoogleSync | Request::GooglePushStatus | Request::GooglePushNow => {
                    return Err(utre_core::Error::Invalid("Google runs in utre-daemon, which isn't serving this database".into()))
                }
                $( Request::$variant { $($f),* } => {
                    let $s = store;
                    serde_json::to_value::<$ret>($call?)?
                } )*
            })
        }

        impl crate::Api {
            $(
                pub fn $name(&self $(, $f: impl IntoWire<$w>)*) -> anyhow::Result<$ret> {
                    self.call(Request::$variant { $($f: $f.into_wire()),* })
                }
            )*
        }
    };
    (@is_write read) => { false };
    (@is_write write) => { true };
}

api! {
    // ----- Tasks: reads -----
    read Inbox inbox() -> Vec<Task> => |s| s.inbox();
    read InboxSorted inbox_sorted(sort: Vec<String>) -> Vec<Task> => |s| s.inbox_sorted(&sort);
    read DeviceSort device_sort() -> Vec<String> => |s| s.device_sort();
    write SetDeviceSort set_device_sort(sort: Vec<String>) -> () => |s| s.set_device_sort(&sort);
    read OpenTasks open_tasks() -> Vec<Task> => |s| s.open_tasks();
    read GetTask get_task(id: String) -> Option<Task> => |s| s.get_task(&id);
    read ResolveTaskId resolve_task_id(suffix: String) -> Option<String> => |s| s.resolve_task_id(&suffix);
    read Subtasks subtasks(parent_id: String) -> Vec<Task> => |s| s.subtasks(&parent_id);
    read Trash trash() -> Vec<Task> => |s| s.trash();
    read Completed completed(limit: usize) -> Vec<Completion> => |s| s.completed(limit);
    read TaskTags task_tags(task_id: String) -> Vec<Tag> => |s| s.task_tags(&task_id);
    read Checklist checklist(task_id: String) -> Vec<ChecklistItem> => |s| s.checklist(&task_id);
    read Reminders reminders(task_id: String) -> Vec<Reminder> => |s| s.reminders(&task_id);
    read UpcomingReminders upcoming_reminders(from: i64, until: i64, tz: TzName) -> Vec<ReminderFire>
        => |s| s.upcoming_reminders(from, until, tz.tz());

    // ----- Tasks: writes -----
    write AddTask add_task(title: String) -> Task => |s| s.add_task(&title);
    write CreateTask create_task(new: NewTask) -> Task => |s| s.create_task(&new);
    write SetTitle set_title(id: String, title: String) -> () => |s| s.set_title(&id, &title);
    write SetNotes set_notes(id: String, notes: String) -> () => |s| s.set_notes(&id, &notes);
    write SetPriority set_priority(id: String, priority: u8) -> () => |s| s.set_priority(&id, priority);
    write SetSchedule set_schedule(id: String, schedule: Option<Schedule>) -> ()
        => |s| s.set_schedule(&id, schedule.as_ref());
    write SetTaskDuration set_task_duration(id: String, duration_min: Option<i64>) -> ()
        => |s| s.set_task_duration(&id, duration_min);
    write SetTaskDue set_task_due(id: String, due_at: Option<i64>) -> () => |s| s.set_task_due(&id, due_at);
    write SetParent set_parent(id: String, parent_id: Option<String>) -> ()
        => |s| s.set_parent(&id, parent_id.as_deref());
    write SetInCalendar set_in_calendar(id: String, show: bool) -> () => |s| s.set_in_calendar(&id, show);
    write SetRepeat set_repeat(id: String, repeat: Option<Repeat>) -> () => |s| s.set_repeat(&id, repeat.as_ref());
    write CompleteTask complete_task(id: String) -> bool => |s| s.complete_task(&id);
    write ReopenTask reopen_task(id: String) -> bool => |s| s.reopen_task(&id);
    write TrashTask trash_task(id: String) -> bool => |s| s.trash_task(&id);
    write RestoreTask restore_task(id: String) -> bool => |s| s.restore_task(&id);

    // ----- Lists -----
    read Lists lists(include_archived: bool) -> Vec<List> => |s| s.lists(include_archived);
    read FindList find_list(id_or_name: String) -> Option<List> => |s| s.find_list(&id_or_name);
    write CreateList create_list(name: String) -> String => |s| s.create_list(&name);
    write RenameList rename_list(id: String, name: String) -> () => |s| s.rename_list(&id, &name);
    write SetListArchived set_list_archived(id: String, archived: bool) -> ()
        => |s| s.set_list_archived(&id, archived);
    write SetListStyle set_list_style(id: String, color: Option<String>, icon: Option<String>) -> ()
        => |s| s.set_list_style(&id, color.as_deref(), icon.as_deref());
    write PlaceList place_list(id: String, before: Option<String>) -> () => |s| s.place_list(&id, before.as_deref());
    write DeleteList delete_list(id: String) -> () => |s| s.delete_list(&id);
    write SetTaskList set_task_list(task_id: String, list_id: Option<String>) -> ()
        => |s| s.set_task_list(&task_id, list_id.as_deref());

    // ----- Tags -----
    read Tags tags() -> Vec<Tag> => |s| s.tags();
    read FindTag find_tag(id_or_path: String) -> Option<Tag> => |s| s.find_tag(&id_or_path);
    write CreateTag create_tag(name: String, parent_id: Option<String>) -> String
        => |s| s.create_tag(&name, parent_id.as_deref());
    write EnsureTagPath ensure_tag_path(path: String) -> String => |s| s.ensure_tag_path(&path);
    write RenameTag rename_tag(id: String, name: String) -> () => |s| s.rename_tag(&id, &name);
    write MoveTag move_tag(id: String, parent_id: Option<String>) -> () => |s| s.move_tag(&id, parent_id.as_deref());
    write PlaceTag place_tag(id: String, before: Option<String>) -> () => |s| s.place_tag(&id, before.as_deref());
    write DeleteTag delete_tag(id: String) -> () => |s| s.delete_tag(&id);
    write SetTagStyle set_tag_style(id: String, color: Option<String>, icon: Option<String>) -> ()
        => |s| s.set_tag_style(&id, color.as_deref(), icon.as_deref());
    write TagTask tag_task(task_id: String, tag_id: String) -> () => |s| s.tag_task(&task_id, &tag_id);
    write UntagTask untag_task(task_id: String, tag_id: String) -> () => |s| s.untag_task(&task_id, &tag_id);

    // ----- Checklist and reminders -----
    write AddChecklistItem add_checklist_item(task_id: String, title: String) -> String
        => |s| s.add_checklist_item(&task_id, &title);
    write SetChecklistDone set_checklist_done(id: String, done: bool) -> () => |s| s.set_checklist_done(&id, done);
    write RenameChecklistItem rename_checklist_item(id: String, title: String) -> ()
        => |s| s.rename_checklist_item(&id, &title);
    write RemoveChecklistItem remove_checklist_item(id: String) -> () => |s| s.remove_checklist_item(&id);
    write AddReminder add_reminder(task_id: String, kind: ReminderKind) -> String
        => |s| s.add_reminder(&task_id, kind);
    write RemoveReminder remove_reminder(id: String) -> () => |s| s.remove_reminder(&id);

    // ----- Filters -----
    read Filters filters() -> Vec<SavedFilter> => |s| s.filters();
    read FindFilter find_filter(id_or_name: String) -> Option<SavedFilter> => |s| s.find_filter(&id_or_name);
    read RunFilter run_filter(id: String, now_ms: i64, tz: TzName) -> Vec<Task> => |s| s.run_filter(&id, now_ms, tz.tz());
    read RunRule run_rule(rule: FilterRule, sort: Vec<String>, show_completed: bool, now_ms: i64, tz: TzName) -> Vec<Task>
        => |s| s.run_rule(&rule, &sort, show_completed, now_ms, tz.tz());
    write SaveFilter save_filter(filter: Filter) -> String => |s| s.save_filter(&filter);
    write UpdateFilter update_filter(id: String, filter: Filter) -> () => |s| s.update_filter(&id, &filter);
    write PlaceFilter place_filter(id: String, before: Option<String>) -> () => |s| s.place_filter(&id, before.as_deref());
    write DeleteFilter delete_filter(id: String) -> () => |s| s.delete_filter(&id);

    // ----- Paired devices (local to this device; sync itself is SyncPair/SyncNow…) -----
    read Peers peers() -> Vec<Peer> => |s| s.peers();
    write RemovePeer remove_peer(node_id: String) -> bool => |s| s.remove_peer(&node_id);
    // Op-log maintenance; changes no visible state, so it isn't announced as a write.
    read Compact compact() -> usize => |s| s.compact();
    read OpCount op_count() -> usize => |s| s.op_count();

    // ----- Google Calendar cache (per device; the fetching itself is Google* in the daemon) -----
    read GcalCalendars gcal_calendars() -> Vec<GcalCalendar> => |s| s.gcal_calendars();
    write GcalSelect gcal_select(calendar_id: String, selected: bool) -> ()
        => |s| s.gcal_select(&calendar_id, selected);
    read GcalPushEnabled gcal_push_enabled() -> bool => |s| s.gcal_push_enabled();
    write GcalPushSet gcal_push_set(enabled: bool) -> () => |s| s.gcal_set_push(enabled);
    read GcalEvents gcal_events(from: i64, until: i64, tz: TzName) -> Vec<GcalEvent>
        => |s| s.gcal_events(from, until, tz.tz());

    // ----- Views (what the app shows) -----
    read Sidebar sidebar(now_ms: i64, tz: TzName) -> Sidebar => |s| views::sidebar(s, now_ms, tz.tz());
    read Tasks tasks(view: TaskView, now_ms: i64, tz: TzName) -> Vec<TaskRow>
        => |s| views::tasks(s, &view, now_ms, tz.tz());
    read ViewSort view_sort(view: TaskView) -> Vec<String> => |s| views::view_sort(s, &view);
    write SetViewSort set_view_sort(view: TaskView, sort: Vec<String>) -> ()
        => |s| views::set_view_sort(s, &view, &sort);
    write PlaceTask place_task(view: TaskView, id: String, before: Option<String>, now_ms: i64, tz: TzName) -> ()
        => |s| views::place_task(s, &view, &id, before.as_deref(), now_ms, tz.tz());
    read TaskDetail task_detail(id: String, now_ms: i64, tz: TzName) -> Option<TaskDetail>
        => |s| views::task_detail(s, &id, now_ms, tz.tz());
    write AddIn add_in(view: TaskView, title: String, now_ms: i64, tz: TzName) -> String
        => |s| views::add_in(s, &view, &title, now_ms, tz.tz());
    write ToggleDone toggle_done(id: String) -> bool => |s| views::toggle_done(s, &id);
    write SetDueText set_due_text(id: String, text: Option<String>, now_ms: i64, tz: TzName) -> ()
        => |s| views::set_due(s, &id, text.as_deref(), now_ms, tz.tz());
    write SetStartText set_start_text(id: String, text: Option<String>, now_ms: i64, tz: TzName) -> ()
        => |s| views::set_start(s, &id, text.as_deref(), now_ms, tz.tz());
    read RuleCount rule_count(rule: FilterRule, now_ms: i64, tz: TzName) -> u32
        => |s| views::rule_count(s, &rule, now_ms, tz.tz());
    read Calendar calendar(from: i64, until: i64, tz: TzName) -> Vec<CalendarItem>
        => |s| views::calendar(s, from, until, tz.tz());
    read Unscheduled unscheduled(filter_id: Option<String>, now_ms: i64, tz: TzName) -> Vec<TaskRow>
        => |s| views::unscheduled(s, filter_id.as_deref(), now_ms, tz.tz());
    write PlaceBlock place_block(id: String, start_at: i64, end_at: Option<i64>, all_day: bool, tz: TzName) -> ()
        => |s| s.place_block(&id, start_at, end_at, all_day, tz.tz());
    write Unschedule unschedule(id: String) -> () => |s| s.unschedule(&id);
    read ParentCandidates parent_candidates(id: String, now_ms: i64, tz: TzName) -> Vec<TaskRow>
        => |s| views::parent_candidates(s, &id, now_ms, tz.tz());
    write TagByPath tag_by_path(task_id: String, path: String) -> ()
        => |s| { let tag = s.ensure_tag_path(&path)?; s.tag_task(&task_id, &tag) };
}

/// What `hello` answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hello {
    /// The database file the daemon owns (canonical path).
    pub db: String,
    /// Bumped after every change, including other processes' writes it noticed.
    pub version: u64,
    pub pid: u32,
}

/// A reply line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reply {
    Ok(Value),
    Err(String),
}

/// An event line on a subscribed connection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    Changed { version: u64 },
}

/// One device's result from `sync_now` or `sync_join`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncOutcome {
    pub peer_id: String,
    pub peer_name: String,
    /// The error when the sync failed (the device is offline, refused…).
    pub error: Option<String>,
    pub sent: u32,
    pub received: u32,
    pub applied: u32,
}

/// What `sync_status` answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncStatus {
    /// This device's iroh endpoint id.
    pub node_id: String,
    pub peers: u32,
    /// Epoch ms of the last round that reached at least one device.
    pub last_sync_ms: Option<i64>,
    /// Why the last round failed, if it reached nobody.
    pub last_error: Option<String>,
}

/// What `google_push_status` and `google_push_now` answer: tasks out to Google (plan/19).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct GooglePushStatus {
    /// This device pushes (the "Send timed tasks to Google" switch).
    pub enabled: bool,
    /// Epoch ms of the last push that went through without errors.
    pub last_push_ms: Option<i64>,
    /// Events created, updated and deleted by the last push, and how many were up to date.
    pub created: usize,
    pub updated: usize,
    pub deleted: usize,
    pub unchanged: usize,
    /// Why the last push failed, if it did.
    pub last_error: Option<String>,
}

/// What `google_status` answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoogleStatus {
    /// The OAuth client file (`google-client.json`) was found, so signing in is possible.
    pub configured: bool,
    pub signed_in: bool,
    /// The browser sign-in is open and waiting.
    pub signing_in: bool,
    /// Epoch ms of the last successful fetch.
    pub last_sync_ms: Option<i64>,
    /// Why the last attempt failed, if it did.
    pub last_error: Option<String>,
}
