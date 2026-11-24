//! Saved filters: a JSON rule tree compiled to a SQL `WHERE` clause
//! (plan/04 Data Model, "Filters").
//!
//! ```json
//! {"any": [
//!   {"field": "inbox", "op": "is", "value": true},
//!   {"all": [
//!     {"field": "due", "op": "on_or_before", "value": "today"},
//!     {"not": {"field": "tag", "op": "has", "value": "weekend"}}
//!   ]}
//! ]}
//! ```
//!
//! Relative dates (`today`, `+7d`, `this_week`…) are resolved when the filter
//! runs, in the device's zone. All-day (floating) and timed dates are compared
//! on the calendar day they fall on. Unless the rule mentions `status`, only open
//! tasks match (plus done ones with `show_completed`). A rule naming a tag or
//! list that no longer exists matches as if that tag or list had no tasks.

use chrono::NaiveDate;
use chrono_tz::Tz;
use rusqlite::types::Value as SqlValue;
use rusqlite::{Connection, OptionalExtension, params_from_iter};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::dates::{DayRange, day_range, floating_ms, local_midnight_ms, today};
use crate::inbox::INBOX_WHERE;
use crate::oplog::Entity;
use crate::organize::{find_list, find_tag, tag_subtree};
use crate::task::{TASK_COLUMNS, Task, query_tasks};
use crate::{Error, Result, Store};

/// A node of the rule tree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FilterRule {
    All { all: Vec<FilterRule> },
    Any { any: Vec<FilterRule> },
    Not { not: Box<FilterRule> },
    Cond(Cond),
}

/// One condition, e.g. `{"field": "priority", "op": ">=", "value": 2}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cond {
    pub field: String,
    pub op: String,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub value: Value,
    /// For `tag`: also match tags nested under the named ones. Default true.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_children: Option<bool>,
}

/// A filter's definition: the rule plus how to show the results.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Filter {
    pub name: String,
    #[serde(default)]
    pub icon: Option<String>,
    pub rule: FilterRule,
    /// Sort keys, first wins: `due_asc`, `due_desc`, `start_asc`, `priority_desc`,
    /// `priority_asc`, `created_asc`, `created_desc`, `title_asc`, `manual`.
    #[serde(default)]
    pub sort: Vec<String>,
    /// Grouping for the UI (`priority`, `list`, `due`, `tag`); the core doesn't group.
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default)]
    pub show_completed: bool,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SavedFilter {
    pub id: String,
    pub filter: Filter,
}

struct Ctx<'a> {
    conn: &'a Connection,
    now_ms: i64,
    tz: Tz,
    today: NaiveDate,
    params: Vec<SqlValue>,
}

impl Ctx<'_> {
    fn bind(&mut self, v: impl Into<SqlValue>) -> String {
        self.params.push(v.into());
        format!("?{}", self.params.len())
    }

    fn bind_list(&mut self, ids: &[String]) -> String {
        if ids.is_empty() {
            return "(NULL)".into();
        }
        let ps: Vec<String> = ids.iter().map(|id| self.bind(id.clone())).collect();
        format!("({})", ps.join(", "))
    }

    /// `col` within `[lower, upper)` dates, whichever way the row stores its date.
    fn day_cmp(&mut self, col: &str, lower: Option<NaiveDate>, upper: Option<NaiveDate>) -> String {
        let mut floating = vec![format!("{col} IS NOT NULL")];
        let mut timed = floating.clone();
        if let Some(d) = lower {
            let (f, l) = (
                self.bind(floating_ms(d)),
                self.bind(local_midnight_ms(d, self.tz)),
            );
            floating.push(format!("{col} >= {f}"));
            timed.push(format!("{col} >= {l}"));
        }
        if let Some(d) = upper {
            let (f, l) = (
                self.bind(floating_ms(d)),
                self.bind(local_midnight_ms(d, self.tz)),
            );
            floating.push(format!("{col} < {f}"));
            timed.push(format!("{col} < {l}"));
        }
        format!(
            "((task.all_day = 1 AND {}) OR (task.all_day = 0 AND {}))",
            floating.join(" AND "),
            timed.join(" AND ")
        )
    }
}

