use std::path::Path;
use std::time::Duration;

use rusqlite::{Connection, OptionalExtension, params};
use serde_json::Value;

use crate::hlc::Hlc;
use crate::oplog::{self, Applied, Entity, Op};
use crate::task::now_ms;
use crate::{Error, Result};

/// Schema migrations, applied in order. `PRAGMA user_version` holds how many ran.
const MIGRATIONS: &[&str] = &[
    include_str!("migrations/001_spike.sql"),
    include_str!("migrations/002_m1.sql"),
];

/// A handle to one Tick Alt database file.
///
/// Several handles may open the same file (the Android app and its widget do):
/// every write runs in an IMMEDIATE transaction that reads and saves the clock,
/// so they never hand out the same HLC.
pub struct Store {
    pub(crate) conn: Connection,
    device: u64,
}

/// Issues ops inside one write transaction.
pub(crate) struct Writer<'a> {
    pub(crate) conn: &'a Connection,
    clock: Hlc,
    now: i64,
}

impl Writer<'_> {
    /// Wall-clock time of this transaction, epoch ms.
    pub(crate) fn now(&self) -> i64 {
        self.now
    }

    /// Record and apply one op setting `entity.field = value`.
    pub(crate) fn set(
        &mut self,
        entity: Entity,
        id: &str,
        field: &str,
        value: impl Into<Value>,
    ) -> Result<()> {
        self.clock = self.clock.tick(self.now.max(0) as u64);
        let op = Op {
            hlc: self.clock,
            entity,
            entity_id: id.to_string(),
            field: field.to_string(),
            value: value.into(),
        };
        match oplog::apply(self.conn, &op)? {
            Applied::Set => Ok(()),
            other => Err(Error::Invalid(format!(
                "local op {}.{} = {} was {other:?}",
                entity.as_str(),
                field,
                op.value
            ))),
        }
    }
}

impl Store {
    /// Open (or create) the database at `path` and bring its schema up to date.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::init(Connection::open(path)?)
    }

    /// An in-memory database, for tests and spikes.
    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        // v2 has no foreign keys (ops arrive in any order); keep SQLite from enforcing v1's.
        conn.pragma_update(None, "foreign_keys", "OFF")?;
        conn.busy_timeout(Duration::from_secs(5))?;
        migrate(&conn)?;
        let device = device_id(&conn)?;
        Ok(Self { conn, device })
    }

    /// This database's device id, the last part of every HLC it issues.
    pub fn device_id(&self) -> u64 {
        self.device
    }

    /// SQLite's `data_version`: it changes when *another* connection commits to this
    /// database, never for this one's own writes. The daemon polls it to notice writes
    /// from processes that opened the file directly (e.g. `tt` with no daemon running).
    pub fn data_version(&self) -> Result<i64> {
        Ok(self
            .conn
            .query_row("PRAGMA data_version", [], |r| r.get(0))?)
    }

    /// Run `f` in an IMMEDIATE transaction with a clock for issuing ops.
    pub(crate) fn write<T>(&self, f: impl FnOnce(&mut Writer) -> Result<T>) -> Result<T> {
        self.conn.execute_batch("BEGIN IMMEDIATE")?;
        let result = (|| {
            let mut w = Writer {
                conn: &self.conn,
                clock: load_clock(&self.conn, self.device)?,
                now: now_ms(),
            };
            let out = f(&mut w)?;
            save_clock(&self.conn, w.clock)?;
            Ok(out)
        })();
        match result {
            Ok(v) => {
                self.conn.execute_batch("COMMIT")?;
                Ok(v)
            }
            Err(e) => {
                let _ = self.conn.execute_batch("ROLLBACK");
                Err(e)
            }
        }
    }

    /// Apply ops received from another device. Returns how many changed a field.
    pub fn apply_remote(&self, ops: &[Op]) -> Result<usize> {
        self.write(|w| {
            let mut set = 0;
            for op in ops {
                w.clock = w.clock.observe(op.hlc, w.now.max(0) as u64);
                if oplog::apply(w.conn, op)? == Applied::Set {
                    set += 1;
                }
            }
            Ok(set)
        })
    }

    /// Every op in the log, oldest first (sync, export and tests).
    pub fn ops(&self) -> Result<Vec<Op>> {
        oplog::read_ops(&self.conn, None)
    }

    /// A text dump of every synced table, rows sorted. Two devices that have seen
    /// the same ops produce the same dump (convergence and golden tests).
    pub fn state_dump(&self) -> Result<String> {
        const TABLES: &[&str] = &[
            "task",
            "list",
            "tag",
            "task_tag",
            "checklist_item",
            "reminder",
            "completion",
            "filter",
            "field_clock",
        ];
        let mut out = String::new();
        for table in TABLES {
            out.push_str(&format!("## {table}\n"));
            let mut rows: Vec<String> = query_rows(&self.conn, &format!("SELECT * FROM {table}"))?
                .into_iter()
                .map(|r| Value::Array(r).to_string())
                .collect();
            rows.sort();
            for r in rows {
                out.push_str(&r);
                out.push('\n');
            }
        }
        Ok(out)
    }
}

