//! What the UIs show: the sidebar, task rows and the task detail, computed from the core.
//! Shared by the daemon (served over the socket) and in-process users (Android, tests).

use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use tt_core::{
    FilterRule, NewTask, ReminderKind, Schedule, Status, Store, Task, is_overdue, local_day,
    offset_label, parse_when, when_label,
};

/// Which kind of view the task list shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewKind {
    Inbox,
    List,
    Tag,
    Filter,
    Completed,
    Trash,
}

/// What the task list shows. `id` names the list, tag or filter; it's unused otherwise.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskView {
    pub kind: ViewKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

impl TaskView {
    fn id(&self) -> tt_core::Result<&str> {
        self.id
            .as_deref()
            .ok_or_else(|| tt_core::Error::Invalid(format!("{:?} view needs an id", self.kind)))
    }
}

/// A tag as a chip on a task card.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TagChip {
    pub id: String,
    pub name: String,
    pub path: String,
    /// `#rrggbb`, or none for the default.
    pub color: Option<String>,
    pub icon: Option<String>,
}

/// One task card.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sidebar {
    pub inbox: u32,
    pub lists: Vec<NavItem>,
    pub filters: Vec<NavItem>,
    pub tags: Vec<NavItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChecklistRow {
    pub id: String,
    pub title: String,
    pub done: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReminderRow {
    pub id: String,
    /// `15m before`, `today 09:00`…
    pub label: String,
}

/// One block on the calendar (see `tt_core::Block`), with what its tile shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalendarItem {
    pub task_id: String,
    pub title: String,
    /// Epoch ms; all-day items run from local midnight to the next one.
    pub start_at: i64,
    /// Exclusive.
    pub end_at: i64,
    pub all_day: bool,
    pub done: bool,
    /// The open task's current occurrence: it can be moved and resized.
    pub movable: bool,
    pub repeating: bool,
    pub priority: u8,
    /// `#rrggbb` of the first coloured tag, else the list's; none for the default accent.
    pub color: Option<String>,
    pub icon: Option<String>,
}

/// Everything the detail pane shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskDetail {
    pub row: TaskRow,
    pub notes: String,
    pub list_id: Option<String>,
    pub start_label: Option<String>,
    /// The RRULE body, e.g. `FREQ=WEEKLY;BYDAY=MO`.
    pub repeat_rule: Option<String>,
    /// The next date counts from completion rather than from the schedule.
    pub repeat_after_completion: bool,
    pub checklist: Vec<ChecklistRow>,
    pub subtasks: Vec<TaskRow>,
    pub reminders: Vec<ReminderRow>,
}

fn rule(v: serde_json::Value) -> tt_core::Result<FilterRule> {
    Ok(serde_json::from_value(v)?)
}

pub fn sidebar(s: &Store, now: i64, tz: Tz) -> tt_core::Result<Sidebar> {
    let count = |r: serde_json::Value| -> tt_core::Result<u32> {
        Ok(s.run_rule(&rule(r)?, &[], false, now, tz)?.len() as u32)
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
        .collect::<tt_core::Result<_>>()?;
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
        .collect::<tt_core::Result<_>>()?;
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
        .collect::<tt_core::Result<_>>()?;
    Ok(Sidebar {
        inbox: s.inbox()?.len() as u32,
        lists,
        filters,
        tags,
    })
}

pub fn tasks(s: &Store, view: &TaskView, now: i64, tz: Tz) -> tt_core::Result<Vec<TaskRow>> {
    let run = |r: serde_json::Value| s.run_rule(&rule(r)?, &[], false, now, tz);
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
                let mut row = task_row(s, t, now, tz)?;
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
            let mut row = task_row(s, t, now, tz)?;
            if view.kind == ViewKind::Trash {
                row.when_label = deleted_at.map(|d| when_label(d, false, now, tz));
            }
            Ok(row)
        })
        .collect()
}

pub fn task_detail(s: &Store, id: &str, now: i64, tz: Tz) -> tt_core::Result<Option<TaskDetail>> {
    let Some(t) = s.get_task(id)? else {
        return Ok(None);
    };
    let (notes, list_id, repeat_rule) = (t.notes.clone(), t.list_id.clone(), t.repeat_rule.clone());
    let repeat_after_completion = t.repeat_mode == tt_core::RepeatMode::AfterCompletion;
    let start_label = t.start_at.map(|ms| when_label(ms, t.all_day, now, tz));
    let checklist = s
        .checklist(id)?
        .into_iter()
        .map(|c| ChecklistRow {
            id: c.id,
            title: c.title,
            done: c.done,
        })
        .collect();
    let subtasks = s
        .subtasks(id)?
        .into_iter()
        .map(|t| task_row(s, t, now, tz))
        .collect::<tt_core::Result<_>>()?;
    let reminders = s
        .reminders(id)?
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
        row: task_row(s, t, now, tz)?,
        notes,
        list_id,
        start_label,
        repeat_rule,
        repeat_after_completion,
        checklist,
        subtasks,
        reminders,
    }))
}

/// Quick add: a plain title, in the current list or tag; everything else lands in Inbox.
pub fn add_in(s: &Store, view: &TaskView, title: &str) -> tt_core::Result<String> {
    let mut new = NewTask {
        title: title.to_string(),
        ..Default::default()
    };
    match view.kind {
        ViewKind::List => new.list_id = view.id.clone(),
        ViewKind::Tag => new.tags = view.id.clone().into_iter().collect(),
        _ => {}
    }
    Ok(s.create_task(&new)?.id)
}