fn bad(c: &Cond, why: &str) -> Error {
    Error::Invalid(format!("filter: {} {}: {why}", c.field, c.op))
}

fn as_str<'v>(c: &Cond, v: &'v Value) -> Result<&'v str> {
    v.as_str().ok_or_else(|| bad(c, "value must be a string"))
}

fn as_bool(c: &Cond) -> Result<bool> {
    c.value
        .as_bool()
        .ok_or_else(|| bad(c, "value must be true or false"))
}

fn as_list(c: &Cond) -> Result<Vec<&str>> {
    match &c.value {
        Value::String(s) => Ok(vec![s.as_str()]),
        Value::Array(items) => items.iter().map(|v| as_str(c, v)).collect(),
        _ => Err(bad(c, "value must be a string or a list of strings")),
    }
}

fn mentions_status(rule: &FilterRule) -> bool {
    match rule {
        FilterRule::All { all } => all.iter().any(mentions_status),
        FilterRule::Any { any } => any.iter().any(mentions_status),
        FilterRule::Not { not } => mentions_status(not),
        FilterRule::Cond(c) => c.field == "status",
    }
}

fn compile(rule: &FilterRule, cx: &mut Ctx) -> Result<String> {
    Ok(match rule {
        FilterRule::All { all } if all.is_empty() => "1".into(),
        FilterRule::Any { any } if any.is_empty() => "0".into(),
        FilterRule::All { all } => {
            let parts = all
                .iter()
                .map(|r| compile(r, cx))
                .collect::<Result<Vec<_>>>()?;
            format!("({})", parts.join(" AND "))
        }
        FilterRule::Any { any } => {
            let parts = any
                .iter()
                .map(|r| compile(r, cx))
                .collect::<Result<Vec<_>>>()?;
            format!("({})", parts.join(" OR "))
        }
        FilterRule::Not { not } => format!("(NOT {})", compile(not, cx)?),
        FilterRule::Cond(c) => compile_cond(c, cx)?,
    })
}

fn tag_ids(c: &Cond, cx: &Ctx, names: &[&str]) -> Result<Vec<String>> {
    let children = c.include_children.unwrap_or(true);
    let mut ids = Vec::new();
    for name in names {
        if let Some(tag) = find_tag(cx.conn, name)? {
            if children {
                ids.extend(tag_subtree(cx.conn, &tag.id)?);
            } else {
                ids.push(tag.id);
            }
        }
    }
    Ok(ids)
}

fn has_tag(cx: &mut Ctx, ids: &[String]) -> String {
    let list = cx.bind_list(ids);
    format!("EXISTS (SELECT 1 FROM task_tag tt WHERE tt.task_id = task.id AND tt.tag_id IN {list})")
}

fn range(c: &Cond, cx: &Ctx, v: &Value) -> Result<DayRange> {
    day_range(as_str(c, v)?, cx.today)
}

