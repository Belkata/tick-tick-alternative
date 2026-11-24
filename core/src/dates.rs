//! Calendar-date helpers shared by filters, recurrence, reminders and the CLI.
//!
//! Timed tasks store an instant (epoch ms) plus their IANA zone. All-day tasks
//! store a *floating* date as UTC midnight of that date, so "Dec 1" stays Dec 1
//! wherever the device is.

use chrono::{
    DateTime, Datelike, Days, Months, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Weekday,
};
use chrono_tz::Tz;

use crate::{Error, Result};

pub(crate) const DAY_MS: i64 = 86_400_000;

/// A calendar date, re-exported for callers that don't depend on chrono.
pub type Day = NaiveDate;

/// The device's zone: `$TT_TZ`, then `$TZ`, then the `/etc/localtime` link
/// (macOS and Arch), else UTC. Android passes its zone in explicitly.
pub fn local_tz() -> Tz {
    let from_env = |k: &str| {
        std::env::var(k)
            .ok()
            .and_then(|v| v.trim_start_matches(':').parse().ok())
    };
    if let Some(tz) = from_env("TT_TZ").or_else(|| from_env("TZ")) {
        return tz;
    }
    std::fs::read_link("/etc/localtime")
        .ok()
        .and_then(|p| {
            let s = p.to_string_lossy().into_owned();
            s.split_once("zoneinfo/")
                .and_then(|(_, name)| name.parse().ok())
        })
        .unwrap_or(Tz::UTC)
}

pub(crate) fn floating_ms(day: NaiveDate) -> i64 {
    day.and_time(NaiveTime::MIN).and_utc().timestamp_millis()
}

pub(crate) fn floating_day(ms: i64) -> NaiveDate {
    DateTime::from_timestamp_millis(ms)
        .unwrap_or_default()
        .date_naive()
}

/// The instant a local date and time happen in `tz`. In a DST gap, the first valid instant after.
pub(crate) fn local_ms(day: NaiveDate, time: NaiveTime, tz: Tz) -> i64 {
    let naive = day.and_time(time);
    match tz.from_local_datetime(&naive) {
        chrono::LocalResult::Single(t) => t.timestamp_millis(),
        chrono::LocalResult::Ambiguous(early, _) => early.timestamp_millis(),
        chrono::LocalResult::None => {
            // Spring-forward gap: try each following minute (gaps are at most a few hours).
            (1..=240)
                .find_map(|m| {
                    tz.from_local_datetime(&(naive + chrono::Duration::minutes(m)))
                        .single()
                })
                .map(|t| t.timestamp_millis())
                .unwrap_or_else(|| naive.and_utc().timestamp_millis())
        }
    }
}

pub(crate) fn local_midnight_ms(day: NaiveDate, tz: Tz) -> i64 {
    local_ms(day, NaiveTime::MIN, tz)
}

pub(crate) fn local_datetime(ms: i64, tz: Tz) -> NaiveDateTime {
    tz.from_utc_datetime(
        &DateTime::from_timestamp_millis(ms)
            .unwrap_or_default()
            .naive_utc(),
    )
    .naive_local()
}

pub(crate) fn today(now_ms: i64, tz: Tz) -> NaiveDate {
    local_datetime(now_ms, tz).date()
}

/// A half-open range of calendar dates `[start, end)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DayRange {
    pub start: NaiveDate,
    pub end: NaiveDate,
}

impl DayRange {
    fn one(d: NaiveDate) -> Self {
        Self {
            start: d,
            end: d + Days::new(1),
        }
    }
}

/// A date token: `today`, `tomorrow`, `yesterday`, `+3d` / `-2d`, `this_week`,
/// `next_week`, `this_month`, `next_month`, a weekday (`mon`…, the next one,
/// today included), or an ISO date `2026-12-01`. Weeks start on Monday.
pub(crate) fn day_range(token: &str, today: NaiveDate) -> Result<DayRange> {
    let t = token.trim().to_ascii_lowercase();
    let bad = || Error::Invalid(format!("unknown date '{token}'"));
    Ok(match t.as_str() {
        "today" => DayRange::one(today),
        "tomorrow" => DayRange::one(today + Days::new(1)),
        "yesterday" => DayRange::one(today - Days::new(1)),
        "this_week" | "next_week" => {
            let monday = today - Days::new(today.weekday().num_days_from_monday() as u64);
            let monday = if t == "next_week" {
                monday + Days::new(7)
            } else {
                monday
            };
            DayRange {
                start: monday,
                end: monday + Days::new(7),
            }
        }
        "this_month" | "next_month" => {
            let first = today.with_day(1).ok_or_else(bad)?;
            let first = if t == "next_month" {
                first + Months::new(1)
            } else {
                first
            };
            DayRange {
                start: first,
                end: first + Months::new(1),
            }
        }
        _ => {
            if let Some(n) = t.strip_suffix('d').and_then(|n| n.parse::<i64>().ok())
                && (t.starts_with('+') || t.starts_with('-'))
            {
                return Ok(DayRange::one(today + chrono::Duration::days(n)));
            }
            if let Ok(wd) = t.parse::<Weekday>() {
                let ahead =
                    (wd.num_days_from_monday() + 7 - today.weekday().num_days_from_monday()) % 7;
                return Ok(DayRange::one(today + Days::new(ahead as u64)));
            }
            DayRange::one(NaiveDate::parse_from_str(&t, "%Y-%m-%d").map_err(|_| bad())?)
        }
    })
}

