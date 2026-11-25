//! The calendar (plan/18 M5 Calendar Log): tasks as blocks on a time grid.
//!
//! A block is a task's start (else due) plus its duration. Moving a block sets the
//! start; resizing sets the duration. The due date stays put, unless the block moves past
//! it, so overdue filters don't shift when you time-block. All-day blocks are one day, at
//! the start (else due) date.
//!
//! Repeating tasks show every occurrence in the window: the current one (the task's own
//! dates) can be moved, later ones are computed from the rule and fixed, and past ones
//! come from the completion history as done. "Repeat after completion" tasks only show
//! their current occurrence, since the later dates depend on when it gets done.

use chrono::{Days, NaiveTime};
use chrono_tz::Tz;
use rusqlite::params;

use crate::dates::{
    DAY_MS, floating_day, floating_ms, local_datetime, local_midnight_ms, local_ms,
};
use crate::recur::{from_local, occurrences_after, task_zone, to_local};
use crate::task::{
    RepeatMode, Schedule, Status, TASK_COLUMNS, Task, query_tasks, require_task, write_schedule,
};
use crate::{Error, Result, Store};

/// How long a block is when its task has no duration yet (decided 2026-11-25).
pub const DEFAULT_BLOCK_MIN: i64 = 30;

/// How far outside the window to look for blocks that still reach into it: all-day dates
/// are floating, and a repeat's start can sit days before its due.
const SLACK_MS: i64 = 31 * DAY_MS;

/// One block on the calendar.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Block {
    pub task_id: String,
    /// Epoch ms. All-day blocks run from local midnight to the next one in the viewer's zone.
    pub start_at: i64,
    /// Exclusive.
    pub end_at: i64,
    pub all_day: bool,
    pub done: bool,
    /// The open task's current occurrence: moving or resizing it changes the task.
    pub movable: bool,
    pub repeating: bool,
}

/// Where a block's start lands for a series occurrence whose anchor (due, else start) is
/// `occurrence`, keeping the task's start-to-due gap in local time.
fn shifted_start(t: &Task, occurrence: i64) -> Option<i64> {
    let start = t.anchor()?;
    let series = t.due_at.or(t.start_at)?;
    let gap = to_local(t, start) - to_local(t, series);
    Some(from_local(t, to_local(t, occurrence) + gap))
}

fn block(t: &Task, start_raw: i64, tz: Tz, done: bool, movable: bool) -> Block {
    let (start_at, end_at) = if t.all_day {
        let day = floating_day(start_raw);
        let next = day.checked_add_days(Days::new(1)).unwrap_or(day);
        (local_midnight_ms(day, tz), local_midnight_ms(next, tz))
    } else {
        let min = t.duration_min.unwrap_or(DEFAULT_BLOCK_MIN);
        (start_raw, start_raw + min * 60_000)
    };
    Block {
        task_id: t.id.clone(),
        start_at,
        end_at,
        all_day: t.all_day,
        done,
        movable,
        repeating: t.repeat_rule.is_some(),
    }
}

/// The current occurrence's anchor plus every later one up to `until_raw`, for an open
/// fixed-schedule repeating task.
fn later_occurrences(t: &Task, until_raw: i64) -> Result<Vec<i64>> {
    let (Some(rule), Some(series)) = (&t.repeat_rule, t.due_at.or(t.start_at)) else {
        return Ok(Vec::new());
    };
    if t.status != Status::Open || t.repeat_mode != RepeatMode::Fixed {
        return Ok(Vec::new());
    }
    let anchor = to_local(t, series);
    let until = to_local(t, until_raw);
    Ok(occurrences_after(rule, anchor, until, task_zone(t))?
        .into_iter()
        .map(|o| from_local(t, o))
        .collect())
}