fn compile_cond(c: &Cond, cx: &mut Ctx) -> Result<String> {
    let op = c.op.as_str();
    Ok(match c.field.as_str() {
        "inbox" => {
            let yes = as_bool(c)?;
            if op != "is" {
                return Err(bad(c, "use 'is'"));
            }
            if yes {
                format!("({INBOX_WHERE})")
            } else {
                format!("(NOT ({INBOX_WHERE}))")
            }
        }
        "tag" => {
            let names = as_list(c)?;
            match op {
                "has" if names.len() == 1 => {
                    let ids = tag_ids(c, cx, &names)?;
                    has_tag(cx, &ids)
                }
                "has_any" => {
                    let ids = tag_ids(c, cx, &names)?;
                    has_tag(cx, &ids)
                }
                "has_none" => {
                    let ids = tag_ids(c, cx, &names)?;
                    format!("(NOT {})", has_tag(cx, &ids))
                }
                "has_all" => {
                    let mut parts = Vec::new();
                    for n in &names {
                        let ids = tag_ids(c, cx, &[n])?;
                        parts.push(has_tag(cx, &ids));
                    }
                    if parts.is_empty() {
                        "1".into()
                    } else {
                        format!("({})", parts.join(" AND "))
                    }
                }
                "has" => return Err(bad(c, "'has' takes one tag; use has_any or has_all")),
                _ => return Err(bad(c, "use has, has_any, has_all or has_none")),
            }
        }
        "list" => match op {
            "none" => "task.list_id IS NULL".into(),
            "is" | "is_not" => {
                let id = find_list(cx.conn, as_str(c, &c.value)?)?.map(|l| l.id);
                match (op, id) {
                    ("is", Some(id)) => format!("task.list_id = {}", cx.bind(id)),
                    ("is", None) => "0".into(),
                    (_, Some(id)) => {
                        format!("(task.list_id IS NULL OR task.list_id != {})", cx.bind(id))
                    }
                    (_, None) => "1".into(),
                }
            }
            _ => return Err(bad(c, "use is, is_not or none")),
        },
        "priority" => {
            let p = c
                .value
                .as_i64()
                .filter(|p| (0..=3).contains(p))
                .ok_or_else(|| bad(c, "value must be 0–3"))?;
            let sql_op = match op {
                "is" => "=",
                ">=" => ">=",
                "<=" => "<=",
                _ => return Err(bad(c, "use is, >= or <=")),
            };
            format!("task.priority {sql_op} {}", cx.bind(p))
        }
        "due" | "start" => {
            let col = if c.field == "due" {
                "task.due_at"
            } else {
                "task.start_at"
            };
            match op {
                "none" => format!("{col} IS NULL"),
                "on" => {
                    let r = range(c, cx, &c.value)?;
                    cx.day_cmp(col, Some(r.start), Some(r.end))
                }
                "before" => {
                    let r = range(c, cx, &c.value)?;
                    cx.day_cmp(col, None, Some(r.start))
                }
                "on_or_before" => {
                    let r = range(c, cx, &c.value)?;
                    cx.day_cmp(col, None, Some(r.end))
                }
                "after" => {
                    let r = range(c, cx, &c.value)?;
                    cx.day_cmp(col, Some(r.end), None)
                }
                "between" => {
                    let Value::Array(ends) = &c.value else {
                        return Err(bad(c, "value must be [from, to]"));
                    };
                    let [from, to] = ends.as_slice() else {
                        return Err(bad(c, "value must be [from, to]"));
                    };
                    let (from, to) = (range(c, cx, from)?, range(c, cx, to)?);
                    cx.day_cmp(col, Some(from.start), Some(to.end))
                }
                "overdue" => {
                    // Timed: the moment has passed. All-day: the date is before today.
                    let now = cx.bind(cx.now_ms);
                    let today = cx.bind(floating_ms(cx.today));
                    format!(
                        "({col} IS NOT NULL AND ((task.all_day = 0 AND {col} < {now}) \
                         OR (task.all_day = 1 AND {col} < {today})))"
                    )
                }
                _ => {
                    return Err(bad(
                        c,
                        "use on, before, after, on_or_before, between, none or overdue",
                    ));
                }
            }
        }
        "has_time" => {
            if op != "is" {
                return Err(bad(c, "use 'is'"));
            }
            let timed = "(coalesce(task.start_at, task.due_at) IS NOT NULL AND task.all_day = 0)";
            if as_bool(c)? {
                timed.into()
            } else {
                format!("(NOT {timed})")
            }
        }
        "repeating" => {
            if op != "is" {
                return Err(bad(c, "use 'is'"));
            }
            if as_bool(c)? {
                "task.repeat_rule IS NOT NULL".into()
            } else {
                "task.repeat_rule IS NULL".into()
            }
        }
        "status" => {
            let s = as_str(c, &c.value)?;
            if op != "is" || !["open", "done", "deleted"].contains(&s) {
                return Err(bad(c, "use is open, done or deleted"));
            }
            format!("task.status = {}", cx.bind(s.to_string()))
        }
        "text" => {
            if op != "contains" {
                return Err(bad(c, "use 'contains'"));
            }
            // Each word must appear as a prefix: `buy mil` matches "Buy milk".
            let query: Vec<String> = as_str(c, &c.value)?
                .split_whitespace()
                .map(|w| format!("\"{}\"*", w.replace('"', "\"\"")))
                .collect();
            if query.is_empty() {
                "1".into()
            } else {
                let q = cx.bind(query.join(" "));
                format!("task.rowid IN (SELECT rowid FROM task_fts WHERE task_fts MATCH {q})")
            }
        }
        other => return Err(Error::Invalid(format!("filter: unknown field '{other}'"))),
    })
}

