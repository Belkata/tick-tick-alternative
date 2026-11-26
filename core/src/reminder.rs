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
use crate::store::Writer;
use crate::task::{Schedule, TASK_COLUMNS, Task, query_tasks, require_task};
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
            // Synced values can be any i64: saturate rather than overflow.
            Some(base.saturating_sub(offset_min.saturating_mul(60_000)))
        }
    }
}

/// The id of a task's automatic at-due-time reminder. It is derived from the task id, so two
/// devices that each add it while offline write the same row and sync collapses them to one.
fn auto_due_id(task_id: &str) -> String {
    Uuid::new_v5(
        &Uuid::NAMESPACE_OID,
        format!("{task_id}/auto-due").as_bytes(),
    )
    .to_string()
}

/// The automatic reminder for a *local* edit of a task's due date (`before` is the old
/// `(due_at, all_day)`, `after` the schedule just written).
///
/// - The due goes from not timed (none, or all-day) to timed and the task has no reminders:
///   add "at due time" (relative, offset 0), so it follows each occurrence of a repeat.
/// - The due stops being timed (cleared or all-day) and the only reminder is that automatic
///   one: remove it. (A relative reminder on an all-day task would fire at 09:00 on the date,
///   which nobody asked for.)
///
/// Only the schedule setters call this. Remote ops, imports and recurrence advancing write
/// ops directly and never do.
pub(crate) fn auto_due_reminder(
    w: &mut Writer,
    task_id: &str,
    before: (Option<i64>, bool),
    after: &Schedule,
) -> Result<()> {
    let was_timed = before.0.is_some() && !before.1;
    let now_timed = after.due_at.is_some() && !after.all_day;
    let id = auto_due_id(task_id);
    let live = load_reminders(w.conn, Some(task_id))?;
    if now_timed && !was_timed && live.is_empty() {
        w.set(Entity::Reminder, &id, "task_id", task_id)?;
        w.set(Entity::Reminder, &id, "kind", "relative")?;
        w.set(Entity::Reminder, &id, "offset_min", 0_i64)?;
        w.set(Entity::Reminder, &id, "deleted", false)?;
    } else if !now_timed
        && let [only] = live.as_slice()
        && only.id == id
        && only.kind == (ReminderKind::Relative { offset_min: 0 })
    {
        w.set(Entity::Reminder, &id, "deleted", true)?;
    }
    Ok(())
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
    fn the_largest_offsets_do_not_overflow() {
        let s = Store::open_in_memory().unwrap();
        let t = s
            .create_task(&NewTask {
                title: "x".into(),
                schedule: Schedule {
                    due_at: Some(HOUR),
                    tz: Some("UTC".into()),
                    ..Default::default()
                },
                ..Default::default()
            })
            .unwrap();
        for offset_min in [100_000_000, -100_000_000] {
            s.add_reminder(&t.id, ReminderKind::Relative { offset_min })
                .unwrap();
        }
        s.upcoming_reminders(0, i64::MAX, Tz::UTC).unwrap();
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
            .create_task_with(
                &NewTask {
                    title: "call".into(),
                    schedule: Schedule {
                        due_at: Some(ten),
                        tz: Some("Europe/Sofia".into()),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                false,
            )
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

    // --- the automatic at-due-time reminder ---

    fn timed(due: i64) -> Schedule {
        Schedule {
            due_at: Some(due),
            tz: Some("UTC".into()),
            ..Default::default()
        }
    }

    fn all_day() -> Schedule {
        Schedule {
            due_at: Some(floating_ms(d("2026-11-25"))),
            all_day: true,
            ..Default::default()
        }
    }

    fn kinds(s: &Store, id: &str) -> Vec<ReminderKind> {
        s.reminders(id)
            .unwrap()
            .into_iter()
            .map(|r| r.kind)
            .collect()
    }

    const AT_DUE: ReminderKind = ReminderKind::Relative { offset_min: 0 };

    #[test]
    fn a_timed_due_from_nothing_adds_an_at_due_reminder() {
        let s = Store::open_in_memory().unwrap();
        let t = s.add_task("call").unwrap();
        s.set_task_due(&t.id, Some(10 * HOUR)).unwrap();
        assert_eq!(kinds(&s, &t.id), [AT_DUE]);
        let fires = s.upcoming_reminders(0, 24 * HOUR, Tz::UTC).unwrap();
        assert_eq!(fires.len(), 1);
        assert_eq!(fires[0].fire_at, 10 * HOUR);
        // Same through create_task and set_schedule.
        let c = s
            .create_task(&NewTask {
                title: "quick".into(),
                schedule: timed(HOUR),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(kinds(&s, &c.id), [AT_DUE]);
        let u = s.add_task("later").unwrap();
        s.set_schedule(&u.id, Some(&timed(HOUR))).unwrap();
        assert_eq!(kinds(&s, &u.id), [AT_DUE]);
    }

    #[test]
    fn a_timed_due_from_all_day_adds_one_and_a_date_only_does_not() {
        let s = Store::open_in_memory().unwrap();
        let t = s.add_task("x").unwrap();
        s.set_schedule(&t.id, Some(&all_day())).unwrap();
        assert!(kinds(&s, &t.id).is_empty());
        s.set_schedule(&t.id, Some(&timed(10 * HOUR))).unwrap();
        assert_eq!(kinds(&s, &t.id), [AT_DUE]);
    }

    #[test]
    fn changing_the_time_adds_nothing_and_a_deleted_one_stays_deleted() {
        let s = Store::open_in_memory().unwrap();
        let t = s.add_task("x").unwrap();
        s.set_task_due(&t.id, Some(10 * HOUR)).unwrap();
        s.set_task_due(&t.id, Some(11 * HOUR)).unwrap();
        assert_eq!(kinds(&s, &t.id), [AT_DUE]);
        let id = s.reminders(&t.id).unwrap()[0].id.clone();
        s.remove_reminder(&id).unwrap();
        s.set_task_due(&t.id, Some(12 * HOUR)).unwrap();
        assert!(kinds(&s, &t.id).is_empty());
        // Going through no date and back is a new "set the time": it comes back, revived.
        s.set_task_due(&t.id, None).unwrap();
        s.set_task_due(&t.id, Some(12 * HOUR)).unwrap();
        assert_eq!(kinds(&s, &t.id), [AT_DUE]);
    }

    #[test]
    fn existing_reminders_are_left_alone() {
        let s = Store::open_in_memory().unwrap();
        let t = s.add_task("x").unwrap();
        s.add_reminder(&t.id, ReminderKind::Relative { offset_min: 30 })
            .unwrap();
        s.set_task_due(&t.id, Some(10 * HOUR)).unwrap();
        assert_eq!(
            kinds(&s, &t.id),
            [ReminderKind::Relative { offset_min: 30 }]
        );
        // Not removed when the due goes away, either: it is not the automatic one.
        s.set_task_due(&t.id, None).unwrap();
        assert_eq!(kinds(&s, &t.id).len(), 1);
        // A user's own "at time" reminder is not the automatic one.
        let u = s.add_task("y").unwrap();
        s.add_reminder(&u.id, AT_DUE).unwrap();
        s.set_task_due(&u.id, Some(10 * HOUR)).unwrap();
        s.set_schedule(&u.id, None).unwrap();
        assert_eq!(kinds(&s, &u.id), [AT_DUE]);
    }

    #[test]
    fn going_all_day_or_clearing_removes_only_the_automatic_one() {
        let s = Store::open_in_memory().unwrap();
        let t = s.add_task("x").unwrap();
        s.set_task_due(&t.id, Some(10 * HOUR)).unwrap();
        s.set_schedule(&t.id, Some(&all_day())).unwrap();
        assert!(kinds(&s, &t.id).is_empty());
        s.set_schedule(&t.id, Some(&timed(10 * HOUR))).unwrap();
        assert_eq!(kinds(&s, &t.id), [AT_DUE]);
        s.set_schedule(&t.id, None).unwrap();
        assert!(kinds(&s, &t.id).is_empty());
        // With another reminder next to it, the automatic one stays.
        s.set_task_due(&t.id, Some(10 * HOUR)).unwrap();
        s.add_reminder(&t.id, ReminderKind::Relative { offset_min: 5 })
            .unwrap();
        s.set_task_due(&t.id, None).unwrap();
        assert_eq!(kinds(&s, &t.id).len(), 2);
    }

    #[test]
    fn import_recurrence_and_remote_ops_do_not_add_one() {
        let s = Store::open_in_memory().unwrap();
        // Import path: create_task_with(.., false).
        let t = s
            .create_task_with(
                &NewTask {
                    title: "imported".into(),
                    schedule: timed(HOUR),
                    ..Default::default()
                },
                false,
            )
            .unwrap();
        assert!(kinds(&s, &t.id).is_empty());
        // Recurrence advancing: an all-day repeat gains nothing, and a timed one keeps one.
        let r = s.add_task("daily").unwrap();
        s.set_schedule(&r.id, Some(&all_day())).unwrap();
        s.set_repeat(
            &r.id,
            Some(&Repeat {
                rule: "FREQ=DAILY".into(),
                mode: RepeatMode::Fixed,
            }),
        )
        .unwrap();
        s.complete_task(&r.id).unwrap();
        assert!(kinds(&s, &r.id).is_empty());
        // Remote ops: a peer's timed task (made without a reminder) arrives as it is.
        let peer = Store::open_in_memory().unwrap();
        let pt = peer
            .create_task_with(
                &NewTask {
                    title: "p".into(),
                    schedule: timed(HOUR),
                    ..Default::default()
                },
                false,
            )
            .unwrap();
        s.apply_remote(&peer.ops().unwrap()).unwrap();
        assert!(s.get_task(&pt.id).unwrap().unwrap().due_at.is_some());
        assert!(kinds(&s, &pt.id).is_empty());
    }

    #[test]
    fn a_repeating_timed_task_keeps_its_reminder_across_occurrences() {
        let s = Store::open_in_memory().unwrap();
        let t = s.add_task("pill").unwrap();
        s.set_schedule(&t.id, Some(&timed(10 * HOUR))).unwrap();
        s.set_repeat(
            &t.id,
            Some(&Repeat {
                rule: "FREQ=DAILY".into(),
                mode: RepeatMode::Fixed,
            }),
        )
        .unwrap();
        s.complete_task(&t.id).unwrap();
        assert_eq!(kinds(&s, &t.id), [AT_DUE]);
        let fires = s.upcoming_reminders(24 * HOUR, 48 * HOUR, Tz::UTC).unwrap();
        assert_eq!(fires.len(), 1);
        assert_eq!(fires[0].fire_at, 34 * HOUR);
    }

    #[test]
    fn concurrent_automatic_reminders_collapse_to_one() {
        let a = Store::open_in_memory().unwrap();
        let b = Store::open_in_memory().unwrap();
        let t = a.add_task("shared").unwrap();
        b.apply_remote(&a.ops().unwrap()).unwrap();
        // Both devices set a time while offline from each other.
        a.set_task_due(&t.id, Some(10 * HOUR)).unwrap();
        b.set_task_due(&t.id, Some(11 * HOUR)).unwrap();
        let (oa, ob) = (a.ops().unwrap(), b.ops().unwrap());
        a.apply_remote(&ob).unwrap();
        b.apply_remote(&oa).unwrap();
        assert_eq!(a.reminders(&t.id).unwrap().len(), 1);
        assert_eq!(kinds(&b, &t.id), [AT_DUE]);
        assert_eq!(a.state_dump().unwrap(), b.state_dump().unwrap());
    }
}
