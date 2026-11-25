//! A new task that a filter will show: quick add in a filter view gives the task
//! the filter's list, tags, priority and dates, so it doesn't vanish from the view.
//!
//! The rule is expanded into alternatives (`any` branches, De Morgan for `not`).
//! The first alternative a fresh task can fully satisfy wins. If none can (it needs
//! a time, a repeat, text the title lacks…), the first one is applied as far as it goes.
//!
//! Dates come out all-day: today if the rule allows it, else the nearest allowed day.
//! A tag rule is met with the named tag itself; `has_any` takes the first one that exists.

use chrono::{Days, NaiveDate};
use chrono_tz::Tz;
use rusqlite::Connection;

use super::{Cond, FilterRule, as_bool, as_list, as_str, bad};
use crate::dates::{day_range, floating_ms, today};
use crate::organize::{find_list, find_tag, tag_subtree};
use crate::{Error, NewTask, Result, Schedule, Store};

/// Alternatives kept at most; real filters have a handful.
const MAX_TERMS: usize = 64;

/// A condition, possibly negated.
#[derive(Clone, Copy)]
struct Leaf<'a> {
    c: &'a Cond,
    neg: bool,
}

/// The rule as alternatives (OR) of conditions (AND).
fn dnf(rule: &FilterRule, neg: bool) -> Vec<Vec<Leaf<'_>>> {
    match rule {
        FilterRule::Cond(c) => vec![vec![Leaf { c, neg }]],
        FilterRule::Not { not } => dnf(not, !neg),
        FilterRule::All { all } if !neg => and(all, neg),
        FilterRule::Any { any } if neg => and(any, neg),
        FilterRule::All { all } => or(all, neg),
        FilterRule::Any { any } => or(any, neg),
    }
}

fn and(parts: &[FilterRule], neg: bool) -> Vec<Vec<Leaf<'_>>> {
    let mut terms = vec![Vec::new()];
    for p in parts {
        let alts = dnf(p, neg);
        terms = terms
            .iter()
            .flat_map(|t| {
                alts.iter().map(move |a| {
                    let mut t: Vec<Leaf> = t.clone();
                    t.extend(a.iter().copied());
                    t
                })
            })
            .take(MAX_TERMS)
            .collect();
    }
    terms
}

fn or(parts: &[FilterRule], neg: bool) -> Vec<Vec<Leaf<'_>>> {
    parts
        .iter()
        .flat_map(|p| dnf(p, neg))
        .take(MAX_TERMS)
        .collect()
}

/// Allowed all-day dates for `due` or `start`: `[lower, upper)`, minus `outside`.
#[derive(Default)]
struct Dates {
    lower: Option<NaiveDate>,
    upper: Option<NaiveDate>,
    need: bool,
    none: bool,
    outside: Vec<(Option<NaiveDate>, Option<NaiveDate>)>,
}

impl Dates {
    fn within(&mut self, lo: Option<NaiveDate>, hi: Option<NaiveDate>) {
        self.need = true;
        self.lower = self.lower.max(lo);
        self.upper = match (self.upper, hi) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
    }

    /// The day to give the task, if any. `false` when the rule can't be met.
    fn pick(&self, today: NaiveDate) -> (Option<NaiveDate>, bool) {
        if !self.need {
            return (None, true);
        }
        if self.none {
            return (None, false);
        }
        let mut day = today.max(self.lower.unwrap_or(today));
        if let Some(hi) = self.upper
            && day >= hi
        {
            day = hi - Days::new(1);
        }
        let ok = self.lower.is_none_or(|lo| day >= lo)
            && !self
                .outside
                .iter()
                .any(|(lo, hi)| lo.is_none_or(|lo| day >= lo) && hi.is_none_or(|hi| day < hi));
        (Some(day), ok)
    }
}

/// What one alternative asks of a new task.
struct Seed {
    exact: bool,
    inbox: Option<bool>,
    list: Option<String>,
    list_none: bool,
    list_not: Vec<String>,
    tags: Vec<String>,
    forbidden: Vec<String>,
    pri_min: u8,
    pri_max: u8,
    pri_not: Vec<u8>,
    due: Dates,
    start: Dates,
}

impl Default for Seed {
    fn default() -> Self {
        Self {
            exact: true,
            inbox: None,
            list: None,
            list_none: false,
            list_not: Vec::new(),
            tags: Vec::new(),
            forbidden: Vec::new(),
            pri_min: 0,
            pri_max: 3,
            pri_not: Vec::new(),
            due: Dates::default(),
            start: Dates::default(),
        }
    }
}

/// Each query word must start some word of the title (as `text contains` does in FTS).
fn title_matches(title: &str, query: &str) -> bool {
    let words: Vec<String> = title
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect();
    query
        .split_whitespace()
        .all(|q| words.iter().any(|w| w.starts_with(&q.to_lowercase())))
}