fn order_by(sort: &[String]) -> Result<String> {
    let mut parts = Vec::new();
    let keys: Vec<&str> = if sort.is_empty() {
        vec!["due_asc", "priority_desc"]
    } else {
        sort.iter().map(String::as_str).collect()
    };
    for key in keys {
        parts.push(match key {
            "due_asc" => "task.due_at IS NULL, task.due_at",
            "due_desc" => "task.due_at IS NULL, task.due_at DESC",
            "start_asc" => "task.start_at IS NULL, task.start_at",
            "priority_desc" => "task.priority DESC",
            "priority_asc" => "task.priority",
            "created_asc" => "task.created_at",
            "created_desc" => "task.created_at DESC",
            "title_asc" => "lower(task.title)",
            "manual" => "task.sort_key",
            other => return Err(Error::Invalid(format!("filter: unknown sort '{other}'"))),
        });
    }
    parts.push("task.created_at, task.id");
    Ok(parts.join(", "))
}

pub(crate) fn run(
    conn: &Connection,
    rule: &FilterRule,
    sort: &[String],
    show_completed: bool,
    now_ms: i64,
    tz: Tz,
) -> Result<Vec<Task>> {
    let mut cx = Ctx {
        conn,
        now_ms,
        tz,
        today: today(now_ms, tz),
        params: Vec::new(),
    };
    let mut cond = compile(rule, &mut cx)?;
    if !mentions_status(rule) {
        let statuses = if show_completed {
            "('open', 'done')"
        } else {
            "('open')"
        };
        cond = format!("{cond} AND task.status IN {statuses}");
    }
    let sql = format!(
        "SELECT {TASK_COLUMNS} FROM task WHERE {cond} ORDER BY {}",
        order_by(sort)?
    );
    query_tasks(conn, &sql, params_from_iter(cx.params))
}

fn load_filter(conn: &Connection, id: &str) -> Result<Option<SavedFilter>> {
    conn.query_row(
        "SELECT id, name, icon, rule, sort, grp, show_completed FROM filter WHERE id = ?1 AND deleted = 0",
        [id],
        row_to_filter,
    )
    .optional()?
    .transpose()
}

type FilterRow = Result<SavedFilter>;

fn row_to_filter(r: &rusqlite::Row<'_>) -> rusqlite::Result<FilterRow> {
    let (id, name, icon): (String, String, Option<String>) = (r.get(0)?, r.get(1)?, r.get(2)?);
    let (rule, sort, group, show_completed): (String, String, Option<String>, bool) =
        (r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?);
    Ok((|| {
        Ok(SavedFilter {
            id,
            filter: Filter {
                name,
                icon,
                rule: serde_json::from_str(&rule)?,
                sort: serde_json::from_str(&sort)?,
                group,
                show_completed,
            },
        })
    })())
}

