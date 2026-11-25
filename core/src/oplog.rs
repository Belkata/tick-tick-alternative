//! The op-log: every change is an op, and applying ops is the only way the
//! materialised tables change (plan/05 P2P Sync, "per-field LWW").
//!
//! An op sets one field of one entity. It is applied when its HLC is newer
//! than the HLC that last set that field; otherwise it is kept in the log and
//! ignored. That makes applying idempotent and order-independent, so devices
//! that have seen the same ops hold the same state.
//!
//! Task tags are a set: each membership is its own field, `tag:<tag id>`, with
//! a boolean value. Concurrent add/remove of the same tag resolves by HLC.

use rusqlite::types::Value as SqlValue;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::hlc::Hlc;
use crate::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Entity {
    Task,
    List,
    Tag,
    Checklist,
    Reminder,
    Completion,
    Filter,
}

impl Entity {
    pub fn as_str(self) -> &'static str {
        match self {
            Entity::Task => "task",
            Entity::List => "list",
            Entity::Tag => "tag",
            Entity::Checklist => "checklist",
            Entity::Reminder => "reminder",
            Entity::Completion => "completion",
            Entity::Filter => "filter",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "task" => Entity::Task,
            "list" => Entity::List,
            "tag" => Entity::Tag,
            "checklist" => Entity::Checklist,
            "reminder" => Entity::Reminder,
            "completion" => Entity::Completion,
            "filter" => Entity::Filter,
            _ => return None,
        })
    }

    fn table(self) -> &'static str {
        match self {
            Entity::Task => "task",
            Entity::List => "list",
            Entity::Tag => "tag",
            Entity::Checklist => "checklist_item",
            Entity::Reminder => "reminder",
            Entity::Completion => "completion",
            Entity::Filter => "filter",
        }
    }
}

/// One change to one field.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Op {
    pub hlc: Hlc,
    pub entity: Entity,
    pub entity_id: String,
    pub field: String,
    pub value: Value,
}

/// What applying an op did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Applied {
    /// The field now holds the op's value.
    Set,
    /// A newer op already set this field.
    Superseded,
    /// The op was already in the log.
    Duplicate,
    /// The field or value isn't valid; the op is logged but ignored.
    Rejected,
}

/// The type a field's JSON value must have.
#[derive(Debug, Clone, Copy)]
enum Kind {
    Text,
    OptText,
    /// An integer in a range.
    Int(i64, i64),
    OptInt,
    Bool,
    Enum(&'static [&'static str]),
    /// Any JSON, stored as text (filter rules).
    Json,
}

const STATUSES: &[&str] = &["open", "done", "deleted"];
const REPEAT_MODES: &[&str] = &["fixed", "after_completion"];
const REMINDER_KINDS: &[&str] = &["relative", "absolute"];

/// The column and type of each syncable field. Anything not listed is rejected,
/// which also keeps column names out of reach of remote input.
fn field_spec(entity: Entity, field: &str) -> Option<Kind> {
    use Kind::*;
    Some(match (entity, field) {
        (Entity::Task, "title" | "notes" | "sort_key") => Text,
        (Entity::Task, "list_id" | "parent_id" | "tz" | "repeat_rule" | "gcal_event_id") => OptText,
        (Entity::Task, "priority") => Int(0, 3),
        (Entity::Task, "start_at" | "due_at" | "duration_min" | "completed_at" | "deleted_at") => {
            OptInt
        }
        (Entity::Task, "all_day" | "in_calendar") => Bool,
        (Entity::Task, "repeat_mode") => Enum(REPEAT_MODES),
        (Entity::Task, "status") => Enum(STATUSES),
        (Entity::Task, "created_at") => Int(0, i64::MAX),

        (Entity::List, "name" | "sort_key") => Text,
        (Entity::List, "color" | "icon") => OptText,
        (Entity::List, "archived" | "deleted") => Bool,
        (Entity::List, "sort") => Json,
        (Entity::List, "created_at") => Int(0, i64::MAX),

        (Entity::Tag, "name" | "sort_key") => Text,
        (Entity::Tag, "parent_id" | "color" | "icon") => OptText,
        (Entity::Tag, "deleted") => Bool,
        (Entity::Tag, "sort") => Json,
        (Entity::Tag, "created_at") => Int(0, i64::MAX),

        (Entity::Checklist, "task_id" | "title" | "sort_key") => Text,
        (Entity::Checklist, "done" | "deleted") => Bool,

        (Entity::Reminder, "task_id") => Text,
        (Entity::Reminder, "kind") => Enum(REMINDER_KINDS),
        (Entity::Reminder, "offset_min" | "at") => OptInt,
        (Entity::Reminder, "deleted") => Bool,

        (Entity::Completion, "task_id") => Text,
        (Entity::Completion, "completed_at") => Int(0, i64::MAX),
        (Entity::Completion, "occurrence_at") => OptInt,

        (Entity::Filter, "name" | "sort_key") => Text,
        (Entity::Filter, "icon" | "grp") => OptText,
        (Entity::Filter, "rule" | "sort") => Json,
        (Entity::Filter, "show_completed" | "deleted") => Bool,
        _ => return None,
    })
}