impl Seed {
    fn require_tag(&mut self, conn: &Connection, names: &[&str]) -> Result<()> {
        for name in names {
            if let Some(tag) = find_tag(conn, name)? {
                if !self.tags.contains(&tag.id) {
                    self.tags.push(tag.id);
                }
                return Ok(());
            }
        }
        self.exact = false; // no such tag: nothing can match
        Ok(())
    }

    fn forbid_tags(&mut self, conn: &Connection, c: &Cond, names: &[&str]) -> Result<()> {
        for name in names {
            if let Some(tag) = find_tag(conn, name)? {
                if c.include_children.unwrap_or(true) {
                    self.forbidden.extend(tag_subtree(conn, &tag.id)?);
                } else {
                    self.forbidden.push(tag.id);
                }
            }
        }
        Ok(())
    }

    fn apply(
        &mut self,
        conn: &Connection,
        leaf: Leaf,
        title: &str,
        today: NaiveDate,
    ) -> Result<()> {
        let (c, pos) = (leaf.c, !leaf.neg);
        let op = c.op.as_str();
        match c.field.as_str() {
            "inbox" => {
                let want = as_bool(c)? == pos;
                if self.inbox.is_some_and(|b| b != want) {
                    self.exact = false;
                }
                self.inbox.get_or_insert(want);
            }
            "tag" => {
                let names = as_list(c)?;
                match (op, pos) {
                    ("has" | "has_any", true) | ("has_none", false) => {
                        self.require_tag(conn, &names)?
                    }
                    ("has_all", true) => {
                        for n in &names {
                            self.require_tag(conn, &[n])?;
                        }
                    }
                    _ => self.forbid_tags(conn, c, &names)?,
                }
            }
            "list" => {
                let named = match op {
                    "none" => None,
                    _ => find_list(conn, as_str(c, &c.value)?)?.map(|l| l.id),
                };
                match (op, pos) {
                    ("none", true) => self.list_none = true,
                    ("none", false) => self.exact = false, // some list, but which?
                    ("is", true) | ("is_not", false) => match named {
                        Some(id) if self.list.as_ref().is_none_or(|l| *l == id) => {
                            self.list = Some(id)
                        }
                        _ => self.exact = false,
                    },
                    _ => self.list_not.extend(named),
                }
            }
            "priority" => {
                let p = c
                    .value
                    .as_i64()
                    .filter(|p| (0..=3).contains(p))
                    .ok_or_else(|| bad(c, "value must be 0–3"))? as u8;
                match (op, pos) {
                    ("is", true) => {
                        self.pri_min = self.pri_min.max(p);
                        self.pri_max = self.pri_max.min(p);
                    }
                    ("is", false) => self.pri_not.push(p),
                    (">=", true) | ("<=", false) => {
                        let lo = if pos { p } else { p + 1 };
                        self.pri_min = self.pri_min.max(lo);
                    }
                    _ => match (pos, p.checked_sub(1)) {
                        (true, _) => self.pri_max = self.pri_max.min(p),
                        (false, Some(hi)) => self.pri_max = self.pri_max.min(hi),
                        (false, None) => self.exact = false, // not >= 0
                    },
                }
            }
            "due" | "start" => {
                let range = |v| day_range(as_str(c, v)?, today);
                let (lo, hi) = match op {
                    "none" => (None, None),
                    "overdue" => (None, Some(today)),
                    "on" => {
                        let r = range(&c.value)?;
                        (Some(r.start), Some(r.end))
                    }
                    "before" => (None, Some(range(&c.value)?.start)),
                    "on_or_before" => (None, Some(range(&c.value)?.end)),
                    "after" => (Some(range(&c.value)?.end), None),
                    "between" => {
                        let ends = c.value.as_array().map(Vec::as_slice);
                        let Some([from, to]) = ends else {
                            return Err(bad(c, "value must be [from, to]"));
                        };
                        (Some(range(from)?.start), Some(range(to)?.end))
                    }
                    _ => return Err(bad(c, "unknown date operator")),
                };
                let dates = if c.field == "due" {
                    &mut self.due
                } else {
                    &mut self.start
                };
                match (op, pos) {
                    ("none", true) => dates.none = true,
                    ("none", false) => dates.within(None, None),
                    (_, true) => dates.within(lo, hi),
                    (_, false) => dates.outside.push((lo, hi)),
                }
            }
            // New tasks get all-day dates and no repeat.
            "has_time" | "repeating" => {
                if as_bool(c)? == pos {
                    self.exact = false;
                }
            }
            "status" => {
                if (as_str(c, &c.value)? == "open") != pos {
                    self.exact = false;
                }
            }
            "text" => {
                if title_matches(title, as_str(c, &c.value)?) != pos {
                    self.exact = false;
                }
            }
            other => return Err(Error::Invalid(format!("filter: unknown field '{other}'"))),
        }
        Ok(())
    }

