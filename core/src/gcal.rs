//! Google Calendar, the parts that need no network (plan/06 Google Calendar, M5b).
//!
//! *Events in:* Google's events are cached per device in `gcal_event` and shown read-only.
//! *Tasks out:* a task becomes an event with a deterministic id, so any device can push it
//! without making duplicates. HTTP and OAuth live in a later slice; this module is the
//! cache and the two mappings, so it can be tested without Google.

use chrono::NaiveDate;
use chrono_tz::Tz;
use rusqlite::{OptionalExtension, params};
use serde_json::{Value, json};

use crate::calendar::DEFAULT_BLOCK_MIN;
use crate::dates::{DAY_MS, floating_day, floating_ms, local_midnight_ms};
use crate::recur::task_zone;
use crate::task::{Status, TASK_COLUMNS, Task, query_tasks};
use crate::{Error, Result, Store};

const B32HEX: &[u8; 32] = b"0123456789abcdefghijklmnopqrstuv";

/// A task's Google event id: its UUID in base32hex, which Google accepts (a–v, 0–9,
/// 5–1024 characters). 26 characters, lowercase, stable on every device.
pub fn task_event_id(task_id: &str) -> Result<String> {
    let uuid = uuid::Uuid::parse_str(task_id)
        .map_err(|_| Error::Invalid(format!("not a task id: {task_id}")))?;
    let (mut acc, mut bits, mut out) = (0u32, 0u32, String::new());
    for byte in uuid.as_bytes() {
        acc = (acc << 8) | *byte as u32;
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(B32HEX[((acc >> bits) & 31) as usize] as char);
        }
        acc &= (1 << bits) - 1;
    }
    if bits > 0 {
        out.push(B32HEX[((acc << (5 - bits)) & 31) as usize] as char);
    }
    Ok(out)
}

/// The body for `events.insert` / `events.update` of a task, or `None` when the task has
/// no place on the calendar (deleted, hidden from it, or without a date). Done tasks stay
/// on Google with a `✓` prefix (decided 2026-11-24). Repeats are not expanded here: the
/// push plan ([`Store::gcal_push_plan`]) sends each occurrence as its own event.
pub fn task_event_body(t: &Task) -> Result<Option<Value>> {
    let Some(start) = t.anchor() else {
        return Ok(None);
    };
    if t.status == Status::Deleted || !t.in_calendar {
        return Ok(None);
    }
    let end = start + t.duration_min.unwrap_or(DEFAULT_BLOCK_MIN) * 60_000;
    Ok(Some(event_body(
        t,
        &task_event_id(&t.id)?,
        start,
        end,
        t.all_day,
        t.status == Status::Done,
    )))
}

/// One event of `t`: `start`/`end` in epoch ms (all-day: floating dates, end ignored).
fn event_body(t: &Task, id: &str, start: i64, end: i64, all_day: bool, done: bool) -> Value {
    let title = if done {
        format!("✓ {}", t.title)
    } else {
        t.title.clone()
    };
    let (start_v, end_v) = if all_day {
        let day = floating_day(start);
        let next = day.succ_opt().unwrap_or(day);
        (
            json!({ "date": date_str(day) }),
            json!({ "date": date_str(next) }),
        )
    } else {
        let zone = task_zone(t).name();
        (
            json!({ "dateTime": rfc3339(start), "timeZone": zone }),
            json!({ "dateTime": rfc3339(end), "timeZone": zone }),
        )
    };
    json!({
        "id": id,
        "summary": title,
        "description": t.notes,
        "start": start_v,
        "end": end_v,
        "extendedProperties": { "private": { "tt_task_id": t.id } },
    })
}

/// How far back done tasks and past occurrences stay on Google, and how far ahead the
/// occurrences of a repeating task are pushed (plan/19 slice 4).
pub const PUSH_PAST_DAYS: i64 = 30;
pub const PUSH_FUTURE_DAYS: i64 = 120;

