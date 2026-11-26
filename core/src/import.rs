//! One-time import of a TickTick export (plan/12).
//!
//! The input is the JSON backup written by the export step: `projects`, `tags`,
//! `undone` and `completed` task lists in TickTick's own field names. Every
//! write goes through the normal op path, so the result syncs like any edit.
//! Running it twice does not duplicate: a task whose title and creation time
//! already exist is skipped.

use std::collections::HashMap;

use chrono::{DateTime, NaiveTime};
use chrono_tz::Tz;
use serde_json::Value;

use crate::oplog::Entity;
use crate::{Error, NewTask, ReminderKind, Repeat, RepeatMode, Result, Schedule, Store};

const FALLBACK_TZ: &str = "Europe/Sofia";

/// What an import did, and what it could not map.
#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct ImportReport {
    pub lists: usize,
    pub tags: usize,
    pub open_tasks: usize,
    pub done_tasks: usize,
    pub subtasks: usize,
    pub checklist_items: usize,
    pub repeats: usize,
    pub reminders: usize,
    pub already_there: usize,
    pub dropped_columns: usize,
    /// Anything that was changed or left out, one line each.
    pub notes: Vec<String>,
}

fn s<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str).filter(|x| !x.is_empty())
}

fn instant(text: &str) -> Option<DateTime<chrono::FixedOffset>> {
    DateTime::parse_from_str(text, "%Y-%m-%dT%H:%M:%S%z").ok()
}

fn ms(text: Option<&str>) -> Option<i64> {
    text.and_then(instant).map(|d| d.timestamp_millis())
}

/// "✈️Work" → ("✈️", "Work"): leading symbols become the icon.
fn split_icon(name: &str) -> (Option<String>, String) {
    let at = name
        .char_indices()
        .find(|(_, c)| c.is_alphanumeric())
        .map_or(0, |(i, _)| i);
    let (icon, rest) = name.split_at(at);
    let icon = icon.trim();
    (
        (!icon.is_empty()).then(|| icon.to_string()),
        rest.trim().to_string(),
    )
}

fn priority(p: i64) -> u8 {
    match p {
        1 => 1,
        3 => 2,
        5 => 3,
        _ => 0,
    }
}

/// Minutes after the task's time (negative = before) from an iCal TRIGGER.
fn trigger_minutes_after(t: &str) -> Option<i64> {
    let body = t.strip_prefix("TRIGGER:")?;
    let (neg, body) = match body.strip_prefix('-') {
        Some(b) => (true, b),
        None => (false, body),
    };
    let body = body.strip_prefix('P')?;
    let (mut secs, mut num) = (0i64, String::new());
    for c in body.chars() {
        match c {
            '0'..='9' => num.push(c),
            'T' => {}
            'W' | 'D' | 'H' | 'M' | 'S' => {
                let n: i64 = num.parse().ok()?;
                num.clear();
                secs += n * match c {
                    'W' => 604_800,
                    'D' => 86_400,
                    'H' => 3_600,
                    'M' => 60,
                    _ => 1,
                };
            }
            _ => return None,
        }
    }
    let m = secs / 60;
    Some(if neg { -m } else { m })
}

fn schedule(t: &Value, report: &mut ImportReport, title: &str) -> Schedule {
    let tz_name = s(t, "timeZone").unwrap_or(FALLBACK_TZ);
    let tz: Tz = tz_name.parse().unwrap_or_else(|_| {
        report.notes.push(format!(
            "'{title}': unknown zone '{tz_name}', used {FALLBACK_TZ}"
        ));
        FALLBACK_TZ.parse().expect("fallback zone")
    });
    let all_day = t.get("isAllDay").and_then(Value::as_bool).unwrap_or(false);
    let conv = |text: Option<&str>| -> Option<i64> {
        let dt = text.and_then(instant)?;
        Some(if all_day {
            let day = dt.with_timezone(&tz).date_naive();
            day.and_time(NaiveTime::MIN).and_utc().timestamp_millis()
        } else {
            dt.timestamp_millis()
        })
    };
    let (mut start, due) = (conv(s(t, "startDate")), conv(s(t, "dueDate")));
    if start == due || matches!((start, due), (Some(a), Some(b)) if a > b) {
        start = None; // TickTick repeats the due date as the start; keep due only
    }
    if start.is_none() && due.is_none() {
        return Schedule::default();
    }
    Schedule {
        start_at: start,
        due_at: due,
        all_day,
        tz: (!all_day).then(|| tz.name().to_string()),
        duration_min: None,
    }
}