fn migrate(conn: &Connection) -> Result<()> {
    let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    let version = version.max(0) as usize;
    if version > MIGRATIONS.len() {
        return Err(Error::Corrupt(format!(
            "database schema v{version} is newer than this build (v{})",
            MIGRATIONS.len()
        )));
    }
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(version) {
        let to = i + 1;
        conn.execute_batch("BEGIN IMMEDIATE")?;
        let result = (|| {
            conn.execute_batch(sql)?;
            if to == 2 {
                backfill_ops(conn)?;
            }
            conn.pragma_update(None, "user_version", to as i64)?;
            Ok::<_, Error>(())
        })();
        match result {
            Ok(()) => conn.execute_batch("COMMIT")?,
            Err(e) => {
                let _ = conn.execute_batch("ROLLBACK");
                return Err(e);
            }
        }
    }
    Ok(())
}

fn device_id(conn: &Connection) -> Result<u64> {
    let stored: Option<String> = conn
        .query_row("SELECT value FROM meta WHERE key = 'device_id'", [], |r| {
            r.get(0)
        })
        .optional()?;
    if let Some(hex) = stored {
        return u64::from_str_radix(&hex, 16)
            .map_err(|_| Error::Corrupt(format!("device_id {hex}")));
    }
    // The random tail of a UUIDv7 (62 random bits). Collisions between one person's
    // devices are negligible.
    let id = uuid::Uuid::now_v7().as_u64_pair().1;
    conn.execute(
        "INSERT OR IGNORE INTO meta (key, value) VALUES ('device_id', ?1)",
        [format!("{id:016x}")],
    )?;
    // Another handle may have won the race; read back whatever is stored.
    let hex: String =
        conn.query_row("SELECT value FROM meta WHERE key = 'device_id'", [], |r| {
            r.get(0)
        })?;
    u64::from_str_radix(&hex, 16).map_err(|_| Error::Corrupt(format!("device_id {hex}")))
}

fn load_clock(conn: &Connection, device: u64) -> Result<Hlc> {
    let stored: Option<String> = conn
        .query_row("SELECT value FROM meta WHERE key = 'clock'", [], |r| {
            r.get(0)
        })
        .optional()?;
    let clock = match stored {
        Some(s) => s.parse().map_err(|e| Error::Corrupt(format!("{e}")))?,
        None => Hlc::zero(device),
    };
    Ok(Hlc { device, ..clock })
}

fn save_clock(conn: &Connection, clock: Hlc) -> Result<()> {
    conn.execute(
        "INSERT INTO meta (key, value) VALUES ('clock', ?1) \
         ON CONFLICT (key) DO UPDATE SET value = excluded.value",
        [clock.to_string()],
    )?;
    Ok(())
}

