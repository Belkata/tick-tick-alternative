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

/// The device's zone: `$UTRE_TZ`, then `$TZ`, then the `/etc/localtime` link
/// (macOS and Arch), else UTC. Android passes its zone in explicitly.
pub fn local_tz() -> Tz {
    let parse = |v: String| v.trim_start_matches(':').parse().ok();
    let from_env = crate::envvar::var("TZ")
        .and_then(parse)
        .or_else(|| std::env::var("TZ").ok().and_then(parse));
    if let Some(tz) = from_env {
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
                // `Duration::days` and `date + Duration` panic when out of range, and filter
                // rules arrive from other devices, so this must fail instead.
                let day = chrono::Duration::try_days(n)
                    .and_then(|d| today.checked_add_signed(d))
                    .filter(|d| d.checked_add_days(Days::new(1)).is_some())
                    .ok_or_else(bad)?;
                return Ok(DayRange::one(day));
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

// ----- Labels shared by the CLI and the app -----

fn local_naive(ms: i64, tz: Tz) -> NaiveDateTime {
    tz.timestamp_millis_opt(ms)
        .single()
        .map(|t| t.naive_local())
        .unwrap_or_default()
}

/// The calendar day a stored date falls on: the floating date for all-day,
/// else the local date in `tz`.
pub fn local_day(ms: i64, all_day: bool, tz: Tz) -> Day {
    if all_day {
        DateTime::from_timestamp_millis(ms)
            .unwrap_or_default()
            .date_naive()
    } else {
        local_naive(ms, tz).date()
    }
}

fn day_label(day: NaiveDate, today: NaiveDate) -> String {
    match (day - today).num_days() {
        0 => "today".into(),
        1 => "tomorrow".into(),
        -1 => "yesterday".into(),
        2..=6 => day.format("%a").to_string(),
        _ if day.year() == today.year() => day.format("%a %-d %b").to_string(),
        _ => day.format("%-d %b %Y").to_string(),
    }
}

/// `today`, `Fri 14:30`, `3 Dec`…: an all-day date or a local date and time.
pub fn when_label(ms: i64, all_day: bool, now_ms: i64, tz: Tz) -> String {
    let today = local_naive(now_ms, tz).date();
    if all_day {
        day_label(local_day(ms, true, tz), today)
    } else {
        let t = local_naive(ms, tz);
        format!("{} {}", day_label(t.date(), today), t.format("%H:%M"))
    }
}

/// Is a due date in the past? All-day dates are late from the next day on.
pub fn is_overdue(due_ms: i64, all_day: bool, now_ms: i64, tz: Tz) -> bool {
    if all_day {
        local_day(due_ms, true, tz) < local_naive(now_ms, tz).date()
    } else {
        due_ms < now_ms
    }
}

/// `at the time`, `15m before`, `1d2h before`.
pub fn offset_label(min: i64) -> String {
    if min == 0 {
        return "at the time".into();
    }
    let (days, hours, mins) = (min / 1440, (min / 60) % 24, min % 60);
    let mut parts = Vec::new();
    if days > 0 {
        parts.push(format!("{days}d"));
    }
    if hours > 0 {
        parts.push(format!("{hours}h"));
    }
    if mins > 0 {
        parts.push(format!("{mins}m"));
    }
    format!("{} before", parts.join(""))
}

/// Parse a task length: `90`, `45m`, `45 min`, `1h`, `1h30`, `1h30m`, `1:30`, `1.5h`.
/// `none` (or empty) clears it (`Ok(None)`). A bare number is minutes. Always positive.
pub fn parse_duration(input: &str) -> Result<Option<i64>> {
    let bad = || {
        Error::Invalid(format!(
            "can't read '{}' as a duration (try 90m or 1h30)",
            input.trim()
        ))
    };
    let t = input.trim().to_lowercase().replace(' ', "");
    if t.is_empty() || t == "none" || t == "no" || t == "-" {
        return Ok(None);
    }
    let num = |s: &str| -> Result<i64> {
        if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) || s.len() > 9 {
            return Err(bad());
        }
        s.parse().map_err(|_| bad())
    };
    let min = if let Some((h, m)) = t.split_once(':') {
        let m = num(m)?;
        if m >= 60 {
            return Err(bad());
        }
        num(h)? * 60 + m
    } else if let Some((h, rest)) = t.split_once('h') {
        let rest = rest
            .strip_suffix("min")
            .or_else(|| rest.strip_suffix('m'))
            .unwrap_or(rest);
        match h.split_once(['.', ',']).filter(|_| rest.is_empty()) {
            // 1.5h: a decimal hour on its own.
            Some((whole, frac)) => {
                let scale = 10_i64.pow(frac.len().min(6) as u32);
                num(whole)? * 60 + (num(frac)? * 60 + scale / 2) / scale
            }
            None => num(h)? * 60 + if rest.is_empty() { 0 } else { num(rest)? },
        }
    } else {
        let digits = t
            .strip_suffix("minutes")
            .or_else(|| t.strip_suffix("min"))
            .or_else(|| t.strip_suffix('m'))
            .unwrap_or(&t);
        num(digits)?
    };
    if min <= 0 || min > 100_000_000 {
        return Err(bad());
    }
    Ok(Some(min))
}

/// `45 min`, `2 h`, `1 h 30 min`.
pub fn duration_label(min: i64) -> String {
    match (min / 60, min % 60) {
        (0, m) => format!("{m} min"),
        (h, 0) => format!("{h} h"),
        (h, m) => format!("{h} h {m} min"),
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
    fn durations_parse_in_every_common_shape() {
        for (text, want) in [
            ("90", Some(90)),
            ("45m", Some(45)),
            ("45 min", Some(45)),
            ("1h", Some(60)),
            ("1h30", Some(90)),
            ("1h30m", Some(90)),
            ("1:30", Some(90)),
            ("0:45", Some(45)),
            ("1.5h", Some(90)),
            ("2H", Some(120)),
            ("none", None),
            ("", None),
        ] {
            assert_eq!(parse_duration(text).unwrap(), want, "{text}");
        }
        for bad in [
            "0",
            "0m",
            "-5",
            "abc",
            "1:75",
            "1h99x",
            "99999999999",
            "1.h5",
        ] {
            assert!(parse_duration(bad).is_err(), "{bad}");
        }
        assert_eq!(duration_label(45), "45 min");
        assert_eq!(duration_label(120), "2 h");
        assert_eq!(duration_label(90), "1 h 30 min");
    }

    #[test]
    fn huge_offsets_are_errors_not_panics() {
        let today = d("2026-11-25");
        for t in [
            "+99999999999999d",
            "-99999999999999d",
            "+9223372036854775807d",
            "+100000000d",
        ] {
            assert!(day_range(t, today).is_err(), "{t}");
        }
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

    #[test]
    fn labels() {
        let today = NaiveDate::from_ymd_opt(2026, 11, 25).unwrap();
        assert_eq!(day_label(today, today), "today");
        assert_eq!(day_label(today + Days::new(2), today), "Fri");
        assert_eq!(day_label(today + Days::new(20), today), "Tue 15 Dec");
        assert_eq!(day_label(today + Days::new(60), today), "24 Jan 2027");
        assert_eq!(offset_label(90), "1h30m before");
        assert_eq!(offset_label(0), "at the time");
        assert_eq!(offset_label(1560), "1d2h before");
    }
}