/// Complete an open task, or reopen a done one. Returns the new done state
/// (a repeating task moves to its next date and stays open).
pub fn toggle_done(s: &Store, id: &str) -> tt_core::Result<bool> {
    let t = s
        .get_task(id)?
        .ok_or_else(|| tt_core::Error::NotFound(format!("task {id}")))?;
    if t.status == Status::Done {
        s.reopen_task(id)?;
        return Ok(false);
    }
    s.complete_task(id)?;
    Ok(s.get_task(id)?.is_some_and(|t| t.status == Status::Done))
}

/// Set the due date from text like `fri`, `tomorrow 14:30` or `2026-12-01`, or clear it.
/// Switching between all-day and timed drops the start date rather than mixing kinds.
pub fn set_due(s: &Store, id: &str, text: Option<&str>, now: i64, tz: Tz) -> tt_core::Result<()> {
    set_date(s, id, DateField::Due, text, now, tz)
}

/// The same for the start date (it drops the due date on a kind switch).
pub fn set_start(s: &Store, id: &str, text: Option<&str>, now: i64, tz: Tz) -> tt_core::Result<()> {
    set_date(s, id, DateField::Start, text, now, tz)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DateField {
    Due,
    Start,
}

fn set_date(
    s: &Store,
    id: &str,
    field: DateField,
    text: Option<&str>,
    now: i64,
    tz: Tz,
) -> tt_core::Result<()> {
    let text = text.map(str::trim).filter(|t| !t.is_empty());
    let parsed = text.map(|t| parse_when(t, now, tz)).transpose()?;
    let t = s
        .get_task(id)?
        .ok_or_else(|| tt_core::Error::NotFound(format!("task {id}")))?;
    let mut sch = Schedule {
        start_at: t.start_at,
        due_at: t.due_at,
        all_day: t.all_day,
        tz: t.tz.clone(),
        duration_min: t.duration_min,
    };
    let (this, other) = match field {
        DateField::Due => (&mut sch.due_at, &mut sch.start_at),
        DateField::Start => (&mut sch.start_at, &mut sch.due_at),
    };
    *this = None;
    if let Some((ms, all_day)) = parsed {
        if all_day != t.all_day {
            *other = None;
        }
        *this = Some(ms);
        sch.all_day = all_day;
    }
    sch.tz = (!sch.all_day).then(|| tz.name().to_string());
    s.set_schedule(id, Some(&sch))
}

/// The calendar's blocks overlapping `[from, until)`, with titles and colours.
pub fn calendar(s: &Store, from: i64, until: i64, tz: Tz) -> tt_core::Result<Vec<CalendarItem>> {
    let mut cache: std::collections::HashMap<String, (Task, Option<String>, Option<String>)> =
        Default::default();
    let mut out = Vec::new();
    for b in s.calendar(from, until, tz)? {
        if !cache.contains_key(&b.task_id) {
            let Some(t) = s.get_task(&b.task_id)? else {
                continue;
            };
            let tags = s.task_tags(&t.id)?;
            let tag = tags.iter().find(|g| g.color.is_some());
            let list = match &t.list_id {
                Some(l) => s.find_list(l)?,
                None => None,
            };
            let color = tag
                .and_then(|g| g.color.clone())
                .or_else(|| list.as_ref().and_then(|l| l.color.clone()));
            let icon = tag
                .and_then(|g| g.icon.clone())
                .or_else(|| tags.iter().find_map(|g| g.icon.clone()))
                .or_else(|| list.and_then(|l| l.icon));
            cache.insert(b.task_id.clone(), (t, color, icon));
        }
        let (t, color, icon) = &cache[&b.task_id];
        out.push(CalendarItem {
            title: t.title.clone(),
            priority: t.priority,
            color: color.clone(),
            icon: icon.clone(),
            task_id: b.task_id,
            start_at: b.start_at,
            end_at: b.end_at,
            all_day: b.all_day,
            done: b.done,
            movable: b.movable,
            repeating: b.repeating,
        });
    }
    Ok(out)
}

/// The calendar's Unscheduled panel: a saved filter's tasks, or by default every open
/// task with no start or due.
pub fn unscheduled(
    s: &Store,
    filter_id: Option<&str>,
    now: i64,
    tz: Tz,
) -> tt_core::Result<Vec<TaskRow>> {
    let tasks = match filter_id {
        Some(id) => s.run_filter(id, now, tz)?,
        None => s.run_rule(
            &rule(serde_json::json!({"all": [
                {"field": "due", "op": "none"},
                {"field": "start", "op": "none"},
            ]}))?,
            &[],
            false,
            now,
            tz,
        )?,
    };
    tasks.into_iter().map(|t| task_row(s, t, now, tz)).collect()
}

/// How many open tasks a rule matches right now (the filter builder's live count).
pub fn rule_count(s: &Store, rule: &FilterRule, now: i64, tz: Tz) -> tt_core::Result<u32> {
    Ok(s.run_rule(rule, &[], false, now, tz)?.len() as u32)
}

fn task_row(s: &Store, t: Task, now: i64, tz: Tz) -> tt_core::Result<TaskRow> {
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
        due_today: t
            .due_at
            .is_some_and(|d| local_day(d, t.all_day, tz) == today),
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
