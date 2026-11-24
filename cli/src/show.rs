//! How `tt` prints tasks, dates and tags.

use chrono::{Datelike, Duration, NaiveDate, TimeZone};
use chrono_tz::Tz;
use tt_core::{Store, Task};

/// Characters of the task id that `tt` prints and accepts.
pub const SHORT_ID_LEN: usize = 8;

/// UUIDv7 starts with a timestamp, so the random tail is the part that tells tasks apart.
pub fn short_id(id: &str) -> &str {
    &id[id.len().saturating_sub(SHORT_ID_LEN)..]
}

fn local(ms: i64, tz: Tz) -> chrono::NaiveDateTime {
    tz.timestamp_millis_opt(ms)
        .single()
        .map(|t| t.naive_local())
        .unwrap_or_default()
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
pub fn when(ms: i64, all_day: bool, now_ms: i64, tz: Tz) -> String {
    let today = local(now_ms, tz).date();
    if all_day {
        let day = chrono::DateTime::from_timestamp_millis(ms)
            .unwrap_or_default()
            .date_naive();
        day_label(day, today)
    } else {
        let t = local(ms, tz);
        format!("{} {}", day_label(t.date(), today), t.format("%H:%M"))
    }
}

pub fn priority_mark(p: u8) -> &'static str {
    match p {
        3 => "!!! ",
        2 => "!!  ",
        1 => "!   ",
        _ => "    ",
    }
}

pub fn priority_name(p: u8) -> &'static str {
    match p {
        3 => "high",
        2 => "medium",
        1 => "low",
        _ => "none",
    }
}

fn is_overdue(t: &Task, now_ms: i64, tz: Tz) -> bool {
    let Some(due) = t.due_at else { return false };
    if t.all_day {
        let day = chrono::DateTime::from_timestamp_millis(due)
            .unwrap_or_default()
            .date_naive();
        day < local(now_ms, tz).date()
    } else {
        due < now_ms
    }
}

/// One line per task: `id  !! title   due · @list #tag`.
pub fn line(store: &Store, t: &Task, now_ms: i64, tz: Tz, indent: usize) -> String {
    let mut extras = Vec::new();
    if let Some(due) = t.due_at {
        let mut s = when(due, t.all_day, now_ms, tz);
        if is_overdue(t, now_ms, tz) && t.status == tt_core::Status::Open {
            s = format!("{s} (overdue)");
        }
        if t.repeat_rule.is_some() {
            s.push_str(" ↻");
        }
        extras.push(s);
    } else if let Some(start) = t.start_at {
        extras.push(format!("from {}", when(start, t.all_day, now_ms, tz)));
    }
    if let Some(list) = t
        .list_id
        .as_deref()
        .and_then(|l| store.find_list(l).ok().flatten())
    {
        extras.push(format!("@{}", list.name));
    }
    for tag in store.task_tags(&t.id).unwrap_or_default() {
        extras.push(format!("#{}", tag.path));
    }
    let check = match t.status {
        tt_core::Status::Done => "✓ ",
        tt_core::Status::Deleted => "✗ ",
        tt_core::Status::Open => "",
    };
    let extras = if extras.is_empty() {
        String::new()
    } else {
        format!("   {}", extras.join("  "))
    };
    format!(
        "{}  {}{}{check}{}{extras}",
        short_id(&t.id),
        priority_mark(t.priority),
        "  ".repeat(indent),
        t.title
    )
}

/// `15m`, `1h30m`, `2d` → minutes.
pub fn parse_offset(s: &str) -> Option<i64> {
    let s = s.trim();
    if s == "0" {
        return Some(0);
    }
    let mut total = 0i64;
    let mut num = String::new();
    for c in s.chars() {
        if c.is_ascii_digit() {
            num.push(c);
            continue;
        }
        let n: i64 = num.parse().ok()?;
        num.clear();
        total += match c {
            'm' => n,
            'h' => n * 60,
            'd' => n * 24 * 60,
            _ => return None,
        };
    }
    num.is_empty().then_some(total)
}

pub fn offset_label(min: i64) -> String {
    if min == 0 {
        return "at the time".into();
    }
    let d = Duration::minutes(min);
    let (days, hours, mins) = (d.num_days(), d.num_hours() % 24, d.num_minutes() % 60);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offsets() {
        assert_eq!(parse_offset("15m"), Some(15));
        assert_eq!(parse_offset("1h30m"), Some(90));
        assert_eq!(parse_offset("2d"), Some(2880));
        assert_eq!(parse_offset("0"), Some(0));
        assert_eq!(parse_offset("15"), None);
        assert_eq!(parse_offset("soon"), None);
        assert_eq!(offset_label(90), "1h30m before");
    }

    #[test]
    fn labels() {
        let today = NaiveDate::from_ymd_opt(2026, 11, 25).unwrap();
        assert_eq!(day_label(today, today), "today");
        assert_eq!(day_label(today + chrono::Days::new(2), today), "Fri");
        assert_eq!(
            day_label(today + chrono::Days::new(20), today),
            "Tue 15 Dec"
        );
        assert_eq!(
            day_label(today + chrono::Days::new(60), today),
            "24 Jan 2027"
        );
    }
}