    fn finish(mut self, title: &str, today: NaiveDate) -> (NewTask, bool) {
        let list_id = if self.list_none {
            None
        } else {
            self.list.take()
        };
        if self.list_none && self.list.is_some()
            || list_id.as_ref().is_some_and(|l| self.list_not.contains(l))
        {
            self.exact = false;
        }
        let before = self.tags.len();
        self.tags.retain(|t| !self.forbidden.contains(t));
        if self.tags.len() != before {
            self.exact = false;
        }
        let priority = (self.pri_min..=self.pri_max)
            .find(|p| !self.pri_not.contains(p))
            .unwrap_or_else(|| {
                self.exact = false;
                0
            });
        let (due, due_ok) = self.due.pick(today);
        let (mut start, start_ok) = self.start.pick(today);
        if let (Some(s), Some(d)) = (start, due)
            && s > d
        {
            start = Some(d);
            self.exact = false;
        }
        self.exact &= due_ok && start_ok;
        let in_inbox =
            list_id.is_none() && self.tags.is_empty() && due.is_none() && start.is_none();
        if self.inbox.is_some_and(|b| b != in_inbox) {
            self.exact = false;
        }
        let new = NewTask {
            title: title.to_string(),
            list_id,
            priority,
            tags: self.tags,
            schedule: Schedule {
                start_at: start.map(floating_ms),
                due_at: due.map(floating_ms),
                all_day: start.or(due).is_some(),
                ..Default::default()
            },
            ..Default::default()
        };
        (new, self.exact)
    }
}

pub(super) fn new_task_matching(
    conn: &Connection,
    rule: &FilterRule,
    title: &str,
    now_ms: i64,
    tz: Tz,
) -> Result<NewTask> {
    let today = today(now_ms, tz);
    let mut first = None;
    for term in dnf(rule, false) {
        let mut seed = Seed::default();
        for leaf in term {
            seed.apply(conn, leaf, title, today)?;
        }
        let (new, exact) = seed.finish(title, today);
        if exact {
            return Ok(new);
        }
        first.get_or_insert(new);
    }
    Ok(first.unwrap_or_else(|| NewTask {
        title: title.to_string(),
        ..Default::default()
    }))
}

impl Store {
    /// A new task titled `title` that `rule` matches, as far as a fresh task can
    /// (quick add in a filter view). Nothing is written.
    pub fn new_task_matching(
        &self,
        rule: &FilterRule,
        title: &str,
        now_ms: i64,
        tz: Tz,
    ) -> Result<NewTask> {
        new_task_matching(&self.conn, rule, title, now_ms, tz)
    }
}

#[cfg(test)]
mod tests {
    use chrono::NaiveTime;

    use super::*;
    use crate::dates::local_ms;

    struct Fx {
        s: Store,
        tz: Tz,
        now: i64,
    }

    fn d(s: &str) -> i64 {
        floating_ms(s.parse().unwrap())
    }

    impl Fx {
        /// Wednesday 2026-11-25, 10:00 in Sofia.
        fn new() -> Self {
            let tz: Tz = "Europe/Sofia".parse().unwrap();
            let day = "2026-11-25".parse().unwrap();
            let now = local_ms(day, NaiveTime::from_hms_opt(10, 0, 0).unwrap(), tz);
            Self {
                s: Store::open_in_memory().unwrap(),
                tz,
                now,
            }
        }

        /// Quick-add `title` under `json`; returns the task and whether the rule shows it.
        fn add(&self, json: &str, title: &str) -> (crate::Task, bool) {
            let rule: FilterRule = serde_json::from_str(json).unwrap();
            let new = self
                .s
                .new_task_matching(&rule, title, self.now, self.tz)
                .unwrap();
            let t = self.s.create_task(&new).unwrap();
            let shown = self
                .s
                .run_rule(&rule, &[], false, self.now, self.tz)
                .unwrap()
                .iter()
                .any(|r| r.id == t.id);
            (t, shown)
        }

        fn tags(&self, id: &str) -> Vec<String> {
            self.s
                .task_tags(id)
                .unwrap()
                .into_iter()
                .map(|t| t.name)
                .collect()
        }
    }

