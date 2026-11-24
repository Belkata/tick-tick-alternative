//! Replication support for M3 sync (plan/05): version vectors, the ops a peer is
//! missing, and the local list of trusted peers.

use std::collections::BTreeMap;

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::hlc::Hlc;
use crate::oplog::{self, Op};
use crate::{Error, Result, Store};

/// The newest op seen from each device (by device id). Two stores with equal vectors
/// have seen the same ops.
pub type VersionVector = BTreeMap<u64, Hlc>;

/// A device this one has paired with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Peer {
    /// The iroh endpoint id.
    pub node_id: String,
    pub name: String,
    pub added_at: i64,
    pub last_seen: Option<i64>,
    /// Its `EndpointAddr` as JSON, from the last time it connected.
    pub last_addr: Option<String>,
}

/// HLC text is `wall-counter-device` in fixed-width hex: the device is the last 16 chars.
const DEVICE_OFFSET: usize = 12 + 1 + 6 + 1;

impl Store {
    /// The newest op per device in this store's log.
    pub fn version_vector(&self) -> Result<VersionVector> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT MAX(hlc) FROM op GROUP BY substr(hlc, {})",
            DEVICE_OFFSET + 1
        ))?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        let mut vv = VersionVector::new();
        for row in rows {
            let hlc: Hlc = row?.parse().map_err(|e| Error::Corrupt(format!("{e}")))?;
            vv.insert(hlc.device, hlc);
        }
        Ok(vv)
    }

    /// The ops a peer with vector `theirs` hasn't seen, oldest first.
    pub fn ops_missing(&self, theirs: &VersionVector) -> Result<Vec<Op>> {
        Ok(oplog::read_ops(&self.conn, None)?
            .into_iter()
            .filter(|op| theirs.get(&op.hlc.device).is_none_or(|seen| op.hlc > *seen))
            .collect())
    }

    // ----- Trusted peers (local only; plain SQL, no ops) -----

    pub fn peers(&self) -> Result<Vec<Peer>> {
        let mut stmt = self.conn.prepare(
            "SELECT node_id, name, added_at, last_seen, last_addr FROM peer ORDER BY added_at",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(Peer {
                node_id: r.get(0)?,
                name: r.get(1)?,
                added_at: r.get(2)?,
                last_seen: r.get(3)?,
                last_addr: r.get(4)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn peer(&self, node_id: &str) -> Result<Option<Peer>> {
        Ok(self.peers()?.into_iter().find(|p| p.node_id == node_id))
    }

    /// Trust a device (pairing). Re-pairing updates its name and address.
    pub fn add_peer(
        &self,
        node_id: &str,
        name: &str,
        addr: Option<&str>,
        now_ms: i64,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO peer (node_id, name, added_at, last_addr) VALUES (?1, ?2, ?3, ?4) \
             ON CONFLICT(node_id) DO UPDATE SET name = excluded.name, \
               last_addr = coalesce(excluded.last_addr, peer.last_addr)",
            params![node_id, name, now_ms, addr],
        )?;
        Ok(())
    }

    /// Stop trusting a device. Returns false if it wasn't a peer.
    pub fn remove_peer(&self, node_id: &str) -> Result<bool> {
        Ok(self
            .conn
            .execute("DELETE FROM peer WHERE node_id = ?1", [node_id])?
            > 0)
    }

    /// Record a successful sync with a trusted peer (and its current name and address).
    pub fn peer_seen(
        &self,
        node_id: &str,
        name: &str,
        addr: Option<&str>,
        now_ms: i64,
    ) -> Result<()> {
        let n = self.conn.execute(
            "UPDATE peer SET last_seen = ?2, name = ?3, last_addr = coalesce(?4, last_addr) \
             WHERE node_id = ?1",
            params![node_id, now_ms, name, addr],
        )?;
        if n == 0 {
            return Err(Error::NotFound(format!("peer {node_id}")));
        }
        Ok(())
    }

    pub fn is_trusted(&self, node_id: &str) -> Result<bool> {
        Ok(self
            .conn
            .query_row("SELECT 1 FROM peer WHERE node_id = ?1", [node_id], |_| {
                Ok(())
            })
            .optional()?
            .is_some())
    }
}

#[cfg(test)]
mod tests {
    use crate::Store;

    #[test]
    fn vectors_and_missing_ops_bring_two_stores_together() {
        let a = Store::open_in_memory().unwrap();
        let b = Store::open_in_memory().unwrap();
        a.add_task("from a").unwrap();
        let t = b.add_task("from b").unwrap();
        b.set_title(&t.id, "from b, edited").unwrap();

        let (va, vb) = (a.version_vector().unwrap(), b.version_vector().unwrap());
        assert_eq!(va.len(), 1);
        assert_eq!(va.keys().next(), Some(&a.device_id()));

        // Each sends what the other lacks; afterwards both agree on everything.
        let to_b = a.ops_missing(&vb).unwrap();
        let to_a = b.ops_missing(&va).unwrap();
        assert!(to_b.iter().all(|op| op.hlc.device == a.device_id()));
        b.apply_remote(&to_b).unwrap();
        a.apply_remote(&to_a).unwrap();
        assert_eq!(a.version_vector().unwrap(), b.version_vector().unwrap());
        assert_eq!(a.state_dump().unwrap(), b.state_dump().unwrap());

        // Nothing left to send either way, and a repeat changes nothing.
        let vb = b.version_vector().unwrap();
        assert!(a.ops_missing(&vb).unwrap().is_empty());
        assert_eq!(b.apply_remote(&to_b).unwrap(), 0);
    }

    #[test]
    fn peers_are_local_and_upserted() {
        let s = Store::open_in_memory().unwrap();
        assert!(!s.is_trusted("n1").unwrap());
        s.add_peer("n1", "laptop", Some("{}"), 1).unwrap();
        s.add_peer("n1", "arch-laptop", None, 2).unwrap();
        let p = s.peer("n1").unwrap().unwrap();
        assert_eq!(
            (p.name.as_str(), p.last_addr.as_deref(), p.added_at),
            ("arch-laptop", Some("{}"), 1)
        );
        s.peer_seen("n1", "arch", Some("{\"x\":1}"), 5).unwrap();
        assert_eq!(s.peer("n1").unwrap().unwrap().last_seen, Some(5));
        assert!(s.peer_seen("nope", "x", None, 5).is_err());
        // Peers aren't replicated state.
        assert!(!s.state_dump().unwrap().contains("arch"));
        assert!(s.remove_peer("n1").unwrap());
        assert!(!s.remove_peer("n1").unwrap());
    }
}
