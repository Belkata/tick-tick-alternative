//! The Kotlin-facing API (UniFFI), used by the Android home-screen widget.
//!
//! It lives in the same library as the Dart API so the app ships one copy of
//! the core and SQLite. The widget and the Flutter UI each open their own
//! connection to the same database file; SQLite in WAL mode handles that.

use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

use utre_api::views::{self, TaskView, ViewKind};
use utre_core::{Store, Task};

pub use crate::google::{AccessTokenSource, TokenAnswer};

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum CoreError {
    #[error("{msg}")]
    Failed { msg: String },
}

impl From<utre_core::Error> for CoreError {
    fn from(e: utre_core::Error) -> Self {
        CoreError::Failed { msg: e.to_string() }
    }
}

/// A task as the widget sees it.
#[derive(uniffi::Record)]
pub struct WidgetTask {
    pub id: String,
    pub title: String,
    /// 0 none, 1 low, 2 medium, 3 high.
    pub priority: u8,
    /// Epoch milliseconds.
    pub due_at: Option<i64>,
}

impl From<Task> for WidgetTask {
    fn from(t: Task) -> Self {
        Self {
            id: t.id,
            title: t.title,
            priority: t.priority,
            due_at: t.due_at,
        }
    }
}

/// What a widget shows: the Inbox, or one list, tag or saved filter.
#[derive(Clone, uniffi::Record)]
pub struct WidgetSource {
    /// `inbox`, `list`, `tag` or `filter`.
    pub kind: String,
    /// The list, tag or filter id; empty for the Inbox.
    pub id: String,
    pub name: String,
    /// Open tasks it shows.
    pub count: u32,
    /// Nesting level for tags; 0 otherwise.
    pub depth: u32,
}

/// One row of the widget's task list, already labelled by the core.
#[derive(uniffi::Record)]
pub struct WidgetRow {
    pub id: String,
    pub title: String,
    /// 0 none, 1 low, 2 medium, 3 high.
    pub priority: u8,
    /// `today`, `Fri 14:30`, `3 Dec`...
    pub due_label: Option<String>,
    pub overdue: bool,
    pub due_today: bool,
    /// `#rrggbb` of up to two coloured tags.
    pub tag_colors: Vec<String>,
}

/// A source's tasks, with its name for the widget header.
#[derive(uniffi::Record)]
pub struct WidgetList {
    pub name: String,
    pub rows: Vec<WidgetRow>,
}

/// A reminder for Android's AlarmManager.
#[derive(uniffi::Record)]
pub struct WidgetReminder {
    pub reminder_id: String,
    pub task_id: String,
    pub title: String,
    /// Epoch milliseconds.
    pub fire_at: i64,
}

#[derive(uniffi::Object)]
pub struct WidgetDb {
    store: Mutex<Store>,
}

#[uniffi::export]
impl WidgetDb {
    /// Open (or create) the database file at `path`.
    #[uniffi::constructor]
    pub fn open(path: String) -> Result<Self, CoreError> {
        Ok(Self {
            store: Mutex::new(Store::open(path)?),
        })
    }

    /// The Inbox, sorted like the app's views (the device's sort).
    pub fn inbox(&self) -> Result<Vec<WidgetTask>, CoreError> {
        let s = self.lock();
        let sort = s.device_sort()?;
        Ok(s.inbox_sorted(&sort)?.into_iter().map(Into::into).collect())
    }

    /// Everything a widget can show: the Inbox, then lists, tags and saved filters.
    pub fn sources(&self, tz: String) -> Result<Vec<WidgetSource>, CoreError> {
        let sb = views::sidebar(&self.lock(), now_ms(), parse_tz(&tz))?;
        let mut out = vec![WidgetSource {
            kind: "inbox".into(),
            id: String::new(),
            name: "Inbox".into(),
            count: sb.inbox,
            depth: 0,
        }];
        for (kind, items) in [("list", sb.lists), ("tag", sb.tags), ("filter", sb.filters)] {
            out.extend(items.into_iter().map(|i| WidgetSource {
                kind: kind.into(),
                id: i.id,
                name: i.name,
                count: i.count,
                depth: i.depth,
            }));
        }
        Ok(out)
    }

    /// The open tasks of a source, sorted like the app's views. Fails if the list, tag or
    /// filter no longer exists (the widget then falls back to the Inbox).
    pub fn tasks(&self, kind: String, id: String, tz: String) -> Result<WidgetList, CoreError> {
        let s = self.lock();
        let (now, tz) = (now_ms(), parse_tz(&tz));
        let view = view_of(&kind, &id)?;
        let name = if view.kind == ViewKind::Inbox {
            "Inbox".to_string()
        } else {
            let sb = views::sidebar(&s, now, tz)?;
            sb.lists
                .into_iter()
                .chain(sb.tags)
                .chain(sb.filters)
                .find(|i| i.id == id)
                .map(|i| i.name)
                .ok_or_else(|| CoreError::Failed { msg: format!("no {kind} {id}") })?
        };
        let rows = views::tasks(&s, &view, now, tz)?
            .into_iter()
            .map(|r| WidgetRow {
                id: r.id,
                title: r.title,
                priority: r.priority,
                due_label: r.due_label,
                overdue: r.overdue,
                due_today: r.due_today,
                tag_colors: r.tags.into_iter().filter_map(|t| t.color).take(2).collect(),
            })
            .collect();
        Ok(WidgetList { name, rows })
    }

