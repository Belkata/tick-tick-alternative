//! Reminders and the upcoming reminder schedule (plan/04, "Reminder").
//!
//! The core only computes *when* reminders fire; each device's shell registers
//! its own OS notifications from `upcoming_reminders`.

use chrono::NaiveTime;
use chrono_tz::Tz;
use rusqlite::{Connection, OptionalExtension};
use uuid::Uuid;

use crate::dates::{floating_day, local_ms};
use crate::oplog::Entity;
use crate::task::{TASK_COLUMNS, Task, query_tasks, require_task};
use crate::{Error, Result, Store};

/// Relative reminders on all-day tasks count back from this local time on the date.
pub const ALL_DAY_REMINDER_TIME: NaiveTime = match NaiveTime::from_hms_opt(9, 0, 0) {
    Some(t) => t,
    None => unreachable!(),
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ReminderKind {
    /// Minutes before the task's start (else due). 0 = at that time.
    Relative { offset_min: i64 },
    /// A fixed instant, epoch ms.
    Absolute { at: i64 },
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Reminder {
    pub id: String,
    pub task_id: String,
    pub kind: ReminderKind,
}

/// One notification to schedule.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ReminderFire {
    pub reminder_id: String,
    pub task_id: String,
    pub title: String,
    pub fire_at: i64,
}

fn require_reminder(conn: &Connection, id: &str) -> Result<()> {
    let ok: Option<i64> = conn
        .query_row(
            "SELECT 1 FROM reminder WHERE id = ?1 AND deleted = 0",
            [id],
            |r| r.get(0),
        )
        .optional()?;
    ok.map(|_| ())
        .ok_or_else(|| Error::NotFound(format!("reminder {id}")))
}

fn load_reminders(conn: &Connection, task_id: Option<&str>) -> Result<Vec<Reminder>> {
    let mut stmt = conn.prepare(
        "SELECT id, task_id, kind, offset_min, at FROM reminder \
         WHERE deleted = 0 AND (?1 IS NULL OR task_id = ?1) ORDER BY id",
    )?;
    let rows = stmt.query_map([task_id], |r| {
        let kind: String = r.get(2)?;
        let offset: Option<i64> = r.get(3)?;
        let at: Option<i64> = r.get(4)?;
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            kind,
            offset,
            at,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (id, task_id, kind, offset, at) = row?;
        let kind = match (kind.as_str(), offset, at) {
            ("absolute", _, Some(at)) => ReminderKind::Absolute { at },
            ("relative", Some(offset_min), _) => ReminderKind::Relative { offset_min },
            _ => continue, // half-synced row
        };
        out.push(Reminder { id, task_id, kind });
    }
    Ok(out)
}

/// When a reminder fires for the task's current occurrence, if it can.
fn fire_time(task: &Task, kind: ReminderKind, tz: Tz) -> Option<i64> {
    match kind {
        ReminderKind::Absolute { at } => Some(at),
        ReminderKind::Relative { offset_min } => {
            let anchor = task.anchor()?;
            let base = if task.all_day {
                local_ms(floating_day(anchor), ALL_DAY_REMINDER_TIME, tz)
            } else {
                anchor
            };
            Some(base - offset_min * 60_000)
        }
    }
}

impl Store {
    pub fn add_reminder(&self, task_id: &str, kind: ReminderKind) -> Result<String> {
        self.write(|w| {
            require_task(w.conn, task_id)?;
            let id = Uuid::now_v7().to_string();
            w.set(Entity::Reminder, &id, "task_id", task_id)?;
            match kind {
                ReminderKind::Relative { offset_min } => {
                    w.set(Entity::Reminder, &id, "kind", "relative")?;
                    w.set(Entity::Reminder, &id, "offset_min", offset_min)?;
                }
                ReminderKind::Absolute { at } => {
                    w.set(Entity::Reminder, &id, "kind", "absolute")?;
                    w.set(Entity::Reminder, &id, "at", at)?;
                }
            }
            Ok(id)
        })
    }

    pub fn remove_reminder(&self, id: &str) -> Result<()> {
        self.write(|w| {
            require_reminder(w.conn, id)?;
            w.set(Entity::Reminder, id, "deleted", true)
        })
    }

    pub fn reminders(&self, task_id: &str) -> Result<Vec<Reminder>> {
        load_reminders(&self.conn, Some(task_id))
    }

    /// Reminders of open tasks that fire in `[from, until)`, soonest first.
    /// `tz` is the device zone, used for all-day tasks.
    pub fn upcoming_reminders(&self, from: i64, until: i64, tz: Tz) -> Result<Vec<ReminderFire>> {
        let sql = format!(
            "SELECT {TASK_COLUMNS} FROM task WHERE status = 'open' \
               AND id IN (SELECT task_id FROM reminder WHERE deleted = 0)"
        );
        let tasks = query_tasks(&self.conn, &sql, [])?;
        let mut fires = Vec::new();
        for task in tasks {
            for r in load_reminders(&self.conn, Some(&task.id))? {
                if let Some(at) = fire_time(&task, r.kind, tz)
                    && at >= from
                    && at < until
                {
                    fires.push(ReminderFire {
                        reminder_id: r.id,
                        task_id: task.id.clone(),
                        title: task.title.clone(),
                        fire_at: at,
                    });
                }
            }
        }
        fires.sort_by(|a, b| {
            a.fire_at
                .cmp(&b.fire_at)
                .then(a.reminder_id.cmp(&b.reminder_id))
        });
        Ok(fires)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dates::{floating_ms, local_datetime};
    use crate::{NewTask, Repeat, RepeatMode, Schedule};
    use chrono::NaiveDate;

    const HOUR: i64 = 3_600_000;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn relative_absolute_and_all_day() {
        let s = Store::open_in_memory().unwrap();
        let tz: Tz = "Europe/Sofia".parse().unwrap();
        let ten = local_ms(
            d("2026-11-25"),
            NaiveTime::from_hms_opt(10, 0, 0).unwrap(),
            tz,
        );
        let timed = s
            .create_task(&NewTask {
                title: "call".into(),
                schedule: Schedule {
                    due_at: Some(ten),
                    tz: Some("Europe/Sofia".into()),
                    ..Default::default()
                },
                ..Default::default()
            })
            .unwrap();
        let all_day = s
            .create_task(&NewTask {
                title: "bins".into(),
                schedule: Schedule {
                    due_at: Some(floating_ms(d("2026-11-25"))),
                    all_day: true,
                    ..Default::default()
                },
                ..Default::default()
            })
            .unwrap();
        s.add_reminder(&timed.id, ReminderKind::Relative { offset_min: 15 })
            .unwrap();
        s.add_reminder(&all_day.id, ReminderKind::Relative { offset_min: 0 })
            .unwrap();
        let undated = s.add_task("ping").unwrap();
        s.add_reminder(&undated.id, ReminderKind::Absolute { at: ten + HOUR })
            .unwrap();
        s.add_reminder(&undated.id, ReminderKind::Relative { offset_min: 5 })
            .unwrap(); // no anchor: never

        let day_start = local_ms(d("2026-11-25"), NaiveTime::MIN, tz);
        let fires = s
            .upcoming_reminders(day_start, day_start + 24 * HOUR, tz)
            .unwrap();
        let shown: Vec<_> = fires
            .iter()
            .map(|f| {
                (
                    f.title.as_str(),
                    local_datetime(f.fire_at, tz).time().to_string(),
                )
            })
            .collect();
        let shown: Vec<(&str, &str)> = shown.iter().map(|(t, at)| (*t, at.as_str())).collect();
        assert_eq!(
            shown,
            [
                ("bins", "09:00:00"),
                ("call", "09:45:00"),
                ("ping", "11:00:00")
            ]
        );

        s.complete_task(&timed.id).unwrap();
        assert_eq!(
            s.upcoming_reminders(day_start, day_start + 24 * HOUR, tz)
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    fn repeating_task_reminds_for_the_next_occurrence() {
        let s = Store::open_in_memory().unwrap();
        let tz = Tz::UTC;
        let t = s
            .create_task(&NewTask {
                title: "water plants".into(),
                schedule: Schedule {
                    due_at: Some(floating_ms(d("2026-11-25"))),
                    all_day: true,
                    ..Default::default()
                },
                ..Default::default()
            })
            .unwrap();
        s.set_repeat(
            &t.id,
            Some(&Repeat {
                rule: "FREQ=DAILY".into(),
                mode: RepeatMode::Fixed,
            }),
        )
        .unwrap();
        let r = s
            .add_reminder(&t.id, ReminderKind::Relative { offset_min: 0 })
            .unwrap();
        s.complete_task(&t.id).unwrap();
        let next_day = floating_ms(d("2026-11-26"));
        let fires = s
            .upcoming_reminders(next_day, next_day + 24 * HOUR, tz)
            .unwrap();
        assert_eq!(fires.len(), 1);
        assert_eq!(fires[0].fire_at, next_day + 9 * HOUR);
        s.remove_reminder(&r).unwrap();
        assert!(s.reminders(&t.id).unwrap().is_empty());
    }
}