/// v1 databases have rows but no ops. Emit an op for every non-default field, so the
/// log fully describes the state and a future peer receives these rows too.
fn backfill_ops(conn: &Connection) -> Result<()> {
    let device = device_id(conn)?;
    let mut w = Writer {
        conn,
        clock: load_clock(conn, device)?,
        now: now_ms(),
    };

    let rows = query_rows(
        conn,
        "SELECT id, title, notes, list_id, parent_id, priority, start_at, due_at, status, \
                completed_at, sort_key, created_at FROM task",
    )?;
    let task_fields = [
        "title",
        "notes",
        "list_id",
        "parent_id",
        "priority",
        "start_at",
        "due_at",
        "status",
        "completed_at",
        "sort_key",
        "created_at",
    ];
    for row in rows {
        backfill_row(&mut w, Entity::Task, &row, &task_fields)?;
    }

    let rows = query_rows(
        conn,
        "SELECT id, name, color, icon, sort_key, archived FROM list",
    )?;
    for row in rows {
        backfill_row(
            &mut w,
            Entity::List,
            &row,
            &["name", "color", "icon", "sort_key", "archived"],
        )?;
    }

    let rows = query_rows(
        conn,
        "SELECT id, name, parent_id, color, icon, sort_key FROM tag",
    )?;
    for row in rows {
        backfill_row(
            &mut w,
            Entity::Tag,
            &row,
            &["name", "parent_id", "color", "icon", "sort_key"],
        )?;
    }

    let pairs = query_rows(conn, "SELECT task_id, tag_id FROM task_tag")?;
    for pair in pairs {
        let (Value::String(task), Value::String(tag)) = (&pair[0], &pair[1]) else {
            continue;
        };
        w.set(Entity::Task, task, &format!("tag:{tag}"), true)?;
    }
    save_clock(conn, w.clock)
}

/// Emit ops for the non-null, non-default columns of one row (`row[0]` is the id).
fn backfill_row(w: &mut Writer, entity: Entity, row: &[Value], fields: &[&str]) -> Result<()> {
    let Value::String(id) = &row[0] else {
        return Ok(());
    };
    for (field, value) in fields.iter().zip(&row[1..]) {
        let value = match (*field, value) {
            (_, Value::Null) => continue,
            ("archived", Value::Number(n)) => Value::Bool(n.as_i64() == Some(1)),
            (_, v) => v.clone(),
        };
        w.set(entity, id, field, value)?;
    }
    Ok(())
}