impl Store {
    /// Blocks overlapping `[from, until)` (epoch ms), in the viewer's zone `tz`, by start.
    pub fn calendar(&self, from: i64, until: i64, tz: Tz) -> Result<Vec<Block>> {
        let overlaps = |b: &Block| b.end_at > from && b.start_at < until;
        let mut out = Vec::new();

        let sql = format!(
            "SELECT {TASK_COLUMNS} FROM task
              WHERE task.status != 'deleted'
                AND coalesce(task.start_at, task.due_at) IS NOT NULL
                AND coalesce(task.start_at, task.due_at) < ?2
                AND (coalesce(task.start_at, task.due_at) >= ?1
                     OR (task.repeat_rule IS NOT NULL AND task.status = 'open'))"
        );
        for t in query_tasks(&self.conn, &sql, params![from - SLACK_MS, until + SLACK_MS])? {
            let Some(start) = t.anchor() else { continue };
            let open = t.status == Status::Open;
            let current = block(&t, start, tz, !open, open);
            if overlaps(&current) {
                out.push(current);
            }
            for occurrence in later_occurrences(&t, until + SLACK_MS)? {
                let Some(s) = shifted_start(&t, occurrence) else {
                    continue;
                };
                let b = block(&t, s, tz, false, false);
                if b.start_at >= until {
                    break;
                }
                if overlaps(&b) {
                    out.push(b);
                }
            }
        }

        // Past occurrences of repeating tasks, from the completion history.
        let sql = format!(
            "SELECT {TASK_COLUMNS}, c.occurrence_at FROM completion c
               JOIN task ON task.id = c.task_id
              WHERE task.status != 'deleted' AND c.occurrence_at BETWEEN ?1 AND ?2"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![from - SLACK_MS, until + SLACK_MS], |r| {
            Ok((Task::from_row(r)?, r.get::<_, i64>(20)?))
        })?;
        for row in rows {
            let (t, occurrence) = row?;
            let Some(s) = shifted_start(&t, occurrence) else {
                continue;
            };
            let mut b = block(&t, s, tz, true, false);
            b.repeating = true;
            if overlaps(&b) {
                out.push(b);
            }
        }

