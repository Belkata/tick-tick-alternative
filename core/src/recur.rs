//! Repeating tasks (plan/04 Data Model, "Repeat rule").
//!
//! Completing a repeating task keeps its id: a completion row is written, its
//! dates move to the next occurrence, and its checklist resets. `fixed` counts
//! the next date from the schedule; `after_completion` from the completion day.
//! The rule is an RRULE body (`FREQ=WEEKLY;BYDAY=MO,TH`), evaluated with the
//! `rrule` crate in the task's zone (UTC for floating all-day dates).

use chrono::{NaiveDateTime, TimeZone};
use rusqlite::params;
use uuid::Uuid;

use crate::dates::{floating_day, floating_ms, local_datetime, local_ms};
use crate::oplog::Entity;
use crate::store::Writer;
use crate::task::{RepeatMode, Status, Task, require_task, write_status};
use crate::{Error, Result, Store};

/// A repeat rule and how the next date is counted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repeat {
    /// RRULE body without DTSTART, e.g. `FREQ=DAILY;INTERVAL=2`.
    pub rule: String,
    pub mode: RepeatMode,
}

/// One entry of the Completed view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Completion {
    pub task_id: String,
    pub title: String,
    pub completed_at: i64,
    /// The date that was completed (due, else start), if it had one.
    pub occurrence_at: Option<i64>,
    /// True for one occurrence of a task that keeps repeating.
    pub repeating: bool,
}

/// The zone a task's dates are evaluated in.
fn task_zone(t: &Task) -> chrono_tz::Tz {
    if t.all_day {
        chrono_tz::Tz::UTC
    } else {
        t.tz.as_deref()
            .and_then(|z| z.parse().ok())
            .unwrap_or(chrono_tz::Tz::UTC)
    }
}

fn to_local(t: &Task, ms: i64) -> NaiveDateTime {
    if t.all_day {
        floating_day(ms).and_time(chrono::NaiveTime::MIN)
    } else {
        local_datetime(ms, task_zone(t))
    }
}

fn from_local(t: &Task, dt: NaiveDateTime) -> i64 {
    if t.all_day {
        floating_ms(dt.date())
    } else {
        local_ms(dt.date(), dt.time(), task_zone(t))
    }
}

/// The first occurrence of `rule` strictly after `anchor`, or `None` when the
/// rule has run out (UNTIL passed). COUNT is handled by the caller.
fn next_after(
    rule: &str,
    anchor: NaiveDateTime,
    tz: chrono_tz::Tz,
) -> Result<Option<NaiveDateTime>> {
    let bad = |e: rrule::RRuleError| Error::Invalid(format!("repeat rule '{rule}': {e}"));
    let parsed: rrule::RRule<rrule::Unvalidated> = rule.parse().map_err(bad)?;
    let rtz = rrule::Tz::Tz(tz);
    let start = rtz
        .from_local_datetime(&anchor)
        .earliest()
        .ok_or_else(|| Error::Invalid("start time doesn't exist in its zone".into()))?;
    let set = parsed.build(start).map_err(bad)?;
    Ok(set
        .into_iter()
        .take(10_000)
        .find(|d| *d > start)
        .map(|d| d.with_timezone(&tz).naive_local()))
}

/// The COUNT left in a rule, if it has one.
fn count(rule: &str) -> Option<u32> {
    rule.split(';')
        .find_map(|p| p.trim().strip_prefix("COUNT="))
        .and_then(|n| n.parse().ok())
}