/// One event the "Tasks" calendar on Google should hold.
#[derive(Debug, Clone, PartialEq)]
pub struct PushEvent {
    /// Google's event id: the task's [`task_event_id`], plus `v` and the occurrence's UTC
    /// start (`yyyymmddhhmm`) for the occurrences of a repeating task.
    pub id: String,
    pub task_id: String,
    /// The `events.insert` body, with a `tt_hash` of its content in the private properties.
    pub body: Value,
}

/// FNV-1a 64 over the body's JSON: stable across builds (unlike `DefaultHasher`), so two
/// devices agree on whether an event on Google is up to date.
fn content_hash(body: &Value) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in body.to_string().bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{h:016x}")
}

fn seal(task_id: &str, mut body: Value) -> PushEvent {
    let hash = content_hash(&body);
    body["extendedProperties"]["private"]["tt_hash"] = Value::String(hash);
    PushEvent {
        id: body["id"].as_str().unwrap_or_default().to_string(),
        task_id: task_id.to_string(),
        body,
    }
}

fn date_str(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

fn rfc3339(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms)
        .map(|d| d.format("%Y-%m-%dT%H:%M:%SZ").to_string())
        .unwrap_or_default()
}

/// One of the user's Google calendars, as listed in Settings.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GcalCalendar {
    pub id: String,
    pub summary: String,
    pub color: Option<String>,
    /// Whether its events show in the app's calendar.
    pub selected: bool,
}

/// A cached Google event, ready to draw.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GcalEvent {
    pub calendar_id: String,
    pub id: String,
    pub summary: String,
    pub start_at: i64,
    pub end_at: i64,
    pub all_day: bool,
    pub html_link: Option<String>,
    pub color: Option<String>,
}

/// What one `events.list` item means for the cache.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GcalEventUpdate {
    Upsert(GcalEvent),
    /// Cancelled, or not something we show: drop it.
    Remove {
        id: String,
    },
}

impl GcalEventUpdate {
    /// Reads one item of Google's `events.list` response. Working-location, out-of-office
    /// and focus-time events are hidden, and so are the events this app pushed itself
    /// (they carry `tt_task_id`; the task already shows as a block).
    pub fn parse(calendar_id: &str, item: &Value) -> Result<GcalEventUpdate> {
        let id = item["id"]
            .as_str()
            .ok_or_else(|| Error::Invalid("event without id".into()))?;
        let remove = || Ok(GcalEventUpdate::Remove { id: id.to_string() });
        let hidden = matches!(
            item["eventType"].as_str(),
            Some("workingLocation" | "outOfOffice" | "focusTime")
        ) || item["extendedProperties"]["private"]["tt_task_id"].is_string();
        if item["status"] == "cancelled" || hidden {
            return remove();
        }
        let (Some((start_at, all_day)), Some((end_at, _))) =
            (when(&item["start"]), when(&item["end"]))
        else {
            return remove();
        };
        Ok(GcalEventUpdate::Upsert(GcalEvent {
            calendar_id: calendar_id.to_string(),
            id: id.to_string(),
            summary: item["summary"].as_str().unwrap_or("(No title)").to_string(),
            start_at,
            end_at,
            all_day,
            html_link: item["htmlLink"].as_str().map(str::to_string),
            color: None,
        }))
    }
}

/// `(epoch ms, all_day)` of a Google start/end object. All-day dates become floating
/// UTC midnights.
fn when(v: &Value) -> Option<(i64, bool)> {
    if let Some(d) = v["date"].as_str() {
        let day = NaiveDate::parse_from_str(d, "%Y-%m-%d").ok()?;
        return Some((floating_ms(day), true));
    }
    let dt = chrono::DateTime::parse_from_rfc3339(v["dateTime"].as_str()?).ok()?;
    Some((dt.timestamp_millis(), false))
}