/// Convert a JSON value to what goes in the column, or `None` if it doesn't fit.
fn to_sql(kind: Kind, v: &Value) -> Option<SqlValue> {
    Some(match (kind, v) {
        (Kind::Text, Value::String(s)) => SqlValue::Text(s.clone()),
        (Kind::OptText, Value::String(s)) => SqlValue::Text(s.clone()),
        (Kind::OptText | Kind::OptInt, Value::Null) => SqlValue::Null,
        (Kind::Int(lo, hi), Value::Number(n)) => {
            let i = n.as_i64()?;
            if i < lo || i > hi {
                return None;
            }
            SqlValue::Integer(i)
        }
        (Kind::OptInt, Value::Number(n)) => SqlValue::Integer(n.as_i64()?),
        (Kind::Bool, Value::Bool(b)) => SqlValue::Integer(*b as i64),
        (Kind::Enum(allowed), Value::String(s)) if allowed.contains(&s.as_str()) => {
            SqlValue::Text(s.clone())
        }
        (Kind::Json, v) => SqlValue::Text(v.to_string()),
        _ => return None,
    })
}

/// Apply one op inside the caller's transaction.
pub(crate) fn apply(conn: &Connection, op: &Op) -> Result<Applied> {
    let hlc = op.hlc.to_string();
    let inserted = conn.execute(
        "INSERT OR IGNORE INTO op (hlc, entity, entity_id, field, value) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![hlc, op.entity.as_str(), op.entity_id, op.field, op.value.to_string()],
    )?;
    if inserted == 0 {
        return Ok(Applied::Duplicate);
    }

    let current: Option<String> = conn
        .query_row(
            "SELECT hlc FROM field_clock WHERE entity = ?1 AND entity_id = ?2 AND field = ?3",
            params![op.entity.as_str(), op.entity_id, op.field],
            |r| r.get(0),
        )
        .optional()?;
    if current.is_some_and(|c| c >= hlc) {
        return Ok(Applied::Superseded);
    }

    if !write_field(conn, op)? {
        return Ok(Applied::Rejected);
    }
    conn.execute(
        "INSERT INTO field_clock (entity, entity_id, field, hlc) VALUES (?1, ?2, ?3, ?4) \
         ON CONFLICT (entity, entity_id, field) DO UPDATE SET hlc = excluded.hlc",
        params![op.entity.as_str(), op.entity_id, op.field, hlc],
    )?;
    Ok(Applied::Set)
}

/// Write the op's value into the materialised table. False if invalid.
fn write_field(conn: &Connection, op: &Op) -> Result<bool> {
    if op.entity_id.is_empty() {
        return Ok(false);
    }
    let table = op.entity.table();

    if op.entity == Entity::Task
        && let Some(tag_id) = op.field.strip_prefix("tag:")
    {
        let Value::Bool(member) = op.value else {
            return Ok(false);
        };
        if tag_id.is_empty() {
            return Ok(false);
        }
        ensure_row(conn, table, &op.entity_id)?;
        if member {
            conn.execute(
                "INSERT OR IGNORE INTO task_tag (task_id, tag_id) VALUES (?1, ?2)",
                params![op.entity_id, tag_id],
            )?;
        } else {
            conn.execute(
                "DELETE FROM task_tag WHERE task_id = ?1 AND tag_id = ?2",
                params![op.entity_id, tag_id],
            )?;
        }
        touch_task(conn, &op.entity_id, op.hlc)?;
        return Ok(true);
    }

    let Some(kind) = field_spec(op.entity, &op.field) else {
        return Ok(false);
    };
    let Some(value) = to_sql(kind, &op.value) else {
        return Ok(false);
    };
    ensure_row(conn, table, &op.entity_id)?;
    // `table` and `field` come from the whitelist above, never from the op as-is.
    conn.execute(
        &format!("UPDATE {table} SET {} = ?2 WHERE id = ?1", op.field),
        params![op.entity_id, value],
    )?;
    if op.entity == Entity::Task {
        touch_task(conn, &op.entity_id, op.hlc)?;
    }
    Ok(true)
}

fn ensure_row(conn: &Connection, table: &str, id: &str) -> Result<()> {
    conn.execute(
        &format!("INSERT OR IGNORE INTO {table} (id) VALUES (?1)"),
        [id],
    )?;
    Ok(())
}

fn touch_task(conn: &Connection, id: &str, hlc: Hlc) -> Result<()> {
    conn.execute(
        "UPDATE task SET updated_at = max(updated_at, ?2) WHERE id = ?1",
        params![id, hlc.wall as i64],
    )?;
    Ok(())
}

/// Apply again the logged ops for a field that a newer schema adds. A build that
/// didn't know the field logged them (so sync doesn't resend them) but rejected them.
pub(crate) fn replay_field(conn: &Connection, entity: Entity, field: &str) -> Result<()> {
    let ops: Vec<Op> = read_ops(conn, None)?
        .into_iter()
        .filter(|o| o.entity == entity && o.field == field)
        .collect();
    for op in ops {
        conn.execute("DELETE FROM op WHERE hlc = ?1", [op.hlc.to_string()])?;
        apply(conn, &op)?;
    }
    Ok(())
}

/// Read ops back from the log, oldest first. `after` limits it to newer stamps.
pub(crate) fn read_ops(conn: &Connection, after: Option<Hlc>) -> Result<Vec<Op>> {
    let after = after.map(|h| h.to_string()).unwrap_or_default();
    let mut stmt = conn.prepare(
        "SELECT hlc, entity, entity_id, field, value FROM op WHERE hlc > ?1 ORDER BY hlc",
    )?;
    let rows = stmt.query_map([after], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, String>(4)?,
        ))
    })?;
    let mut ops = Vec::new();
    for row in rows {
        let (hlc, entity, entity_id, field, value) = row?;
        ops.push(Op {
            hlc: hlc.parse().map_err(|e| Error::Corrupt(format!("{e}")))?,
            entity: Entity::parse(&entity)
                .ok_or_else(|| Error::Corrupt(format!("entity {entity}")))?,
            entity_id,
            field,
            value: serde_json::from_str(&value)?,
        });
    }
    Ok(ops)
}