impl Store {
    /// Run a rule without saving it (the filter builder's live preview).
    pub fn run_rule(
        &self,
        rule: &FilterRule,
        sort: &[String],
        show_completed: bool,
        now_ms: i64,
        tz: Tz,
    ) -> Result<Vec<Task>> {
        run(&self.conn, rule, sort, show_completed, now_ms, tz)
    }

    /// Save a new filter. The rule is checked by compiling it once. Returns its id.
    pub fn save_filter(&self, f: &Filter) -> Result<String> {
        let id = Uuid::now_v7().to_string();
        self.write_filter(&id, f)?;
        Ok(id)
    }

    pub fn update_filter(&self, id: &str, f: &Filter) -> Result<()> {
        if load_filter(&self.conn, id)?.is_none() {
            return Err(Error::NotFound(format!("filter {id}")));
        }
        self.write_filter(id, f)
    }

    fn write_filter(&self, id: &str, f: &Filter) -> Result<()> {
        let name = f.name.trim();
        if name.is_empty() {
            return Err(Error::Invalid("a filter needs a name".into()));
        }
        run(&self.conn, &f.rule, &f.sort, f.show_completed, 0, Tz::UTC)?; // validate
        self.write(|w| {
            w.set(Entity::Filter, id, "name", name)?;
            w.set(Entity::Filter, id, "icon", f.icon.clone())?;
            w.set(Entity::Filter, id, "rule", serde_json::to_value(&f.rule)?)?;
            w.set(Entity::Filter, id, "sort", serde_json::to_value(&f.sort)?)?;
            w.set(Entity::Filter, id, "grp", f.group.clone())?;
            w.set(Entity::Filter, id, "show_completed", f.show_completed)
        })
    }

    pub fn delete_filter(&self, id: &str) -> Result<()> {
        if load_filter(&self.conn, id)?.is_none() {
            return Err(Error::NotFound(format!("filter {id}")));
        }
        self.write(|w| w.set(Entity::Filter, id, "deleted", true))
    }