impl Store {
    /// Import a TickTick export. Safe to repeat.
    pub fn import_ticktick(&self, export: &Value) -> Result<ImportReport> {
        let mut r = ImportReport::default();

        // Tags first, parents before children, by display path.
        let tag_defs: Vec<&Value> = export["tags"]
            .as_array()
            .map(|a| a.iter().collect())
            .unwrap_or_default();
        let label_of: HashMap<&str, &str> = tag_defs
            .iter()
            .filter_map(|t| Some((s(t, "name")?, s(t, "label").or(s(t, "name"))?)))
            .collect();
        let parent_of: HashMap<&str, &str> = tag_defs
            .iter()
            .filter_map(|t| Some((s(t, "name")?, s(t, "parent")?)))
            .collect();
        let path_of = |name: &str| -> String {
            let mut parts = vec![label_of.get(name).copied().unwrap_or(name)];
            let mut cur = name;
            while let Some(p) = parent_of.get(cur).copied() {
                parts.push(label_of.get(p).copied().unwrap_or(p));
                cur = p;
                if parts.len() > 8 {
                    break;
                }
            }
            parts.reverse();
            parts.join("/")
        };
        let mut tag_ids: HashMap<String, String> = HashMap::new();
        for t in &tag_defs {
            let Some(name) = s(t, "name") else { continue };
            let id = self.ensure_tag_path(&path_of(name))?;
            if let Some(color) = s(t, "color") {
                self.set_tag_style(&id, Some(color), None)?;
            }
            tag_ids.insert(name.to_string(), id);
            r.tags += 1;
        }

        // Lists. The Inbox is not a list here.
        let mut list_ids: HashMap<String, String> = HashMap::new();
        for p in export["projects"].as_array().into_iter().flatten() {
            let (Some(pid), Some(name)) = (s(p, "id"), s(p, "name")) else {
                continue;
            };
            if pid == "inbox" {
                continue;
            }
            let (icon, name) = split_icon(name);
            let id = match self.find_list(&name)? {
                Some(l) if l.name.eq_ignore_ascii_case(&name) => l.id,
                _ => {
                    r.lists += 1;
                    self.create_list(&name)?
                }
            };
            self.set_list_style(&id, s(p, "color"), icon.as_deref())?;
            list_ids.insert(pid.to_string(), id);
        }
        if export["columns"].as_array().is_some_and(|c| !c.is_empty()) {
            r.dropped_columns = export["columns"].as_array().map_or(0, Vec::len);
            r.notes
                .push("kanban columns dropped (plan/11 question 7)".into());
        }

        // Tasks: open first, then completed.
        let tasks: Vec<(&Value, bool)> = export["undone"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|t| (t, false))
            .chain(
                export["completed"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|t| (t, true)),
            )
            .collect();
        let mut ids: HashMap<String, String> = HashMap::new(); // TickTick id → ours
        let mut seen = std::collections::HashSet::new();
        // Tasks from an earlier import. Occurrences of one repeating task share a title
        // and creation time, so only rows that existed before this run count.
        let before: HashMap<(String, i64), String> = {
            let mut stmt = self
                .conn
                .prepare("SELECT title, created_at, id FROM task")?;
            stmt.query_map([], |row| Ok(((row.get(0)?, row.get(1)?), row.get(2)?)))?
                .collect::<rusqlite::Result<_>>()?
        };
        let mut parents: Vec<(String, String, String)> = Vec::new(); // ours, parent tt id, title

        for (t, in_done_list) in &tasks {
            let Some(tt_id) = s(t, "id") else { continue };
            if !seen.insert(tt_id.to_string()) {
                continue;
            }
            let title = s(t, "title").map(str::trim).filter(|x| !x.is_empty());
            let title_text = title.unwrap_or("(untitled)").to_string();
            let created = ms(s(t, "createdTime"));
            if let Some(c) = created
                && let Some(id) = before.get(&(title_text.clone(), c))
            {
                ids.insert(tt_id.to_string(), id.clone());
                r.already_there += 1;
                continue;
            }
            let sched = schedule(t, &mut r, &title_text);
            let has_date = sched.start_at.or(sched.due_at).is_some();
            let mut tags = Vec::new();
            for name in t["tags"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
            {
                match tag_ids.get(name) {
                    Some(id) => tags.push(id.clone()),
                    None => {
                        let id = self.ensure_tag_path(name)?;
                        tag_ids.insert(name.to_string(), id.clone());
                        r.notes.push(format!(
                            "tag '{name}' wasn't in the tag list; created plain"
                        ));
                        tags.push(id);
                    }
                }
            }
            let new = NewTask {
                title: title_text.clone(),
                notes: s(t, "content").unwrap_or_default().to_string(),
                list_id: s(t, "projectId").and_then(|p| list_ids.get(p)).cloned(),
                parent_id: None,
                priority: priority(t["priority"].as_i64().unwrap_or(0)),
                schedule: sched,
                tags,
            };
            let task = self.create_task_with(&new, false)?;
            let id = task.id;
            if let Some(c) = created {
                self.write(|w| w.set(Entity::Task, &id, "created_at", c))?;
            }

            if let Some(flag) = s(t, "repeatFlag") {
                if has_date {
                    let mut rule = flag.trim_start_matches("RRULE:").to_string();
                    // UNTIL=20400701 (date) → date-time, which RRULE parsing needs.
                    if let Some(i) = rule.find("UNTIL=") {
                        let v = &rule[i + 6..];
                        let end = v.find(';').unwrap_or(v.len());
                        if end == 8 {
                            rule.insert_str(i + 6 + 8, "T000000Z");
                        }
                    }
                    let mode = if s(t, "repeatFrom") == Some("1") {
                        RepeatMode::AfterCompletion
                    } else {
                        RepeatMode::Fixed
                    };
                    match self.set_repeat(&id, Some(&Repeat { rule, mode })) {
                        Ok(()) => r.repeats += 1,
                        Err(e) => r.notes.push(format!(
                            "'{title_text}': repeat '{flag}' not imported ({e})"
                        )),
                    }
                } else {
                    r.notes.push(format!(
                        "'{title_text}': repeat without a date, not imported"
                    ));
                }
            }

            let all_day = t.get("isAllDay").and_then(Value::as_bool).unwrap_or(false);
            for trig in t["reminders"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
            {
                if !has_date {
                    r.notes.push(format!(
                        "'{title_text}': reminder without a date, not imported"
                    ));
                    continue;
                }
                let Some(after) = trigger_minutes_after(trig) else {
                    r.notes
                        .push(format!("'{title_text}': reminder '{trig}' not understood"));
                    continue;
                };
                // All-day tasks fire at 09:00 on the day, so TickTick's +9h is our offset 0.
                let offset_min = if all_day { 9 * 60 - after } else { -after };
                self.add_reminder(&id, ReminderKind::Relative { offset_min })?;
                r.reminders += 1;
            }

            let mut items: Vec<&Value> = t["items"].as_array().into_iter().flatten().collect();
            items.sort_by_key(|i| i["sortOrder"].as_i64().unwrap_or(0));
            for item in items {
                let Some(it) = s(item, "title") else { continue };
                let cid = self.add_checklist_item(&id, it)?;
                if item["status"].as_i64().unwrap_or(0) != 0 {
                    self.set_checklist_done(&cid, true)?;
                }
                r.checklist_items += 1;
            }

            if let Some(p) = s(t, "parentId") {
                parents.push((id.clone(), p.to_string(), title_text.clone()));
            }

            let status = t["status"].as_i64().unwrap_or(0);
            if status != 0 || *in_done_list {
                let at = ms(s(t, "completedTime")).or(created).unwrap_or(0);
                self.write(|w| {
                    w.set(Entity::Task, &id, "status", "done")?;
                    w.set(Entity::Task, &id, "completed_at", at)
                })?;
                r.done_tasks += 1;
            } else {
                r.open_tasks += 1;
            }
            ids.insert(tt_id.to_string(), id);
        }

        for (id, parent_tt, title) in parents {
            match ids.get(&parent_tt) {
                Some(pid) => match self.set_parent(&id, Some(pid)) {
                    Ok(()) => r.subtasks += 1,
                    Err(e) => r.notes.push(format!("'{title}': kept top-level ({e})")),
                },
                None => r.notes.push(format!(
                    "'{title}': its parent isn't in the export, kept top-level"
                )),
            }
        }
        Ok(r)
    }
}

/// Parse an export file's text.
pub fn parse_export(text: &str) -> Result<Value> {
    let v: Value = serde_json::from_str(text)
        .map_err(|e| Error::Invalid(format!("not a TickTick export: {e}")))?;
    if v.get("undone").is_none() && v.get("completed").is_none() {
        return Err(Error::Invalid(
            "no 'undone' or 'completed' tasks in this file".into(),
        ));
    }
    Ok(v)
}