fn query_rows(conn: &Connection, sql: &str) -> Result<Vec<Vec<Value>>> {
    let mut stmt = conn.prepare(sql)?;
    let n = stmt.column_count();
    let rows = stmt.query_map(params![], |r| {
        (0..n)
            .map(|i| {
                Ok(match r.get_ref(i)? {
                    rusqlite::types::ValueRef::Null => Value::Null,
                    rusqlite::types::ValueRef::Integer(v) => Value::from(v),
                    rusqlite::types::ValueRef::Real(v) => Value::from(v),
                    rusqlite::types::ValueRef::Text(t) => {
                        Value::from(String::from_utf8_lossy(t).into_owned())
                    }
                    rusqlite::types::ValueRef::Blob(_) => Value::Null,
                })
            })
            .collect::<rusqlite::Result<Vec<_>>>()
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_are_idempotent_on_reopen() {
        let dir = std::env::temp_dir().join(format!("tt-core-mig-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("tt.db");
        let id = {
            let s = Store::open(&path).unwrap();
            s.add_task("persist").unwrap().id
        };
        let s = Store::open(&path).unwrap();
        assert_eq!(s.get_task(&id).unwrap().unwrap().title, "persist");
    }

    /// A database written by the M0 spike gains ops for every existing row.
    #[test]
    fn spike_database_is_migrated_and_backfilled() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(MIGRATIONS[0]).unwrap();
        conn.execute_batch(
            "INSERT INTO list (id, name) VALUES ('l1', 'Work');
             INSERT INTO tag (id, name) VALUES ('g1', 'home');
             INSERT INTO task (id, title, list_id, priority, created_at, updated_at)
                 VALUES ('t1', 'Old task', 'l1', 2, 1000, 1000);
             INSERT INTO task (id, title, created_at, updated_at) VALUES ('t2', 'Tagged', 2000, 2000);
             INSERT INTO task_tag (task_id, tag_id) VALUES ('t2', 'g1');
             PRAGMA user_version = 1;",
        )
        .unwrap();
        let s = Store::init(conn).unwrap();

        let t1 = s.get_task("t1").unwrap().unwrap();
        assert_eq!(
            (t1.title.as_str(), t1.priority, t1.list_id.as_deref()),
            ("Old task", 2, Some("l1"))
        );
        let ops = s.ops().unwrap();
        let has = |e: Entity, id: &str, f: &str| {
            ops.iter()
                .any(|o| o.entity == e && o.entity_id == id && o.field == f)
        };
        assert!(has(Entity::Task, "t1", "title"));
        assert!(has(Entity::Task, "t1", "list_id"));
        assert!(has(Entity::Task, "t2", "tag:g1"));
        assert!(has(Entity::List, "l1", "name"));
        assert!(has(Entity::Tag, "g1", "name"));
        assert!(
            s.inbox().unwrap().is_empty(),
            "t1 is in a list, t2 is tagged"
        );
    }

    #[test]
    fn duplicate_and_stale_ops_are_ignored() {
        let a = Store::open_in_memory().unwrap();
        let t = a.add_task("v1").unwrap();
        a.set_title(&t.id, "v2").unwrap();
        let ops = a.ops().unwrap();

        let b = Store::open_in_memory().unwrap();
        // Newest first: the older title op must not win.
        let mut reversed = ops.clone();
        reversed.reverse();
        b.apply_remote(&reversed).unwrap();
        assert_eq!(b.get_task(&t.id).unwrap().unwrap().title, "v2");
        assert_eq!(b.apply_remote(&ops).unwrap(), 0, "all duplicates");
    }

    #[test]
    fn invalid_remote_ops_are_logged_not_applied() {
        let s = Store::open_in_memory().unwrap();
        let t = s.add_task("x").unwrap();
        let bad = |field: &str, value: Value, counter| Op {
            hlc: Hlc {
                wall: u64::MAX >> 16,
                counter,
                device: 99,
            },
            entity: Entity::Task,
            entity_id: t.id.clone(),
            field: field.into(),
            value,
        };
        let ops = [
            bad("priority", Value::from(9), 0),
            bad("status", Value::from("exploded"), 1),
            bad("id; DROP TABLE task", Value::from("x"), 2),
        ];
        assert_eq!(s.apply_remote(&ops).unwrap(), 0);
        let t = s.get_task(&t.id).unwrap().unwrap();
        assert_eq!(t.priority, 0);
        assert_eq!(t.status, crate::Status::Open);
    }

    #[test]
    fn local_edit_after_remote_wins() {
        let a = Store::open_in_memory().unwrap();
        let t = a.add_task("t").unwrap();
        let b = Store::open_in_memory().unwrap();
        b.apply_remote(&a.ops().unwrap()).unwrap();

        // A's clock runs far ahead; B must still win with an edit made after seeing A's op.
        let mut ahead = a.ops().unwrap().pop().unwrap();
        ahead.hlc.wall += 3_600_000;
        ahead.hlc.counter += 1;
        ahead.field = "title".into();
        ahead.value = "from the future".into();
        b.apply_remote(&[ahead]).unwrap();
        b.set_title(&t.id, "later on B").unwrap();
        assert_eq!(b.get_task(&t.id).unwrap().unwrap().title, "later on B");
    }

    #[test]
    fn data_version_moves_only_for_other_connections() {
        let dir = std::env::temp_dir().join(format!("tt-dv-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("tt.db");
        let a = Store::open(&path).unwrap();
        let b = Store::open(&path).unwrap();
        let v0 = a.data_version().unwrap();
        a.add_task("mine").unwrap();
        assert_eq!(a.data_version().unwrap(), v0, "own writes don't count");
        b.add_task("theirs").unwrap();
        assert_ne!(a.data_version().unwrap(), v0, "another connection's commit does");
        drop((a, b));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
