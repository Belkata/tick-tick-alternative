//! What the UIs show: the sidebar, task rows and the task detail, computed from the core.
//! Shared by the daemon (served over the socket) and in-process users (Android, tests).

use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use utre_core::{
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
    fn id(&self) -> utre_core::Result<&str> {
        self.id
            .as_deref()
            .ok_or_else(|| utre_core::Error::Invalid(format!("{:?} view needs an id", self.kind)))
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

/// One block on the calendar (see `utre_core::Block`), with what its tile shows.
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
    /// The raw start (epoch ms; all-day dates are UTC midnight of the calendar day), like
    /// `row.due_at`. `row.all_day` says how to read it. Defaulted so a peer that doesn't
    /// send it (an older daemon) still decodes.
    #[serde(default)]
    pub start_at: Option<i64>,
    /// The RRULE body, e.g. `FREQ=WEEKLY;BYDAY=MO`.
    pub repeat_rule: Option<String>,
    /// The next date counts from completion rather than from the schedule.
    pub repeat_after_completion: bool,
    pub checklist: Vec<ChecklistRow>,
    pub subtasks: Vec<TaskRow>,
    pub reminders: Vec<ReminderRow>,
    /// The parent's title when this is a subtask (`row.parent_id` is its id).
    pub parent_title: Option<String>,
    /// "Show in Calendar".
    pub in_calendar: bool,
}

fn rule(v: serde_json::Value) -> utre_core::Result<FilterRule> {
    Ok(serde_json::from_value(v)?)
}

pub fn sidebar(s: &Store, now: i64, tz: Tz) -> utre_core::Result<Sidebar> {
    let count = |r: serde_json::Value| -> utre_core::Result<u32> {
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
        .collect::<utre_core::Result<_>>()?;
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
        .collect::<utre_core::Result<_>>()?;
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
        .collect::<utre_core::Result<_>>()?;
    Ok(Sidebar {
        inbox: s.inbox()?.len() as u32,
        lists,
        filters,
        tags,
    })
}

/// Views with a sort menu; Completed and Trash keep their own order.
fn sortable(kind: ViewKind) -> bool {
    !matches!(kind, ViewKind::Completed | ViewKind::Trash)
}

/// How a view is sorted: the device's one sort, shared by every sortable view.
/// Empty for Completed and Trash.
pub fn view_sort(s: &Store, view: &TaskView) -> utre_core::Result<Vec<String>> {
    if sortable(view.kind) {
        s.device_sort()
    } else {
        Ok(Vec::new())
    }
}

/// Choose the sort from a view's menu. It applies to every view on this device.
pub fn set_view_sort(s: &Store, view: &TaskView, sort: &[String]) -> utre_core::Result<()> {
    if !sortable(view.kind) {
        return Err(utre_core::Error::Invalid(format!(
            "the {:?} view can't be sorted",
            view.kind
        )));
    }
    s.set_device_sort(sort)
}

/// The open tasks a sortable view shows, in `sort` order.
fn sorted_tasks(
    s: &Store,
    view: &TaskView,
    sort: &[String],
    now: i64,
    tz: Tz,
) -> utre_core::Result<Vec<Task>> {
    let run = |r: serde_json::Value| s.run_rule(&rule(r)?, sort, false, now, tz);
    match view.kind {
        ViewKind::Inbox => s.inbox_sorted(sort),
        ViewKind::List => {
            run(serde_json::json!({"field": "list", "op": "is", "value": view.id()?}))
        }
        ViewKind::Tag => run(serde_json::json!({"field": "tag", "op": "has", "value": view.id()?})),
        ViewKind::Filter => {
            let f = s
                .find_filter(view.id()?)?
                .ok_or_else(|| {
                    utre_core::Error::NotFound(format!("filter {}", view.id().unwrap_or("")))
                })?
                .filter;
            s.run_rule(&f.rule, sort, f.show_completed, now, tz)
        }
        ViewKind::Completed | ViewKind::Trash => Err(utre_core::Error::Invalid(format!(
            "the {:?} view can't be sorted",
            view.kind
        ))),
    }
}

/// Drag to reorder: put a task right before `before` (or last) in the view's manual
/// order. The order is the tasks' own (`sort_key`), shared by every view.
pub fn place_task(
    s: &Store,
    view: &TaskView,
    id: &str,
    before: Option<&str>,
    now: i64,
    tz: Tz,
) -> utre_core::Result<()> {
    let shown: Vec<String> = sorted_tasks(s, view, &["manual".to_string()], now, tz)?
        .into_iter()
        .map(|t| t.id)
        .collect();
    s.place_task(id, before, &shown)
}

pub fn tasks(s: &Store, view: &TaskView, now: i64, tz: Tz) -> utre_core::Result<Vec<TaskRow>> {
    let tasks = match view.kind {
        ViewKind::Inbox | ViewKind::List | ViewKind::Tag | ViewKind::Filter => {
            sorted_tasks(s, view, &s.device_sort()?, now, tz)?
        }
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

pub fn task_detail(s: &Store, id: &str, now: i64, tz: Tz) -> utre_core::Result<Option<TaskDetail>> {
    let Some(t) = s.get_task(id)? else {
        return Ok(None);
    };
    let (notes, list_id, repeat_rule) = (t.notes.clone(), t.list_id.clone(), t.repeat_rule.clone());
    let in_calendar = t.in_calendar;
    let parent_title = match &t.parent_id {
        Some(p) => s.get_task(p)?.map(|p| p.title),
        None => None,
    };
    let repeat_after_completion = t.repeat_mode == utre_core::RepeatMode::AfterCompletion;
    let start_at = t.start_at;
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
        .collect::<utre_core::Result<_>>()?;
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
        start_at,
        repeat_rule,
        repeat_after_completion,
        checklist,
        subtasks,
        reminders,
        parent_title,
        in_calendar,
    }))
}

/// Quick add: a plain title, in the current list or tag. In a filter the task gets
/// what the filter asks for (list, tags, priority, dates), so it stays in view.
/// Everything else lands in Inbox.
pub fn add_in(
    s: &Store,
    view: &TaskView,
    title: &str,
    now: i64,
    tz: Tz,
) -> utre_core::Result<String> {
    let mut new = NewTask {
        title: title.to_string(),
        ..Default::default()
    };
    match view.kind {
        ViewKind::List => new.list_id = view.id.clone(),
        ViewKind::Tag => new.tags = view.id.clone().into_iter().collect(),
        ViewKind::Filter => {
            let f = s.find_filter(view.id()?)?.ok_or_else(|| {
                utre_core::Error::NotFound(format!("filter {}", view.id().unwrap_or("")))
            })?;
            new = s.new_task_matching(&f.filter.rule, title, now, tz)?;
        }
        _ => {}
    }
    Ok(s.create_task(&new)?.id)
}

/// Complete an open task, or reopen a done one. Returns the new done state
/// (a repeating task moves to its next date and stays open).
pub fn toggle_done(s: &Store, id: &str) -> utre_core::Result<bool> {
    let t = s
        .get_task(id)?
        .ok_or_else(|| utre_core::Error::NotFound(format!("task {id}")))?;
    if t.status == Status::Done {
        s.reopen_task(id)?;
        return Ok(false);
    }
    s.complete_task(id)?;
    Ok(s.get_task(id)?.is_some_and(|t| t.status == Status::Done))
}

/// Set the due date from text like `fri`, `tomorrow 14:30` or `2026-12-01`, or clear it.
/// Switching between all-day and timed drops the start date rather than mixing kinds.
pub fn set_due(s: &Store, id: &str, text: Option<&str>, now: i64, tz: Tz) -> utre_core::Result<()> {
    set_date(s, id, DateField::Due, text, now, tz)
}

/// The same for the start date (it drops the due date on a kind switch).
pub fn set_start(
    s: &Store,
    id: &str,
    text: Option<&str>,
    now: i64,
    tz: Tz,
) -> utre_core::Result<()> {
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
) -> utre_core::Result<()> {
    let text = text.map(str::trim).filter(|t| !t.is_empty());
    let parsed = text.map(|t| parse_when(t, now, tz)).transpose()?;
    let t = s
        .get_task(id)?
        .ok_or_else(|| utre_core::Error::NotFound(format!("task {id}")))?;
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
pub fn calendar(s: &Store, from: i64, until: i64, tz: Tz) -> utre_core::Result<Vec<CalendarItem>> {
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

/// The calendar's Unscheduled panel: a saved filter's tasks (`filter_id` is the filter's
/// id), a list's (`list:<id>`) or a tag's (`tag:<id>`), or by default every open task
/// with no start or due. Whatever the source, a task with a start or a due date is
/// never listed: it is already on the calendar (or kept off it by "Show in Calendar"),
/// so dropping it on a day makes it leave the panel and `unschedule` brings it back.
pub fn unscheduled(
    s: &Store,
    filter_id: Option<&str>,
    now: i64,
    tz: Tz,
) -> utre_core::Result<Vec<TaskRow>> {
    let tasks = match filter_id {
        Some(id) if id.starts_with("list:") || id.starts_with("tag:") => {
            let (kind, id) = id.split_once(':').unwrap_or_default();
            let (field, op) = if kind == "list" {
                ("list", "is")
            } else {
                ("tag", "has")
            };
            s.run_rule(
                &rule(serde_json::json!({"field": field, "op": op, "value": id}))?,
                &[],
                false,
                now,
                tz,
            )?
        }
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
    tasks
        .into_iter()
        .filter(|t| t.in_calendar && t.start_at.is_none() && t.due_at.is_none())
        .map(|t| task_row(s, t, now, tz))
        .collect()
}

/// Open tasks that `id` can be moved under (the detail's Parent picker).
pub fn parent_candidates(s: &Store, id: &str, now: i64, tz: Tz) -> utre_core::Result<Vec<TaskRow>> {
    s.parent_candidates(id)?
        .into_iter()
        .map(|t| task_row(s, t, now, tz))
        .collect()
}

/// How many open tasks a rule matches right now (the filter builder's live count).
pub fn rule_count(s: &Store, rule: &FilterRule, now: i64, tz: Tz) -> utre_core::Result<u32> {
    Ok(s.run_rule(rule, &[], false, now, tz)?.len() as u32)
}

fn task_row(s: &Store, t: Task, now: i64, tz: Tz) -> utre_core::Result<TaskRow> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use utre_core::Filter;

    #[test]
    fn detail_carries_the_raw_dates() {
        let s = Store::open_in_memory().unwrap();
        let tz = Tz::Europe__Sofia;
        let now = 1_795_600_000_000; // 2026-11-25
        let id = s.add_task("Dates").unwrap().id;
        set_start(&s, &id, Some("2026-12-01 14:30"), now, tz).unwrap();
        set_due(&s, &id, Some("2026-12-02 09:00"), now, tz).unwrap();
        let d = task_detail(&s, &id, now, tz).unwrap().unwrap();
        let t = s.get_task(&id).unwrap().unwrap();
        assert_eq!(d.start_at, t.start_at);
        assert!(d.start_at.is_some());
        assert_eq!(d.row.due_at, t.due_at);
        assert!(!d.row.all_day);
        set_start(&s, &id, None, now, tz).unwrap();
        assert_eq!(
            task_detail(&s, &id, now, tz).unwrap().unwrap().start_at,
            None
        );

        // A payload from an older daemon (no `start_at`) still decodes.
        let mut v = serde_json::to_value(&d).unwrap();
        v.as_object_mut().unwrap().remove("start_at");
        let old: TaskDetail = serde_json::from_value(v).unwrap();
        assert_eq!(old.start_at, None);
    }

    #[test]
    fn placed_tasks_leave_every_unscheduled_source() {
        let s = Store::open_in_memory().unwrap();
        let tz = Tz::Europe__Sofia;
        let now = 1_795_600_000_000;
        let list = s.create_list("Work").unwrap();
        let tag = s.ensure_tag_path("deep").unwrap();
        let ids = |f: Option<&str>| -> Vec<String> {
            let mut v: Vec<_> = unscheduled(&s, f, now, tz)
                .unwrap()
                .into_iter()
                .map(|r| r.title)
                .collect();
            v.sort();
            v
        };
        let mk = |title: &str| {
            let id = s.add_task(title).unwrap().id;
            s.set_task_list(&id, Some(&list)).unwrap();
            s.tag_task(&id, &tag).unwrap();
            id
        };
        let timed = mk("timed");
        let allday = mk("allday");
        let start_only = mk("start");
        let due_only = mk("due");
        let done = mk("done");
        let hidden = mk("hidden");
        let sources = [
            None,
            Some(format!("list:{list}")),
            Some(format!("tag:{tag}")),
        ];
        for src in &sources {
            assert_eq!(ids(src.as_deref()).len(), 6);
        }
        let t0 = now + 3_600_000;
        s.place_block(&timed, t0, Some(t0 + 3_600_000), false, tz)
            .unwrap();
        s.place_block(&allday, t0, None, true, tz).unwrap();
        set_start(&s, &start_only, Some("2026-12-01 14:30"), now, tz).unwrap();
        set_due(&s, &due_only, Some("2026-12-02 09:00"), now, tz).unwrap();
        s.complete_task(&done).unwrap();
        s.set_in_calendar(&hidden, false).unwrap();
        for src in &sources {
            assert!(ids(src.as_deref()).is_empty(), "{src:?}");
        }
        s.unschedule(&timed).unwrap();
        s.unschedule(&allday).unwrap();
        for src in &sources {
            assert_eq!(ids(src.as_deref()), ["allday", "timed"], "{src:?}");
        }
        // Clearing the dates by hand brings a task back too.
        set_start(&s, &start_only, None, now, tz).unwrap();
        assert_eq!(ids(None).len(), 3);
    }

    #[test]
    fn quick_add_in_a_filter_stays_in_it() {
        let s = Store::open_in_memory().unwrap();
        let tz = Tz::Europe__Sofia;
        let now = 1_795_600_000_000; // 2026-11-25
        s.create_list("Work").unwrap();
        s.ensure_tag_path("deep").unwrap();
        let rule = serde_json::from_str(
            r#"{"all": [
                {"field": "list", "op": "is", "value": "Work"},
                {"field": "tag", "op": "has", "value": "deep"},
                {"field": "due", "op": "on_or_before", "value": "today"}
            ]}"#,
        )
        .unwrap();
        let id = s
            .save_filter(&Filter {
                name: "Focus".into(),
                icon: None,
                rule,
                sort: vec![],
                group: None,
                show_completed: false,
            })
            .unwrap();
        let view = TaskView {
            kind: ViewKind::Filter,
            id: Some(id),
        };
        let task = add_in(&s, &view, "Design review", now, tz).unwrap();
        let rows = tasks(&s, &view, now, tz).unwrap();
        assert_eq!(rows.iter().map(|r| &r.id).collect::<Vec<_>>(), [&task]);
        assert!(s.inbox().unwrap().is_empty());
    }

    #[test]
    fn one_sort_for_every_view_and_manual_moves() {
        let s = Store::open_in_memory().unwrap();
        let tz = Tz::Europe__Sofia;
        let now = 1_795_600_000_000; // 2026-11-25
        let list = TaskView {
            kind: ViewKind::List,
            id: Some(s.create_list("Home").unwrap()),
        };
        for t in ["b", "c", "a"] {
            add_in(&s, &list, t, now, tz).unwrap();
        }
        let titles = |v: &TaskView| -> Vec<String> {
            tasks(&s, v, now, tz)
                .unwrap()
                .into_iter()
                .map(|r| r.title)
                .collect()
        };
        assert_eq!(view_sort(&s, &list).unwrap(), ["due_asc", "priority_desc"]);
        set_view_sort(&s, &list, &["title_asc".into()]).unwrap();
        assert_eq!(titles(&list), ["a", "b", "c"]);

        // The choice is the same in every view.
        let inbox = TaskView {
            kind: ViewKind::Inbox,
            id: None,
        };
        assert_eq!(view_sort(&s, &inbox).unwrap(), ["title_asc"]);
        let tag = TaskView {
            kind: ViewKind::Tag,
            id: Some(s.ensure_tag_path("x").unwrap()),
        };
        assert_eq!(view_sort(&s, &tag).unwrap(), ["title_asc"]);

        set_view_sort(&s, &inbox, &["manual".into()]).unwrap();
        assert_eq!(
            titles(&list),
            ["a", "c", "b"],
            "never arranged: newest first"
        );
        let rows = tasks(&s, &list, now, tz).unwrap();
        place_task(&s, &list, &rows[2].id, Some(&rows[0].id), now, tz).unwrap();
        assert_eq!(titles(&list), ["b", "a", "c"]);

        let trash = TaskView {
            kind: ViewKind::Trash,
            id: None,
        };
        assert!(set_view_sort(&s, &trash, &["title_asc".into()]).is_err());
        assert!(view_sort(&s, &trash).unwrap().is_empty());
    }
}