    /// Quick add: the task lands in the source (list or tag, or what a filter asks for),
    /// else in the Inbox. Returns the new task's id.
    pub fn add_in(&self, kind: String, id: String, title: String, tz: String) -> Result<String, CoreError> {
        let view = view_of(&kind, &id)?;
        Ok(views::add_in(&self.lock(), &view, title.trim(), now_ms(), parse_tz(&tz))?)
    }

    pub fn add(&self, title: String) -> Result<WidgetTask, CoreError> {
        Ok(self.lock().add_task(&title)?.into())
    }

    /// Returns false if the task was already done.
    pub fn complete(&self, id: String) -> Result<bool, CoreError> {
        Ok(self.lock().complete_task(&id)?)
    }

    /// Reminders of open tasks firing in `[from_ms, until_ms)`, soonest first. `tz` is the
    /// device's IANA zone (Android has no /etc/localtime), used for all-day tasks.
    pub fn upcoming_reminders(
        &self,
        from_ms: i64,
        until_ms: i64,
        tz: String,
    ) -> Result<Vec<WidgetReminder>, CoreError> {
        let tz: chrono_tz::Tz = tz.parse().unwrap_or(chrono_tz::Tz::UTC);
        Ok(self
            .lock()
            .upcoming_reminders(from_ms, until_ms, tz)?
            .into_iter()
            .map(|f| WidgetReminder {
                reminder_id: f.reminder_id,
                task_id: f.task_id,
                title: f.title,
                fire_at: f.fire_at,
            })
            .collect())
    }
}

/// Background sync (Android WorkManager): one round with every paired device, through the
/// app's sync node if it's running in this process, else one started now. Returns a short
/// summary for the log.
#[uniffi::export]
pub fn background_sync(db_path: String, key_path: String, device: String) -> Result<String, CoreError> {
    let failed = |e: anyhow::Error| CoreError::Failed { msg: format!("{e:#}") };
    let phone = crate::phone::start(Path::new(&db_path), Path::new(&key_path), device).map_err(failed)?;
    let outcomes = phone.rt.block_on(phone.svc.round());
    let ok = outcomes.iter().filter(|o| o.error.is_none()).count();
    let mut summary = format!("synced with {ok} of {} devices", outcomes.len());
    // Google events, about every 15 minutes (the job's period; expedited runs skip it).
    if let Some(google) = crate::google::get() {
        match crate::google::runtime().block_on(google.sync_if_stale(GOOGLE_STALE)) {
            Ok(true) => summary.push_str(", Google events fetched"),
            Ok(false) => {}
            Err(e) => summary.push_str(&format!(", Google: {e}")),
        }
    }
    Ok(summary)
}

/// A job-triggered Google fetch is skipped when one succeeded this recently.
const GOOGLE_STALE: std::time::Duration = std::time::Duration::from_secs(14 * 60);

/// Install the phone's Google service (Kotlin calls this at app and job start; the first
/// call wins). `source` answers token requests; `configured` says whether this build has its
/// Android OAuth client (plan/19 slice 5).
#[uniffi::export]
pub fn google_init(db_path: String, source: Arc<dyn AccessTokenSource>, configured: bool) {
    crate::google::install(Path::new(&db_path), source, configured);
}

/// The consent screen succeeded: remember the first access token (the app then fetches).
#[uniffi::export]
pub fn google_signed_in(access_token: String) -> Result<(), CoreError> {
    let failed = |msg: String| CoreError::Failed { msg };
    let google = crate::google::get().ok_or_else(|| failed("Google isn't initialised".into()))?;
    google.signed_in(access_token).map_err(failed)
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn parse_tz(tz: &str) -> chrono_tz::Tz {
    tz.parse().unwrap_or(chrono_tz::Tz::UTC)
}

fn view_of(kind: &str, id: &str) -> Result<TaskView, CoreError> {
    let kind = match kind {
        "inbox" => ViewKind::Inbox,
        "list" => ViewKind::List,
        "tag" => ViewKind::Tag,
        "filter" => ViewKind::Filter,
        other => return Err(CoreError::Failed { msg: format!("unknown source kind {other}") }),
    };
    let id = (kind != ViewKind::Inbox).then(|| id.to_string());
    Ok(TaskView { kind, id })
}

impl WidgetDb {
    fn lock(&self) -> MutexGuard<'_, Store> {
        self.store.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> WidgetDb {
        WidgetDb { store: Mutex::new(Store::open_in_memory().unwrap()) }
    }

    #[test]
    fn sources_tasks_and_quick_add_follow_the_source() {
        let db = db();
        let (list, tag) = {
            let s = db.lock();
            (s.create_list("Work").unwrap(), s.create_tag("urgent", None).unwrap())
        };
        let tz = || "Europe/Sofia".to_string();
        db.add("loose".into()).unwrap();
        db.add_in("list".into(), list.clone(), "  report ".into(), tz()).unwrap();
        db.add_in("tag".into(), tag.clone(), "call".into(), tz()).unwrap();

        let kinds: Vec<_> = db.sources(tz()).unwrap().into_iter().map(|s| s.kind).collect();
        assert_eq!(kinds, ["inbox", "list", "tag"]);

        let titles = |kind: &str, id: &str| -> Vec<String> {
            db.tasks(kind.into(), id.into(), tz()).unwrap().rows.into_iter().map(|r| r.title).collect()
        };
        let inbox = titles("inbox", ""); // tagged and listed tasks are not in the derived Inbox
        assert_eq!(inbox, ["loose"]);
        assert_eq!(titles("list", &list), ["report"]);
        assert_eq!(titles("tag", &tag), ["call"]);
        assert_eq!(db.tasks("list".into(), list, tz()).unwrap().name, "Work");
        assert!(db.tasks("list".into(), "gone".into(), tz()).is_err());
    }
}