fn with_count(rule: &str, n: u32) -> String {
    rule.split(';')
        .map(|p| {
            if p.trim().starts_with("COUNT=") {
                format!("COUNT={n}")
            } else {
                p.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(";")
}

fn normalize(rule: &str) -> String {
    let upper = rule.trim().to_ascii_uppercase();
    upper.strip_prefix("RRULE:").unwrap_or(&upper).to_string()
}

/// Complete `id`. Returns false if it isn't open.
pub(crate) fn complete(w: &mut Writer, id: &str) -> Result<bool> {
    let t = require_task(w.conn, id)?;
    if t.status != Status::Open {
        return Ok(false);
    }
    let (Some(rule), Some(anchor_ms)) = (t.repeat_rule.clone(), t.due_at.or(t.start_at)) else {
        write_status(w, id, Status::Done)?;
        return Ok(true);
    };

    let now = w.now();
    let remaining = count(&rule);
    let anchor = to_local(&t, anchor_ms);
    let next = if remaining.is_some_and(|n| n <= 1) {
        None // this was the last of COUNT occurrences
    } else {
        let from = match t.repeat_mode {
            RepeatMode::Fixed => anchor,
            // Same time of day, counted from the day it was done.
            RepeatMode::AfterCompletion => to_local(&t, now).date().and_time(anchor.time()),
        };
        let from = if t.all_day {
            from.date().and_time(chrono::NaiveTime::MIN)
        } else {
            from
        };
        next_after(&rule, from, task_zone(&t))?
    };

    let Some(next) = next else {
        // The series is over: the task is simply done.
        write_status(w, id, Status::Done)?;
        return Ok(true);
    };

    let completion = Uuid::now_v7().to_string();
    w.set(Entity::Completion, &completion, "task_id", id)?;
    w.set(Entity::Completion, &completion, "completed_at", now)?;
    w.set(Entity::Completion, &completion, "occurrence_at", anchor_ms)?;

    // Move both dates by the same local shift, so the duration and time of day survive DST.
    let shift = next - anchor;
    let due_at = t.due_at.map(|d| from_local(&t, to_local(&t, d) + shift));
    let start_at = t.start_at.map(|s| from_local(&t, to_local(&t, s) + shift));
    w.set(Entity::Task, id, "due_at", due_at)?;
    w.set(Entity::Task, id, "start_at", start_at)?;
    if let Some(n) = remaining {
        w.set(Entity::Task, id, "repeat_rule", with_count(&rule, n - 1))?;
    }

    let mut stmt = w
        .conn
        .prepare("SELECT id FROM checklist_item WHERE task_id = ?1 AND done = 1 AND deleted = 0")?;
    let done: Vec<String> = stmt
        .query_map([id], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    drop(stmt);
    for item in done {
        w.set(Entity::Checklist, &item, "done", false)?;
    }
    Ok(true)
}

impl Store {
    /// Make a task repeat, or stop it repeating with `None`. It needs a date first.
    pub fn set_repeat(&self, id: &str, repeat: Option<&Repeat>) -> Result<()> {
        self.write(|w| {
            let t = require_task(w.conn, id)?;
            match repeat {
                None => {
                    w.set(Entity::Task, id, "repeat_rule", None::<String>)?;
                    w.set(Entity::Task, id, "repeat_mode", RepeatMode::Fixed.as_str())
                }
                Some(r) => {
                    let anchor = t.due_at.or(t.start_at).ok_or_else(|| {
                        Error::Invalid("give the task a date before making it repeat".into())
                    })?;
                    let rule = normalize(&r.rule);
                    if rule.contains("DTSTART") {
                        return Err(Error::Invalid(
                            "leave DTSTART out; the task's date is the start".into(),
                        ));
                    }
                    next_after(&rule, to_local(&t, anchor), task_zone(&t))?; // validates
                    w.set(Entity::Task, id, "repeat_rule", rule)?;
                    w.set(Entity::Task, id, "repeat_mode", r.mode.as_str())
                }
            }
        })
    }

    /// The Completed view: done tasks and past occurrences of repeating ones, newest first.
    pub fn completed(&self, limit: usize) -> Result<Vec<Completion>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, title, completed_at, coalesce(due_at, start_at), 0 FROM task
              WHERE status = 'done' AND completed_at IS NOT NULL
             UNION ALL
             SELECT c.task_id, t.title, c.completed_at, c.occurrence_at, 1
               FROM completion c JOIN task t ON t.id = c.task_id
              WHERE t.status != 'deleted'
             ORDER BY 3 DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], |r| {
            Ok(Completion {
                task_id: r.get(0)?,
                title: r.get(1)?,
                completed_at: r.get(2)?,
                occurrence_at: r.get(3)?,
                repeating: r.get(4)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dates::{floating_day, local_datetime};
    use crate::{NewTask, Schedule};
    use chrono::NaiveDate;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn all_day_task(s: &Store, day: &str) -> Task {
        s.create_task(&NewTask {
            title: "r".into(),
            schedule: Schedule {
                due_at: Some(floating_ms(d(day))),
                all_day: true,
                ..Default::default()
            },
            ..Default::default()
        })
        .unwrap()
    }

    fn due_day(s: &Store, id: &str) -> NaiveDate {
        floating_day(s.get_task(id).unwrap().unwrap().due_at.unwrap())
    }

    fn fixed(rule: &str) -> Repeat {
        Repeat {
            rule: rule.into(),
            mode: RepeatMode::Fixed,
        }
    }

    #[test]
    fn weekly_fixed_advances_and_logs_completion() {
        let s = Store::open_in_memory().unwrap();
        let t = all_day_task(&s, "2026-11-23"); // a Monday
        s.set_repeat(&t.id, Some(&fixed("FREQ=WEEKLY;BYDAY=MO,TH")))
            .unwrap();
        s.complete_task(&t.id).unwrap();
        assert_eq!(due_day(&s, &t.id), d("2026-11-26"), "Thursday");
        s.complete_task(&t.id).unwrap();
        assert_eq!(due_day(&s, &t.id), d("2026-11-30"), "next Monday");
        let task = s.get_task(&t.id).unwrap().unwrap();
        assert_eq!(task.status, Status::Open, "keeps repeating");
        let done = s.completed(10).unwrap();
        assert_eq!(done.len(), 2);
        assert!(done.iter().all(|c| c.repeating));
    }

    #[test]
    fn after_completion_counts_from_today() {
        let s = Store::open_in_memory().unwrap();
        let t = all_day_task(&s, "2020-01-01"); // long overdue
        s.set_repeat(
            &t.id,
            Some(&Repeat {
                rule: "FREQ=DAILY;INTERVAL=3".into(),
                mode: RepeatMode::AfterCompletion,
            }),
        )
        .unwrap();
        s.complete_task(&t.id).unwrap();
        let expected = crate::dates::utc_today() + chrono::Days::new(3);
        assert_eq!(due_day(&s, &t.id), expected);
    }

    #[test]
    fn count_runs_out_then_task_is_done() {
        let s = Store::open_in_memory().unwrap();
        let t = all_day_task(&s, "2026-11-25");
        s.set_repeat(&t.id, Some(&fixed("FREQ=DAILY;COUNT=2")))
            .unwrap();
        s.complete_task(&t.id).unwrap();
        let task = s.get_task(&t.id).unwrap().unwrap();
        assert_eq!(
            (task.status, task.repeat_rule.as_deref()),
            (Status::Open, Some("FREQ=DAILY;COUNT=1"))
        );
        s.complete_task(&t.id).unwrap();
        assert_eq!(s.get_task(&t.id).unwrap().unwrap().status, Status::Done);
    }

    #[test]
    fn until_in_the_past_finishes_the_series() {
        let s = Store::open_in_memory().unwrap();
        let t = all_day_task(&s, "2026-11-25");
        s.set_repeat(&t.id, Some(&fixed("FREQ=DAILY;UNTIL=20261125T000000Z")))
            .unwrap();
        s.complete_task(&t.id).unwrap();
        assert_eq!(s.get_task(&t.id).unwrap().unwrap().status, Status::Done);
    }

    #[test]
    fn timed_task_keeps_local_time_across_dst() {
        let s = Store::open_in_memory().unwrap();
        let tz: chrono_tz::Tz = "Europe/Sofia".parse().unwrap();
        let start = local_ms(
            d("2026-10-24"),
            chrono::NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
            tz,
        );
        let t = s
            .create_task(&NewTask {
                title: "standup".into(),
                schedule: Schedule {
                    start_at: Some(start),
                    due_at: Some(start + 30 * 60_000),
                    tz: Some("Europe/Sofia".into()),
                    ..Default::default()
                },
                ..Default::default()
            })
            .unwrap();
        s.set_repeat(&t.id, Some(&fixed("FREQ=DAILY"))).unwrap();
        s.complete_task(&t.id).unwrap(); // crosses the 2026-10-25 DST change
        s.complete_task(&t.id).unwrap();
        let t = s.get_task(&t.id).unwrap().unwrap();
        assert_eq!(
            local_datetime(t.start_at.unwrap(), tz).to_string(),
            "2026-10-26 09:00:00"
        );
        assert_eq!(
            local_datetime(t.due_at.unwrap(), tz).to_string(),
            "2026-10-26 09:30:00"
        );
    }

    #[test]
    fn checklist_resets_on_repeat() {
        let s = Store::open_in_memory().unwrap();
        let t = all_day_task(&s, "2026-11-25");
        let item = s.add_checklist_item(&t.id, "pack bag").unwrap();
        s.set_checklist_done(&item, true).unwrap();
        s.set_repeat(&t.id, Some(&fixed("FREQ=DAILY"))).unwrap();
        s.complete_task(&t.id).unwrap();
        assert!(!s.checklist(&t.id).unwrap()[0].done);
    }

    #[test]
    fn bad_rules_and_undated_tasks_are_rejected() {
        let s = Store::open_in_memory().unwrap();
        let undated = s.add_task("u").unwrap();
        assert!(
            s.set_repeat(&undated.id, Some(&fixed("FREQ=DAILY")))
                .is_err()
        );
        let t = all_day_task(&s, "2026-11-25");
        assert!(s.set_repeat(&t.id, Some(&fixed("FREQ=SOMETIMES"))).is_err());
        s.set_repeat(&t.id, Some(&fixed("rrule:freq=daily")))
            .unwrap();
        assert_eq!(
            s.get_task(&t.id).unwrap().unwrap().repeat_rule.as_deref(),
            Some("FREQ=DAILY")
        );
        s.set_repeat(&t.id, None).unwrap();
        assert!(s.get_task(&t.id).unwrap().unwrap().repeat_rule.is_none());
    }
}
