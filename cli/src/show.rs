//! How `tt` prints tasks, dates and tags.

use chrono_tz::Tz;
use tt_core::{Store, Task};
pub use tt_core::{offset_label, when_label as when};

/// Characters of the task id that `tt` prints and accepts.
pub const SHORT_ID_LEN: usize = 8;

/// UUIDv7 starts with a timestamp, so the random tail is the part that tells tasks apart.
pub fn short_id(id: &str) -> &str {
    &id[id.len().saturating_sub(SHORT_ID_LEN)..]
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

/// One line per task: `id  !! title   due · @list #tag`.
pub fn line(store: &Store, t: &Task, now_ms: i64, tz: Tz, indent: usize) -> String {
    let mut extras = Vec::new();
    if let Some(due) = t.due_at {
        let mut s = when(due, t.all_day, now_ms, tz);
        if tt_core::is_overdue(due, t.all_day, now_ms, tz) && t.status == tt_core::Status::Open {
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
    }
}