impl Store {
    /// Replaces the list of Google calendars, keeping each one's selection and token.
    pub fn gcal_set_calendars(&self, calendars: &[GcalCalendar]) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        for c in calendars {
            tx.execute(
                "INSERT INTO gcal_calendar (id, summary, color) VALUES (?1, ?2, ?3)
                 ON CONFLICT(id) DO UPDATE SET summary = ?2, color = ?3",
                params![c.id, c.summary, c.color],
            )?;
        }
        let mut stale = Vec::new();
        {
            let mut q = tx.prepare("SELECT id FROM gcal_calendar")?;
            for id in q.query_map([], |r| r.get::<_, String>(0))? {
                let id = id?;
                if !calendars.iter().any(|c| c.id == id) {
                    stale.push(id);
                }
            }
        }
        for id in stale {
            tx.execute("DELETE FROM gcal_event WHERE calendar_id = ?1", [&id])?;
            tx.execute("DELETE FROM gcal_calendar WHERE id = ?1", [&id])?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Chooses whether a calendar's events show. Hiding drops its cache and token.
    pub fn gcal_select(&self, calendar_id: &str, selected: bool) -> Result<()> {
        let n = self.conn.execute(
            "UPDATE gcal_calendar SET selected = ?2,
                    sync_token = CASE WHEN ?2 = 0 THEN NULL ELSE sync_token END
              WHERE id = ?1",
            params![calendar_id, selected],
        )?;
        if n == 0 {
            return Err(Error::NotFound(calendar_id.to_string()));
        }
        if !selected {
            self.conn.execute(
                "DELETE FROM gcal_event WHERE calendar_id = ?1",
                [calendar_id],
            )?;
        }
        Ok(())
    }

    /// Whether this device pushes tasks to Google (off until the user turns it on).
    pub fn gcal_push_enabled(&self) -> Result<bool> {
        Ok(self.meta_get("gcal_push")?.as_deref() == Some("1"))
    }

    /// Turn pushing on or off for this device. Local only, never synced.
    pub fn gcal_set_push(&self, on: bool) -> Result<()> {
        self.meta_set("gcal_push", Some(if on { "1" } else { "0" }))
    }

    /// The id of this device's "Tasks" calendar on Google, once found or created.
    pub fn gcal_tasks_calendar(&self) -> Result<Option<String>> {
        self.meta_get("gcal_tasks_calendar")
    }

    pub fn gcal_set_tasks_calendar(&self, id: Option<&str>) -> Result<()> {
        self.meta_set("gcal_tasks_calendar", id)
    }

    fn meta_get(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0))
            .optional()?)
    }

    fn meta_set(&self, key: &str, value: Option<&str>) -> Result<()> {
        match value {
            Some(v) => self.conn.execute(
                "INSERT INTO meta (key, value) VALUES (?1, ?2)
                 ON CONFLICT (key) DO UPDATE SET value = excluded.value",
                params![key, v],
            )?,
            None => self
                .conn
                .execute("DELETE FROM meta WHERE key = ?1", [key])?,
        };
        Ok(())
    }

    /// Everything the "Tasks" calendar on Google should hold at `now_ms`, by event id.
    ///
    /// - A task with a date that is shown on the calendar becomes one event (`task_event_id`).
    ///   Open ones always; done ones for [`PUSH_PAST_DAYS`] after their date.
    /// - **Repeats go out as a window of single events**, not as a Google recurring event:
    ///   the app's own occurrences (`Store::calendar`, so "repeat after completion" shows
    ///   only the next one and COUNT/UNTIL are honoured) from [`PUSH_PAST_DAYS`] back to
    ///   [`PUSH_FUTURE_DAYS`] ahead, past ones from the completion history marked done.
    ///   Each occurrence's id carries its UTC start, so it is stable while the task lives.
    ///   Plan/19 slice 4 explains why this beats an RRULE.
    pub fn gcal_push_plan(&self, now_ms: i64) -> Result<Vec<PushEvent>> {
        let mut out = std::collections::BTreeMap::new();
        let sql = format!(
            "SELECT {TASK_COLUMNS} FROM task
              WHERE task.status != 'deleted' AND task.in_calendar = 1
                AND task.repeat_rule IS NULL
                AND coalesce(task.start_at, task.due_at) IS NOT NULL
                AND (task.status = 'open' OR coalesce(task.start_at, task.due_at) >= ?1)"
        );
        for t in query_tasks(&self.conn, &sql, params![now_ms - PUSH_PAST_DAYS * DAY_MS])? {
            if let Some(body) = task_event_body(&t)? {
                let e = seal(&t.id, body);
                out.insert(e.id.clone(), e);
            }
        }
        let blocks = self.calendar(
            now_ms - PUSH_PAST_DAYS * DAY_MS,
            now_ms + PUSH_FUTURE_DAYS * DAY_MS,
            Tz::UTC,
        )?;
        let mut tasks: std::collections::HashMap<String, Option<Task>> = Default::default();
        for b in blocks.into_iter().filter(|b| b.repeating) {
            if !tasks.contains_key(&b.task_id) {
                tasks.insert(b.task_id.clone(), self.get_task(&b.task_id)?);
            }
            let Some(Some(t)) = tasks.get(&b.task_id) else {
                continue;
            };
            let stamp = chrono::DateTime::from_timestamp_millis(b.start_at)
                .map(|d| d.format("%Y%m%d%H%M").to_string())
                .unwrap_or_default();
            let id = format!("{}v{stamp}", task_event_id(&t.id)?);
            let body = event_body(t, &id, b.start_at, b.end_at, b.all_day, b.done);
            out.insert(id, seal(&t.id, body));
        }
        Ok(out.into_values().collect())
    }

    /// Drops every cached Google calendar and event (signing out of Google).
    pub fn gcal_forget(&self) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM gcal_event", [])?;
        tx.execute("DELETE FROM gcal_calendar", [])?;
        tx.execute("DELETE FROM meta WHERE key = 'gcal_tasks_calendar'", [])?;
        tx.commit()?;
        Ok(())
    }

    pub fn gcal_calendars(&self) -> Result<Vec<GcalCalendar>> {
        let mut q = self.conn.prepare(
            "SELECT id, summary, color, selected FROM gcal_calendar ORDER BY summary, id",
        )?;
        let rows = q.query_map([], |r| {
            Ok(GcalCalendar {
                id: r.get(0)?,
                summary: r.get(1)?,
                color: r.get(2)?,
                selected: r.get(3)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// The `syncToken` for the next incremental fetch; `None` means fetch everything.
    pub fn gcal_sync_token(&self, calendar_id: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row(
                "SELECT sync_token FROM gcal_calendar WHERE id = ?1",
                [calendar_id],
                |r| r.get(0),
            )
            .unwrap_or(None))
    }

    /// Applies one fetch. `full` (no token was sent, or Google answered 410 Gone) drops
    /// the calendar's cached events first. The new token is saved with the changes, so a
    /// crash never keeps a token whose events were lost.
    pub fn gcal_apply(
        &self,
        calendar_id: &str,
        full: bool,
        updates: &[GcalEventUpdate],
        next_sync_token: Option<&str>,
    ) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        if full {
            tx.execute(
                "DELETE FROM gcal_event WHERE calendar_id = ?1",
                [calendar_id],
            )?;
        }
        for u in updates {
            match u {
                GcalEventUpdate::Upsert(e) => {
                    tx.execute(
                        "INSERT OR REPLACE INTO gcal_event
                           (calendar_id, id, summary, start_at, end_at, all_day, html_link, color)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                        params![
                            calendar_id,
                            e.id,
                            e.summary,
                            e.start_at,
                            e.end_at,
                            e.all_day,
                            e.html_link,
                            e.color
                        ],
                    )?;
                }
                GcalEventUpdate::Remove { id } => {
                    tx.execute(
                        "DELETE FROM gcal_event WHERE calendar_id = ?1 AND id = ?2",
                        params![calendar_id, id],
                    )?;
                }
            }
        }
        tx.execute(
            "UPDATE gcal_calendar SET sync_token = ?2 WHERE id = ?1",
            params![calendar_id, next_sync_token],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Cached events of selected calendars overlapping `[from, until)`, in the viewer's
    /// zone `tz` (all-day ones run from local midnight to local midnight), by start.
    pub fn gcal_events(&self, from: i64, until: i64, tz: Tz) -> Result<Vec<GcalEvent>> {
        let slack = 2 * DAY_MS;
        let mut q = self.conn.prepare(
            "SELECT e.calendar_id, e.id, e.summary, e.start_at, e.end_at, e.all_day,
                    e.html_link, coalesce(e.color, c.color)
               FROM gcal_event e JOIN gcal_calendar c ON c.id = e.calendar_id
              WHERE c.selected = 1 AND e.start_at < ?2 AND e.end_at > ?1",
        )?;
        let rows = q.query_map(params![from - slack, until + slack], |r| {
            Ok(GcalEvent {
                calendar_id: r.get(0)?,
                id: r.get(1)?,
                summary: r.get(2)?,
                start_at: r.get(3)?,
                end_at: r.get(4)?,
                all_day: r.get(5)?,
                html_link: r.get(6)?,
                color: r.get(7)?,
            })
        })?;
        let mut out = Vec::new();
        for e in rows {
            let mut e = e?;
            if e.all_day {
                e.start_at = local_midnight_ms(floating_day(e.start_at), tz);
                e.end_at = local_midnight_ms(floating_day(e.end_at), tz);
            }
            if e.end_at > from && e.start_at < until {
                out.push(e);
            }
        }
        out.sort_by(|a, b| (a.start_at, &a.id).cmp(&(b.start_at, &b.id)));
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cal(id: &str) -> GcalCalendar {
        GcalCalendar {
            id: id.into(),
            summary: id.into(),
            color: Some("#0b8043".into()),
            selected: false,
        }
    }

    #[test]
    fn event_id_is_valid_and_stable() {
        let id = task_event_id("0190f3a4-7b2c-7d11-9e55-3c1f0a8b2d44").unwrap();
        assert_eq!(id.len(), 26);
        assert!(
            id.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'v').contains(&b))
        );
        assert_eq!(
            id,
            task_event_id("0190F3A4-7B2C-7D11-9E55-3C1F0A8B2D44").unwrap()
        );
        assert!(task_event_id("nope").is_err());
        assert_eq!(
            task_event_id("00000000-0000-0000-0000-000000000000").unwrap(),
            "0".repeat(26)
        );
        assert_eq!(
            task_event_id("ffffffff-ffff-ffff-ffff-ffffffffffff").unwrap(),
            format!("{}s", "v".repeat(25))
        );
    }

    #[test]
    fn parses_timed_all_day_and_hidden_events() {
        let timed = json!({"id":"a","summary":"Dentist","htmlLink":"https://g/x",
            "start":{"dateTime":"2026-12-01T09:00:00+02:00"},
            "end":{"dateTime":"2026-12-01T10:00:00+02:00"}});
        let GcalEventUpdate::Upsert(e) = GcalEventUpdate::parse("c", &timed).unwrap() else {
            panic!()
        };
        let seven_utc = chrono::DateTime::parse_from_rfc3339("2026-12-01T07:00:00Z")
            .unwrap()
            .timestamp_millis();
        assert_eq!(
            (e.start_at, e.end_at - e.start_at, e.all_day),
            (seven_utc, 3_600_000, false)
        );
        let day = json!({"id":"b","start":{"date":"2026-12-24"},"end":{"date":"2026-12-25"}});
        let GcalEventUpdate::Upsert(e) = GcalEventUpdate::parse("c", &day).unwrap() else {
            panic!()
        };
        assert!(e.all_day);
        assert_eq!(e.summary, "(No title)");
        assert_eq!(e.end_at - e.start_at, DAY_MS);
        for hidden in [
            json!({"id":"c","status":"cancelled"}),
            json!({"id":"d","eventType":"workingLocation",
                   "start":{"date":"2026-12-01"},"end":{"date":"2026-12-02"}}),
            json!({"id":"e","extendedProperties":{"private":{"tt_task_id":"x"}},
                   "start":{"date":"2026-12-01"},"end":{"date":"2026-12-02"}}),
        ] {
            assert!(matches!(
                GcalEventUpdate::parse("c", &hidden).unwrap(),
                GcalEventUpdate::Remove { .. }
            ));
        }
    }

    #[test]
    fn cache_sync_token_and_selection() {
        let s = Store::open_in_memory().unwrap();
        s.gcal_set_calendars(&[cal("a"), cal("b")]).unwrap();
        s.gcal_select("a", true).unwrap();
        let ev = |id: &str, day: &str, next: &str| {
            GcalEventUpdate::parse(
                "a",
                &json!({"id":id,"summary":id,"start":{"date":day},"end":{"date":next}}),
            )
            .unwrap()
        };
        let tz: Tz = "Europe/Sofia".parse().unwrap();
        let day = |d| local_midnight_ms(NaiveDate::from_ymd_opt(2026, 12, d).unwrap(), tz);
        let (from, until) = (day(1), day(8));
        s.gcal_apply(
            "a",
            true,
            &[
                ev("x", "2026-12-02", "2026-12-03"),
                ev("y", "2026-12-20", "2026-12-21"),
            ],
            Some("T1"),
        )
        .unwrap();
        assert_eq!(s.gcal_sync_token("a").unwrap().as_deref(), Some("T1"));
        let got = s.gcal_events(from, until, tz).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].id, "x");
        assert_eq!(got[0].start_at, day(2));
        assert_eq!(got[0].color.as_deref(), Some("#0b8043"));
        // Incremental removal, then a full resync (410) clears what is left first.
        s.gcal_apply(
            "a",
            false,
            &[GcalEventUpdate::Remove { id: "x".into() }],
            Some("T2"),
        )
        .unwrap();
        assert!(s.gcal_events(from, until, tz).unwrap().is_empty());
        s.gcal_apply("a", true, &[], None).unwrap();
        assert_eq!(s.gcal_sync_token("a").unwrap(), None);
        // Hiding a calendar drops its events and token; unknown calendars are refused.
        s.gcal_apply(
            "a",
            false,
            &[ev("z", "2026-12-02", "2026-12-03")],
            Some("T3"),
        )
        .unwrap();
        s.gcal_select("a", false).unwrap();
        assert!(s.gcal_events(from, until, tz).unwrap().is_empty());
        assert_eq!(s.gcal_sync_token("a").unwrap(), None);
        assert!(s.gcal_select("nope", true).is_err());
        // Dropping a calendar from the list removes it.
        s.gcal_set_calendars(&[cal("b")]).unwrap();
        assert_eq!(s.gcal_calendars().unwrap().len(), 1);
        // Signing out forgets everything.
        s.gcal_forget().unwrap();
        assert!(s.gcal_calendars().unwrap().is_empty());
    }

    fn dated(s: &Store, title: &str, due: i64, repeat: Option<&str>) -> Task {
        let t = s
            .create_task(&crate::NewTask {
                title: title.into(),
                schedule: crate::Schedule {
                    start_at: Some(due),
                    due_at: Some(due),
                    tz: Some("Europe/Sofia".into()),
                    ..Default::default()
                },
                ..Default::default()
            })
            .unwrap();
        if let Some(rule) = repeat {
            s.set_repeat(
                &t.id,
                Some(&crate::Repeat {
                    rule: rule.into(),
                    mode: crate::RepeatMode::Fixed,
                }),
            )
            .unwrap();
        }
        t
    }

    const NOW: i64 = 1_796_000_000_000;

    #[test]
    fn push_plan_follows_the_task() {
        let s = Store::open_in_memory().unwrap();
        let a = dated(&s, "Dentist", NOW + 2 * DAY_MS, None);
        let id = task_event_id(&a.id).unwrap();
        let plan = s.gcal_push_plan(NOW).unwrap();
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].id, id);
        assert_eq!(plan[0].body["summary"], "Dentist");
        let hash = plan[0].body["extendedProperties"]["private"]["tt_hash"].clone();
        assert!(hash.is_string());
        // Same task, same plan; an edit changes the hash.
        assert_eq!(plan, s.gcal_push_plan(NOW).unwrap());
        s.set_title(&a.id, "Dentist 2").unwrap();
        let again = s.gcal_push_plan(NOW).unwrap();
        assert_ne!(
            again[0].body["extendedProperties"]["private"]["tt_hash"],
            hash
        );
        // Done tasks stay with a check mark, for 30 days.
        s.complete_task(&a.id).unwrap();
        assert_eq!(
            s.gcal_push_plan(NOW).unwrap()[0].body["summary"],
            "✓ Dentist 2"
        );
        assert!(s.gcal_push_plan(NOW + 60 * DAY_MS).unwrap().is_empty());
        // Hidden from the calendar, or trashed: no event.
        let b = dated(&s, "B", NOW + DAY_MS, None);
        assert_eq!(s.gcal_push_plan(NOW).unwrap().len(), 2);
        s.set_in_calendar(&b.id, false).unwrap();
        assert_eq!(s.gcal_push_plan(NOW).unwrap().len(), 1);
        s.set_in_calendar(&b.id, true).unwrap();
        s.trash_task(&b.id).unwrap();
        assert_eq!(s.gcal_push_plan(NOW).unwrap().len(), 1);
    }

    #[test]
    fn repeats_become_a_window_of_occurrences() {
        let s = Store::open_in_memory().unwrap();
        let t = dated(&s, "Water plants", NOW + DAY_MS, Some("FREQ=DAILY"));
        let plan = s.gcal_push_plan(NOW).unwrap();
        // About PUSH_FUTURE_DAYS occurrences, each its own id under the task's.
        assert!(plan.len() >= 118 && plan.len() <= 122, "{}", plan.len());
        let base = task_event_id(&t.id).unwrap();
        assert!(plan.iter().all(|e| e.id.starts_with(&format!("{base}v"))));
        let ids: std::collections::HashSet<_> = plan.iter().map(|e| &e.id).collect();
        assert_eq!(ids.len(), plan.len());
        // Completing moves the series on: the done occurrence stays, marked.
        s.complete_task(&t.id).unwrap();
        let after = s.gcal_push_plan(NOW).unwrap();
        let done: Vec<_> = after
            .iter()
            .filter(|e| e.body["summary"] == "✓ Water plants")
            .collect();
        assert_eq!(done.len(), 1);
        assert_eq!(done[0].id, plan[0].id);
    }

    #[test]
    fn push_settings_are_per_device_and_forgotten_on_sign_out() {
        let s = Store::open_in_memory().unwrap();
        assert!(!s.gcal_push_enabled().unwrap());
        s.gcal_set_push(true).unwrap();
        assert!(s.gcal_push_enabled().unwrap());
        s.gcal_set_tasks_calendar(Some("cal1")).unwrap();
        assert_eq!(s.gcal_tasks_calendar().unwrap().as_deref(), Some("cal1"));
        s.gcal_forget().unwrap();
        assert_eq!(s.gcal_tasks_calendar().unwrap(), None);
        assert!(s.gcal_push_enabled().unwrap(), "the switch stays");
    }
}