    #[test]
    fn takes_tags_priority_and_a_due_date() {
        let f = Fx::new();
        f.s.ensure_tag_path("work").unwrap();
        let (t, shown) = f.add(
            r#"{"all": [
                {"field": "tag", "op": "has", "value": "work"},
                {"field": "priority", "op": ">=", "value": 2},
                {"field": "due", "op": "on_or_before", "value": "+7d"}
            ]}"#,
            "Write report",
        );
        assert!(shown);
        assert_eq!(f.tags(&t.id), ["work"]);
        assert_eq!(t.priority, 2);
        assert_eq!(t.due_at, Some(d("2026-11-25")));
        assert!(t.all_day);
    }

    #[test]
    fn takes_the_list() {
        let f = Fx::new();
        let list = f.s.create_list("Errands").unwrap();
        let (t, shown) = f.add(
            r#"{"field": "list", "op": "is", "value": "errands"}"#,
            "Milk",
        );
        assert!(shown);
        assert_eq!(t.list_id, Some(list));
    }

    #[test]
    fn picks_the_nearest_allowed_day() {
        let f = Fx::new();
        for (json, day) in [
            (
                r#"{"field": "due", "op": "after", "value": "tomorrow"}"#,
                "2026-11-27",
            ),
            (r#"{"field": "due", "op": "overdue"}"#, "2026-11-24"),
            (
                r#"{"field": "due", "op": "before", "value": "today"}"#,
                "2026-11-24",
            ),
            (
                r#"{"field": "due", "op": "between", "value": ["+3d", "+5d"]}"#,
                "2026-11-28",
            ),
            (
                r#"{"field": "due", "op": "on", "value": "next_week"}"#,
                "2026-11-30",
            ),
            (
                r#"{"field": "due", "op": "on", "value": "this_week"}"#,
                "2026-11-25",
            ),
            (r#"{"not": {"field": "due", "op": "none"}}"#, "2026-11-25"),
            (
                r#"{"field": "start", "op": "on", "value": "fri"}"#,
                "2026-11-27",
            ),
        ] {
            let (t, shown) = f.add(json, "x");
            assert!(shown, "{json}");
            assert_eq!(t.due_at.or(t.start_at), Some(d(day)), "{json}");
        }
    }

    #[test]
    fn an_inbox_branch_needs_nothing() {
        let f = Fx::new();
        let (t, shown) = f.add(
            r#"{"any": [
                {"field": "inbox", "op": "is", "value": true},
                {"field": "due", "op": "on", "value": "today"}
            ]}"#,
            "triage me",
        );
        assert!(shown);
        assert_eq!(t.due_at, None);
    }

    #[test]
    fn skips_branches_a_new_task_cannot_meet() {
        let f = Fx::new();
        f.s.ensure_tag_path("work").unwrap();
        let (t, shown) = f.add(
            r#"{"any": [
                {"field": "has_time", "op": "is", "value": true},
                {"field": "repeating", "op": "is", "value": true},
                {"field": "tag", "op": "has_any", "value": ["gone", "work"]}
            ]}"#,
            "x",
        );
        assert!(shown);
        assert_eq!(f.tags(&t.id), ["work"]);
    }

    #[test]
    fn negations_are_pushed_down() {
        let f = Fx::new();
        f.s.ensure_tag_path("home").unwrap();
        // not (home or urgent) = no home tag and priority below 2
        let (t, shown) = f.add(
            r#"{"all": [
                {"not": {"any": [
                    {"field": "tag", "op": "has", "value": "home"},
                    {"field": "priority", "op": ">=", "value": 2}
                ]}},
                {"not": {"field": "priority", "op": "is", "value": 0}}
            ]}"#,
            "x",
        );
        assert!(shown);
        assert_eq!(t.priority, 1);
        assert!(f.tags(&t.id).is_empty());
    }

    #[test]
    fn a_child_tag_rule_uses_the_named_tag() {
        let f = Fx::new();
        f.s.ensure_tag_path("home/garden").unwrap();
        let (t, shown) = f.add(r#"{"field": "tag", "op": "has", "value": "home"}"#, "x");
        assert!(shown);
        assert_eq!(f.tags(&t.id), ["home"]);
    }

    #[test]
    fn text_is_matched_against_the_title() {
        let f = Fx::new();
        f.s.ensure_tag_path("work").unwrap();
        let json = r#"{"all": [
            {"field": "tag", "op": "has", "value": "work"},
            {"field": "text", "op": "contains", "value": "meet"}
        ]}"#;
        let (_, shown) = f.add(json, "Team meeting");
        assert!(shown);
        // Can't be met, but the task still gets what it can.
        let (t, shown) = f.add(json, "Call Bob");
        assert!(!shown);
        assert_eq!(f.tags(&t.id), ["work"]);
    }

    #[test]
    fn rules_that_match_nothing_give_a_plain_task() {
        let f = Fx::new();
        let (t, shown) = f.add(r#"{"any": []}"#, "x");
        assert!(!shown);
        assert_eq!((t.list_id, t.due_at, t.priority), (None, None, 0));
    }
}
