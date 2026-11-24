use std::path::Path;

use rusqlite::Connection;

use crate::Result;

const SCHEMA: &str = include_str!("schema.sql");

/// A handle to one Tick Alt database file.
pub struct Store {
    pub(crate) conn: Connection,
}

impl Store {
    /// Open (or create) the database at `path` and apply the schema.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::init(Connection::open(path)?)
    }

    /// An in-memory database, for tests and spikes.
    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self { conn })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_applies_twice() {
        let store = Store::open_in_memory().unwrap();
        store.conn.execute_batch(SCHEMA).unwrap();
    }
}