    /// Saved filters, by sidebar order then name. A fresh database has none (plan/11 question 3).
    pub fn filters(&self) -> Result<Vec<SavedFilter>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, icon, rule, sort, grp, show_completed FROM filter \
             WHERE deleted = 0 ORDER BY sort_key, lower(name), id",
        )?;
        let rows = stmt.query_map([], row_to_filter)?;
        rows.map(|r| r?).collect()
    }

    /// A saved filter by id or name (case-insensitive).
    pub fn find_filter(&self, id_or_name: &str) -> Result<Option<SavedFilter>> {
        Ok(self
            .filters()?
            .into_iter()
            .find(|f| f.id == id_or_name || f.filter.name.eq_ignore_ascii_case(id_or_name)))
    }

    /// Run a saved filter now, in the device's zone `tz`.
    pub fn run_filter(&self, id: &str, now_ms: i64, tz: Tz) -> Result<Vec<Task>> {
        let f =
            load_filter(&self.conn, id)?.ok_or_else(|| Error::NotFound(format!("filter {id}")))?;
        let f = f.filter;
        run(&self.conn, &f.rule, &f.sort, f.show_completed, now_ms, tz)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dates::local_ms;
    use crate::{NewTask, Repeat, RepeatMode, Schedule};
    use chrono::NaiveTime;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn rule(json: &str) -> FilterRule {
        serde_json::from_str(json).unwrap()
    }

    struct Fx {
        s: Store,
        tz: Tz,
        now: i64,
    }

    impl Fx {
        /// Wednesday 2026-11-25, 10:00 in Sofia.
        fn new() -> Self {
            let tz: Tz = "Europe/Sofia".parse().unwrap();
            let now = local_ms(
                d("2026-11-25"),
                NaiveTime::from_hms_opt(10, 0, 0).unwrap(),
                tz,
            );
            Self {
                s: Store::open_in_memory().unwrap(),
                tz,
                now,
            }
        }

        fn all_day(&self, title: &str, day: &str) -> String {
            self.s
                .create_task(&NewTask {
                    title: title.into(),
                    schedule: Schedule {
                        due_at: Some(floating_ms(d(day))),
                        all_day: true,
                        ..Default::default()
                    },
                    ..Default::default()
                })
                .unwrap()
                .id
        }

        fn timed(&self, title: &str, day: &str, hm: (u32, u32)) -> String {
            let at = local_ms(
                d(day),
                NaiveTime::from_hms_opt(hm.0, hm.1, 0).unwrap(),
                self.tz,
            );
            self.s
                .create_task(&NewTask {
                    title: title.into(),
                    schedule: Schedule {
                        due_at: Some(at),
                        tz: Some("Europe/Sofia".into()),
                        ..Default::default()
                    },
                    ..Default::default()
                })
                .unwrap()
                .id
        }

        fn titles(&self, json: &str) -> Vec<String> {
            let mut t: Vec<String> = self
                .s
                .run_rule(&rule(json), &[], false, self.now, self.tz)
                .unwrap()
                .into_iter()
                .map(|t| t.title)
                .collect();
            t.sort();
            t
        }
    }

    #[test]
    fn due_tokens_match_floating_and_timed_by_local_day() {
        let f = Fx::new();
        f.all_day("all-day today", "2026-11-25");
        f.timed("late tonight", "2026-11-25", (23, 30)); // 21:30 UTC, still the 25th locally
        f.timed("just after midnight", "2026-11-26", (0, 15)); // 22:15 UTC on the 25th!
        f.all_day("next week", "2026-12-01");
        f.all_day("last week", "2026-11-18");
        f.s.add_task("undated").unwrap();

        assert_eq!(
            f.titles(r#"{"field":"due","op":"on","value":"today"}"#),
            ["all-day today", "late tonight"]
        );
        assert_eq!(
            f.titles(r#"{"field":"due","op":"on","value":"tomorrow"}"#),
            ["just after midnight"]
        );
        assert_eq!(
            f.titles(r#"{"field":"due","op":"before","value":"today"}"#),
            ["last week"]
        );
        assert_eq!(
            f.titles(r#"{"field":"due","op":"on_or_before","value":"+7d"}"#),
            [
                "all-day today",
                "just after midnight",
                "last week",
                "late tonight",
                "next week"
            ]
        );
        assert_eq!(
            f.titles(r#"{"field":"due","op":"after","value":"this_week"}"#),
            ["next week"]
        );
        assert_eq!(
            f.titles(r#"{"field":"due","op":"between","value":["tomorrow","2026-12-01"]}"#),
            ["just after midnight", "next week"]
        );
        assert_eq!(f.titles(r#"{"field":"due","op":"none"}"#), ["undated"]);
        assert_eq!(f.titles(r#"{"field":"due","op":"overdue"}"#), ["last week"]);
    }

    #[test]
    fn overdue_timed_task_today() {
        let f = Fx::new();
        f.timed("this morning", "2026-11-25", (8, 0));
        f.timed("this evening", "2026-11-25", (18, 0));
        assert_eq!(
            f.titles(r#"{"field":"due","op":"overdue"}"#),
            ["this morning"]
        );
    }

    #[test]
    fn tags_include_children_unless_exact() {
        let f = Fx::new();
        let home = f.s.ensure_tag_path("home").unwrap();
        let garden = f.s.ensure_tag_path("home/garden").unwrap();
        let work = f.s.ensure_tag_path("work").unwrap();
        let a = f.s.add_task("home task").unwrap();
        let b = f.s.add_task("garden task").unwrap();
        let c = f.s.add_task("work task").unwrap();
        let both = f.s.add_task("both").unwrap();
        f.s.tag_task(&a.id, &home).unwrap();
        f.s.tag_task(&b.id, &garden).unwrap();
        f.s.tag_task(&c.id, &work).unwrap();
        f.s.tag_task(&both.id, &garden).unwrap();
        f.s.tag_task(&both.id, &work).unwrap();
        f.s.add_task("untagged").unwrap();

        assert_eq!(
            f.titles(r#"{"field":"tag","op":"has","value":"home"}"#),
            ["both", "garden task", "home task"]
        );
        assert_eq!(
            f.titles(r#"{"field":"tag","op":"has","value":"home","include_children":false}"#),
            ["home task"]
        );
        assert_eq!(
            f.titles(r#"{"field":"tag","op":"has_all","value":["home","work"]}"#),
            ["both"]
        );
        assert_eq!(
            f.titles(r#"{"field":"tag","op":"has_any","value":["home/garden","work"]}"#)
                .len(),
            3
        );
        assert_eq!(
            f.titles(r#"{"field":"tag","op":"has_none","value":["home"]}"#),
            ["untagged", "work task"]
        );
        assert!(
            f.titles(r#"{"field":"tag","op":"has","value":"no-such-tag"}"#)
                .is_empty()
        );
    }

    #[test]
    fn plan_example_today_plus_inbox() {
        let f = Fx::new();
        let weekend = f.s.ensure_tag_path("weekend").unwrap();
        f.s.add_task("triage me").unwrap();
        f.all_day("due today", "2026-11-25");
        let w = f.all_day("weekend chore", "2026-11-24");
        f.s.tag_task(&w, &weekend).unwrap();
        f.all_day("later", "2026-12-24");
        let json = r#"{"any": [
            {"field": "inbox", "op": "is", "value": true},
            {"all": [
              {"field": "due", "op": "on_or_before", "value": "today"},
              {"not": {"field": "tag", "op": "has", "value": "weekend", "include_children": true}}
            ]}
        ]}"#;
        assert_eq!(f.titles(json), ["due today", "triage me"]);
    }

    #[test]
    fn list_priority_text_repeating_status() {
        let f = Fx::new();
        let work = f.s.create_list("Work").unwrap();
        let a = f.s.add_task("Write quarterly report").unwrap();
        f.s.set_task_list(&a.id, Some(&work)).unwrap();
        f.s.set_priority(&a.id, 3).unwrap();
        let b = f.s.add_task("Buy milk").unwrap();
        f.s.set_priority(&b.id, 1).unwrap();
        let r = f.all_day("Water plants", "2026-11-25");
        f.s.set_repeat(
            &r,
            Some(&Repeat {
                rule: "FREQ=DAILY".into(),
                mode: RepeatMode::Fixed,
            }),
        )
        .unwrap();
        let done = f.s.add_task("Old thing").unwrap();
        f.s.complete_task(&done.id).unwrap();

        assert_eq!(
            f.titles(r#"{"field":"list","op":"is","value":"work"}"#),
            ["Write quarterly report"]
        );
        assert_eq!(
            f.titles(r#"{"field":"list","op":"is_not","value":"Work"}"#)
                .len(),
            2
        );
        assert_eq!(f.titles(r#"{"field":"list","op":"none"}"#).len(), 2);
        assert_eq!(
            f.titles(r#"{"field":"priority","op":">=","value":1}"#)
                .len(),
            2
        );
        assert_eq!(
            f.titles(r#"{"field":"text","op":"contains","value":"quart rep"}"#),
            ["Write quarterly report"]
        );
        assert_eq!(
            f.titles(r#"{"field":"repeating","op":"is","value":true}"#),
            ["Water plants"]
        );
        assert_eq!(
            f.titles(r#"{"field":"status","op":"is","value":"done"}"#),
            ["Old thing"]
        );
        assert_eq!(
            f.titles(r#"{"field":"has_time","op":"is","value":false}"#)
                .len(),
            3
        );
        assert_eq!(f.titles(r#"{"all":[]}"#).len(), 3, "open only by default");
        let with_done =
            f.s.run_rule(&rule(r#"{"all":[]}"#), &[], true, f.now, f.tz)
                .unwrap();
        assert_eq!(with_done.len(), 4);
    }

    #[test]
    fn text_search_follows_edits() {
        let f = Fx::new();
        let t = f.s.add_task("Renew passport").unwrap();
        f.s.set_title(&t.id, "Renew driving licence").unwrap();
        assert!(
            f.titles(r#"{"field":"text","op":"contains","value":"passport"}"#)
                .is_empty()
        );
        assert_eq!(
            f.titles(r#"{"field":"text","op":"contains","value":"licence"}"#)
                .len(),
            1
        );
        f.s.set_notes(&t.id, "bring \"old\" photo").unwrap();
        assert_eq!(
            f.titles(r#"{"field":"text","op":"contains","value":"\"old"}"#)
                .len(),
            1
        );
    }

    #[test]
    fn sorting() {
        let f = Fx::new();
        f.all_day("c later", "2026-12-01");
        f.all_day("a sooner", "2026-11-26");
        let p = f.s.add_task("b undated urgent").unwrap();
        f.s.set_priority(&p.id, 3).unwrap();
        let by = |sort: &[&str]| -> Vec<String> {
            let sort: Vec<String> = sort.iter().map(|s| s.to_string()).collect();
            f.s.run_rule(&rule(r#"{"all":[]}"#), &sort, false, f.now, f.tz)
                .unwrap()
                .into_iter()
                .map(|t| t.title)
                .collect()
        };
        assert_eq!(
            by(&[]),
            ["a sooner", "c later", "b undated urgent"],
            "due first, undated last"
        );
        assert_eq!(by(&["priority_desc"])[0], "b undated urgent");
        assert_eq!(
            by(&["title_asc"]),
            ["a sooner", "b undated urgent", "c later"]
        );
        assert!(
            f.s.run_rule(
                &rule(r#"{"all":[]}"#),
                &["sideways".into()],
                false,
                f.now,
                f.tz
            )
            .is_err()
        );
    }

    #[test]
    fn saved_filters_round_trip() {
        let f = Fx::new();
        assert!(
            f.s.filters().unwrap().is_empty(),
            "a fresh database has no filters"
        );
        f.all_day("today", "2026-11-25");
        let def = Filter {
            name: "Today".into(),
            icon: Some("☀️".into()),
            rule: rule(r#"{"field":"due","op":"on_or_before","value":"today"}"#),
            sort: vec!["priority_desc".into()],
            group: Some("priority".into()),
            show_completed: false,
        };
        let id = f.s.save_filter(&def).unwrap();
        let saved = f.s.find_filter("today").unwrap().unwrap();
        assert_eq!((saved.id.as_str(), &saved.filter), (id.as_str(), &def));
        assert_eq!(f.s.run_filter(&id, f.now, f.tz).unwrap().len(), 1);

        let bad = Filter {
            rule: rule(r#"{"field":"colour","op":"is","value":"red"}"#),
            ..def.clone()
        };
        assert!(f.s.save_filter(&bad).is_err());
        f.s.delete_filter(&id).unwrap();
        assert!(f.s.filters().unwrap().is_empty());
    }

    #[test]
    fn malformed_conditions_are_errors() {
        let f = Fx::new();
        for json in [
            r#"{"field":"priority","op":">=","value":7}"#,
            r#"{"field":"due","op":"on","value":"someday"}"#,
            r#"{"field":"due","op":"between","value":"today"}"#,
            r#"{"field":"tag","op":"has","value":["a","b"]}"#,
            r#"{"field":"inbox","op":"is","value":"yes"}"#,
            r#"{"field":"status","op":"is","value":"wont_do"}"#,
        ] {
            assert!(
                f.s.run_rule(&rule(json), &[], false, f.now, f.tz).is_err(),
                "{json}"
            );
        }
    }
}