/// Parse a date the user typed: a date token (all-day) optionally followed by a
/// time `HH:MM` (timed, in `tz`), or a bare time (today). Returns (epoch ms, all_day).
pub fn parse_when(input: &str, now_ms: i64, tz: Tz) -> Result<(i64, bool)> {
    let today = today(now_ms, tz);
    let mut parts = input.split_whitespace();
    let first = parts
        .next()
        .ok_or_else(|| Error::Invalid("empty date".into()))?;
    let second = parts.next();
    if parts.next().is_some() {
        return Err(Error::Invalid(format!("can't read date '{input}'")));
    }
    let parse_time = |s: &str| {
        NaiveTime::parse_from_str(s, "%H:%M")
            .map_err(|_| Error::Invalid(format!("bad time '{s}' (use HH:MM)")))
    };
    match second {
        None => {
            if let Ok(time) = parse_time(first) {
                return Ok((local_ms(today, time, tz), false));
            }
            Ok((floating_ms(day_range(first, today)?.start), true))
        }
        Some(time) => Ok((
            local_ms(day_range(first, today)?.start, parse_time(time)?, tz),
            false,
        )),
    }
}

/// Today's date in UTC (tests).
#[cfg(test)]
pub(crate) fn utc_today() -> NaiveDate {
    chrono::Utc::now().date_naive()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn tokens() {
        let wed = d("2026-11-25");
        assert_eq!(day_range("today", wed).unwrap().start, wed);
        assert_eq!(day_range("+7d", wed).unwrap().start, d("2026-12-02"));
        assert_eq!(day_range("-1d", wed).unwrap().start, d("2026-11-24"));
        let week = day_range("this_week", wed).unwrap();
        assert_eq!((week.start, week.end), (d("2026-11-23"), d("2026-11-30")));
        assert_eq!(day_range("next_week", wed).unwrap().start, d("2026-11-30"));
        let month = day_range("next_month", wed).unwrap();
        assert_eq!((month.start, month.end), (d("2026-12-01"), d("2027-01-01")));
        assert_eq!(day_range("fri", wed).unwrap().start, d("2026-11-27"));
        assert_eq!(day_range("wed", wed).unwrap().start, wed, "today counts");
        assert_eq!(day_range("2027-01-15", wed).unwrap().start, d("2027-01-15"));
        assert!(day_range("someday", wed).is_err());
    }

    #[test]
    fn parse_when_all_day_and_timed() {
        let tz: Tz = "Europe/Sofia".parse().unwrap(); // UTC+2 in November
        let now = local_ms(
            d("2026-11-25"),
            NaiveTime::from_hms_opt(10, 0, 0).unwrap(),
            tz,
        );
        let (ms, all_day) = parse_when("tomorrow", now, tz).unwrap();
        assert!(all_day);
        assert_eq!(floating_day(ms), d("2026-11-26"));
        let (ms, all_day) = parse_when("tomorrow 14:30", now, tz).unwrap();
        assert!(!all_day);
        assert_eq!(local_datetime(ms, tz).to_string(), "2026-11-26 14:30:00");
        assert_eq!(ms, floating_ms(d("2026-11-26")) + (12 * 60 + 30) * 60_000);
        let (ms, _) = parse_when("18:00", now, tz).unwrap();
        assert_eq!(local_datetime(ms, tz).to_string(), "2026-11-25 18:00:00");
    }

    #[test]
    fn dst_gap_moves_forward() {
        let tz: Tz = "Europe/Sofia".parse().unwrap();
        // 2026-03-29 03:00 local doesn't exist (clocks jump 03:00 -> 04:00).
        let ms = local_ms(
            d("2026-03-29"),
            NaiveTime::from_hms_opt(3, 30, 0).unwrap(),
            tz,
        );
        assert_eq!(local_datetime(ms, tz).to_string(), "2026-03-29 04:00:00");
    }
}