        out.sort_by(|a, b| (a.start_at, &a.task_id).cmp(&(b.start_at, &b.task_id)));
        Ok(out)
    }

    /// Put a task on the calendar at `start_at` (epoch ms; for all-day, any instant of
    /// that day in `tz`). A timed block lasts until `end_at`, else keeps the task's
    /// duration, else gets [`DEFAULT_BLOCK_MIN`]. The due date keeps its day; it only moves
    /// when the block would start after it (to the block's end, or its day).
    pub fn place_block(
        &self,
        id: &str,
        start_at: i64,
        end_at: Option<i64>,
        all_day: bool,
        tz: Tz,
    ) -> Result<()> {
        self.write(|w| {
            let t = require_task(w.conn, id)?;
            if t.status != Status::Open {
                return Err(Error::Invalid(
                    "only open tasks move on the calendar".into(),
                ));
            }
            let s = if all_day {
                let start = floating_ms(local_datetime(start_at, tz).date());
                let due = t.due_at.map(|d| {
                    let day = if t.all_day {
                        d
                    } else {
                        floating_ms(local_datetime(d, task_zone(&t)).date())
                    };
                    day.max(start)
                });
                Schedule {
                    start_at: Some(start),
                    due_at: due,
                    all_day: true,
                    tz: None,
                    duration_min: t.duration_min,
                }
            } else {
                let min = match end_at {
                    Some(end) => (end - start_at) / 60_000,
                    None => t.duration_min.unwrap_or(DEFAULT_BLOCK_MIN),
                };
                if min <= 0 {
                    return Err(Error::Invalid("a block must end after it starts".into()));
                }
                // A timed task keeps its zone; one that had none takes the viewer's.
                let zone = if t.all_day || t.tz.is_none() {
                    tz
                } else {
                    task_zone(&t)
                };
                let end = start_at + min * 60_000;
                let due = t.due_at.map(|d| {
                    let d = if t.all_day {
                        // An all-day due keeps its day: the end of that day.
                        local_ms(
                            floating_day(d),
                            NaiveTime::from_hms_opt(23, 59, 0).unwrap_or_default(),
                            zone,
                        )
                    } else {
                        d
                    };
                    if d < start_at { end } else { d }
                });
                Schedule {
                    start_at: Some(start_at),
                    due_at: due,
                    all_day: false,
                    tz: Some(zone.name().to_string()),
                    duration_min: Some(min),
                }
            };
            write_schedule(w, id, &s)
        })
    }

    /// Take a task off the calendar: clear its start and due (the duration is kept for the
    /// next drop). A repeating task needs its repeat stopped first.
    pub fn unschedule(&self, id: &str) -> Result<()> {
        self.write(|w| {
            let t = require_task(w.conn, id)?;
            if t.repeat_rule.is_some() {
                return Err(Error::Invalid(
                    "stop the repeat before taking the task off the calendar".into(),
                ));
            }
            write_schedule(
                w,
                id,
                &Schedule {
                    duration_min: t.duration_min,
                    ..Default::default()
                },
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{NewTask, Repeat};
    use chrono::NaiveDate;

    const SOFIA: Tz = chrono_tz::Europe::Sofia;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn at(day: &str, h: u32, m: u32) -> i64 {
        local_ms(d(day), NaiveTime::from_hms_opt(h, m, 0).unwrap(), SOFIA)
    }

    fn midnight(day: &str) -> i64 {
        local_midnight_ms(d(day), SOFIA)
    }

    fn timed(s: &Store, title: &str, start: Option<i64>, due: Option<i64>) -> Task {
        s.create_task(&NewTask {
            title: title.into(),
            schedule: Schedule {
                start_at: start,
                due_at: due,
                tz: Some("Europe/Sofia".into()),
                ..Default::default()
            },
            ..Default::default()
        })
        .unwrap()
    }

    fn all_day(s: &Store, title: &str, due: &str) -> Task {
        s.create_task(&NewTask {
            title: title.into(),
            schedule: Schedule {
                due_at: Some(floating_ms(d(due))),
                all_day: true,
                ..Default::default()
            },
            ..Default::default()
        })
        .unwrap()
    }

    fn week(s: &Store, monday: &str) -> Vec<Block> {
        let from = midnight(monday);
        let until = midnight(&(d(monday) + Days::new(7)).to_string());
        s.calendar(from, until, SOFIA).unwrap()
    }

    fn get(s: &Store, id: &str) -> Task {
        s.get_task(id).unwrap().unwrap()
    }

    #[test]
    fn a_task_is_its_start_plus_duration() {
        let s = Store::open_in_memory().unwrap();
        let t = timed(
            &s,
            "write",
            Some(at("2026-11-25", 10, 0)),
            Some(at("2026-11-27", 18, 0)),
        );
        timed(&s, "next week", Some(at("2026-12-03", 10, 0)), None);
        s.add_task("no date").unwrap();

        let blocks = week(&s, "2026-11-23");
        assert_eq!(blocks.len(), 1);
        let b = &blocks[0];
        assert_eq!(b.task_id, t.id);
        assert_eq!(
            (b.start_at, b.end_at),
            (at("2026-11-25", 10, 0), at("2026-11-25", 10, 30))
        );
        assert!(b.movable && !b.done && !b.all_day && !b.repeating);
    }

    #[test]
    fn due_only_and_all_day_tasks() {
        let s = Store::open_in_memory().unwrap();
        timed(&s, "call", None, Some(at("2026-11-24", 15, 0)));
        all_day(&s, "bins", "2026-11-26");
        let blocks = week(&s, "2026-11-23");
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].start_at, at("2026-11-24", 15, 0));
        assert!(blocks[1].all_day);
        assert_eq!(
            (blocks[1].start_at, blocks[1].end_at),
            (midnight("2026-11-26"), midnight("2026-11-27"))
        );
    }

    #[test]
    fn done_tasks_show_as_done_and_fixed() {
        let s = Store::open_in_memory().unwrap();
        let t = timed(&s, "done", Some(at("2026-11-24", 9, 0)), None);
        s.complete_task(&t.id).unwrap();
        let b = &week(&s, "2026-11-23")[0];
        assert!(b.done && !b.movable);
    }

    #[test]
    fn weekly_repeat_shows_every_occurrence_and_only_the_first_moves() {
        let s = Store::open_in_memory().unwrap();
        let t = timed(&s, "gym", Some(at("2026-11-02", 18, 0)), None);
        s.set_repeat(
            &t.id,
            Some(&Repeat {
                rule: "FREQ=WEEKLY".into(),
                mode: RepeatMode::Fixed,
            }),
        )
        .unwrap();
        let blocks = s
            .calendar(midnight("2026-11-01"), midnight("2026-12-01"), SOFIA)
            .unwrap();
        let starts: Vec<_> = blocks.iter().map(|b| b.start_at).collect();
        assert_eq!(
            starts,
            [
                "2026-11-02",
                "2026-11-09",
                "2026-11-16",
                "2026-11-23",
                "2026-11-30"
            ]
            .map(|day| at(day, 18, 0))
        );
        assert!(blocks[0].movable);
        assert!(
            blocks[1..]
                .iter()
                .all(|b| !b.movable && !b.done && b.repeating)
        );
    }

    #[test]
    fn repeat_keeps_local_time_across_dst() {
        let s = Store::open_in_memory().unwrap();
        // Sofia moves to summer time on 2026-03-29.
        let t = timed(&s, "standup", Some(at("2026-03-27", 9, 0)), None);
        s.set_repeat(
            &t.id,
            Some(&Repeat {
                rule: "FREQ=DAILY".into(),
                mode: RepeatMode::Fixed,
            }),
        )
        .unwrap();
        let blocks = s
            .calendar(midnight("2026-03-27"), midnight("2026-03-31"), SOFIA)
            .unwrap();
        let starts: Vec<_> = blocks.iter().map(|b| b.start_at).collect();
        assert_eq!(
            starts,
            ["2026-03-27", "2026-03-28", "2026-03-29", "2026-03-30"].map(|day| at(day, 9, 0))
        );
    }

    #[test]
    fn count_and_after_completion_limit_the_occurrences() {
        let s = Store::open_in_memory().unwrap();
        let a = all_day(&s, "twice", "2026-11-23");
        s.set_repeat(
            &a.id,
            Some(&Repeat {
                rule: "FREQ=DAILY;COUNT=2".into(),
                mode: RepeatMode::Fixed,
            }),
        )
        .unwrap();
        let b = all_day(&s, "after", "2026-11-23");
        s.set_repeat(
            &b.id,
            Some(&Repeat {
                rule: "FREQ=DAILY".into(),
                mode: RepeatMode::AfterCompletion,
            }),
        )
        .unwrap();
        let blocks = week(&s, "2026-11-23");
        assert_eq!(blocks.iter().filter(|x| x.task_id == a.id).count(), 2);
        assert_eq!(blocks.iter().filter(|x| x.task_id == b.id).count(), 1);
    }

    #[test]
    fn completed_occurrences_come_from_history() {
        let s = Store::open_in_memory().unwrap();
        let t = all_day(&s, "water plants", "2026-11-23");
        s.set_repeat(
            &t.id,
            Some(&Repeat {
                rule: "FREQ=DAILY;INTERVAL=2".into(),
                mode: RepeatMode::Fixed,
            }),
        )
        .unwrap();
        s.complete_task(&t.id).unwrap(); // now due on the 25th
        let blocks = week(&s, "2026-11-23");
        let days: Vec<_> = blocks
            .iter()
            .map(|b| (b.start_at, b.done, b.movable))
            .collect();
        assert_eq!(
            days,
            vec![
                (midnight("2026-11-23"), true, false),
                (midnight("2026-11-25"), false, true),
                (midnight("2026-11-27"), false, false),
                (midnight("2026-11-29"), false, false),
            ]
        );
    }

    #[test]
    fn placing_an_unscheduled_task_gives_it_the_default_length() {
        let s = Store::open_in_memory().unwrap();
        let t = s.add_task("plan trip").unwrap();
        s.place_block(&t.id, at("2026-11-25", 14, 0), None, false, SOFIA)
            .unwrap();
        let t = get(&s, &t.id);
        assert_eq!(t.start_at, Some(at("2026-11-25", 14, 0)));
        assert_eq!(t.due_at, None);
        assert_eq!(t.duration_min, Some(DEFAULT_BLOCK_MIN));
        assert_eq!(t.tz.as_deref(), Some("Europe/Sofia"));
    }

    #[test]
    fn resizing_sets_duration_and_the_due_only_moves_when_passed() {
        let s = Store::open_in_memory().unwrap();
        let due = at("2026-11-25", 17, 0);
        let t = timed(&s, "report", None, Some(due));
        s.place_block(
            &t.id,
            at("2026-11-25", 9, 0),
            Some(at("2026-11-25", 11, 0)),
            false,
            SOFIA,
        )
        .unwrap();
        let got = get(&s, &t.id);
        assert_eq!(
            (got.start_at, got.due_at, got.duration_min),
            (Some(at("2026-11-25", 9, 0)), Some(due), Some(120))
        );

        s.place_block(&t.id, at("2026-11-26", 10, 0), None, false, SOFIA)
            .unwrap();
        let got = get(&s, &t.id);
        assert_eq!(got.start_at, Some(at("2026-11-26", 10, 0)));
        assert_eq!(
            got.due_at,
            Some(at("2026-11-26", 12, 0)),
            "due moves to the block's end"
        );
    }

    #[test]
    fn switching_between_all_day_and_timed_keeps_the_due_day() {
        let s = Store::open_in_memory().unwrap();
        let t = all_day(&s, "taxes", "2026-11-27");
        s.place_block(&t.id, at("2026-11-25", 10, 0), None, false, SOFIA)
            .unwrap();
        let got = get(&s, &t.id);
        assert!(!got.all_day);
        assert_eq!(got.due_at, Some(at("2026-11-27", 23, 59)));

        s.place_block(&t.id, at("2026-11-28", 10, 0), None, true, SOFIA)
            .unwrap();
        let got = get(&s, &t.id);
        assert!(got.all_day);
        assert_eq!(got.start_at, Some(floating_ms(d("2026-11-28"))));
        assert_eq!(
            got.due_at,
            Some(floating_ms(d("2026-11-28"))),
            "a due before the start moves up"
        );
        assert_eq!(got.tz, None);
    }

    #[test]
    fn bad_placements_are_refused() {
        let s = Store::open_in_memory().unwrap();
        let t = s.add_task("x").unwrap();
        let start = at("2026-11-25", 10, 0);
        assert!(
            s.place_block(&t.id, start, Some(start), false, SOFIA)
                .is_err()
        );
        s.complete_task(&t.id).unwrap();
        assert!(s.place_block(&t.id, start, None, false, SOFIA).is_err());
    }

    #[test]
    fn unscheduling_clears_dates_but_not_repeats() {
        let s = Store::open_in_memory().unwrap();
        let t = timed(
            &s,
            "read",
            Some(at("2026-11-25", 20, 0)),
            Some(at("2026-11-26", 20, 0)),
        );
        s.place_block(
            &t.id,
            at("2026-11-25", 20, 0),
            Some(at("2026-11-25", 21, 0)),
            false,
            SOFIA,
        )
        .unwrap();
        s.unschedule(&t.id).unwrap();
        let got = get(&s, &t.id);
        assert_eq!(
            (got.start_at, got.due_at, got.duration_min),
            (None, None, Some(60))
        );
        assert!(s.inbox().unwrap().iter().any(|i| i.id == t.id));

        let r = all_day(&s, "weekly", "2026-11-25");
        s.set_repeat(
            &r.id,
            Some(&Repeat {
                rule: "FREQ=WEEKLY".into(),
                mode: RepeatMode::Fixed,
            }),
        )
        .unwrap();
        assert!(s.unschedule(&r.id).is_err());
    }
}
