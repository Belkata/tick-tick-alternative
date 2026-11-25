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

/// A trusted device as one peer tells another about it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SharedPeer {
    pub node_id: String,
    pub name: String,
    /// When it was paired (epoch ms), to weigh against removals.
    pub added_at: i64,
    pub addr: Option<String>,
}

/// A forgotten device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Removal {
    pub node_id: String,
    pub removed_at: i64,
}

/// Who a device trusts and who it forgot, exchanged at the end of every sync.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustList {
    pub peers: Vec<SharedPeer>,
    pub removed: Vec<Removal>,
}

/// What merging a trust list changed (device names).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TrustChange {
    pub added: Vec<String>,
    pub removed: Vec<String>,
}

/// HLC text is `wall-counter-device` in fixed-width hex: the device is the last 16 chars.
const DEVICE_OFFSET: usize = 12 + 1 + 6 + 1;

impl Store {
    /// The newest op per device in this store's log.
    pub fn version_vector(&self) -> Result<VersionVector> {
        // The op log's newest per device, plus what compaction recorded before dropping ops.
        let mut stmt = self.conn.prepare(&format!(
            "SELECT MAX(hlc) FROM op GROUP BY substr(hlc, {}) \
             UNION ALL SELECT max_hlc FROM device_clock",
            DEVICE_OFFSET + 1
        ))?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        let mut vv = VersionVector::new();
        for row in rows {
            let hlc: Hlc = row?.parse().map_err(|e| Error::Corrupt(format!("{e}")))?;
            let top = vv.entry(hlc.device).or_insert(hlc);
            if hlc > *top {
                *top = hlc;
            }
        }
        Ok(vv)
    }

    /// Op-log compaction: drop ops a newer op on the same field has superseded. Under
    /// per-field last-writer-wins only each field's winner shapes the state, so a peer that
    /// never saw the dropped ops still converges from the winners, and a compacted log is
    /// itself the snapshot a new device bootstraps from. Returns how many ops were dropped.
    pub fn compact(&self) -> Result<usize> {
        let tx = self.conn.unchecked_transaction()?;
        // Remember each device's newest HLC first, so the vector can't go backwards.
        self.conn.execute(
            &format!(
                "INSERT INTO device_clock (device, max_hlc) \
                 SELECT substr(hlc, {0}), MAX(hlc) FROM op WHERE true GROUP BY substr(hlc, {0}) \
                 ON CONFLICT(device) DO UPDATE SET max_hlc = max(max_hlc, excluded.max_hlc)",
                DEVICE_OFFSET + 1
            ),
            [],
        )?;
        let dropped = self.conn.execute(
            "DELETE FROM op WHERE EXISTS (SELECT 1 FROM field_clock fc \
               WHERE fc.entity = op.entity AND fc.entity_id = op.entity_id \
                 AND fc.field = op.field AND fc.hlc > op.hlc)",
            [],
        )?;
        tx.commit()?;
        Ok(dropped)
    }

    /// How many ops the log holds (to decide whether compacting is worth it).
    pub fn op_count(&self) -> Result<usize> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM op", [], |r| r.get::<_, i64>(0))? as usize)
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
        // Pairing again on purpose overrides an earlier "forget".
        self.conn
            .execute("DELETE FROM peer_removed WHERE node_id = ?1", [node_id])?;
        Ok(())
    }

    /// Stop trusting a device. Returns false if it wasn't a peer.
    /// Stop trusting a device, and remember that, so the removal spreads to the other
    /// paired devices at their next sync. Returns false if it wasn't a peer.
    pub fn remove_peer(&self, node_id: &str) -> Result<bool> {
        let removed = self
            .conn
            .execute("DELETE FROM peer WHERE node_id = ?1", [node_id])?
            > 0;
        if removed {
            self.remember_removal(node_id, crate::task::now_ms())?;
        }
        Ok(removed)
    }

    fn remember_removal(&self, node_id: &str, at: i64) -> Result<()> {
        self.conn.execute(
            "INSERT INTO peer_removed (node_id, removed_at) VALUES (?1, ?2) \
             ON CONFLICT(node_id) DO UPDATE SET removed_at = max(removed_at, excluded.removed_at)",
            params![node_id, at],
        )?;
        Ok(())
    }

    /// What this device tells its peers about trust: who it trusts, and who was forgotten.
    pub fn trust_list(&self) -> Result<TrustList> {
        let peers = self
            .peers()?
            .into_iter()
            .map(|p| SharedPeer {
                node_id: p.node_id,
                name: p.name,
                added_at: p.added_at,
                addr: p.last_addr,
            })
            .collect();
        let mut stmt = self
            .conn
            .prepare("SELECT node_id, removed_at FROM peer_removed ORDER BY node_id")?;
        let removed = stmt
            .query_map([], |r| {
                Ok(Removal {
                    node_id: r.get(0)?,
                    removed_at: r.get(1)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(TrustList { peers, removed })
    }

    /// Fold a paired device's trust list into ours (plan/05 pairing step 4): its peers
    /// become ours, its removals remove ours. The newer of "added" and "removed" wins,
    /// by wall-clock time; an explicit re-pair clears a removal. `self_id` (this device)
    /// and `sender` (who sent the list) are never added or removed this way.
    pub fn merge_trust(
        &self,
        theirs: &TrustList,
        self_id: &str,
        sender: &str,
    ) -> Result<TrustChange> {
        let tx = self.conn.unchecked_transaction()?;
        let mut change = TrustChange::default();
        for r in &theirs.removed {
            if r.node_id == self_id || r.node_id == sender {
                continue;
            }
            if let Some(p) = self.peer(&r.node_id)?
                && p.added_at <= r.removed_at
            {
                self.conn
                    .execute("DELETE FROM peer WHERE node_id = ?1", [&r.node_id])?;
                change.removed.push(p.name);
            }
            self.remember_removal(&r.node_id, r.removed_at)?;
        }
        for p in &theirs.peers {
            if p.node_id == self_id || p.node_id == sender || self.is_trusted(&p.node_id)? {
                continue;
            }
            let removed_at: Option<i64> = self
                .conn
                .query_row(
                    "SELECT removed_at FROM peer_removed WHERE node_id = ?1",
                    [&p.node_id],
                    |r| r.get(0),
                )
                .optional()?;
            if removed_at.is_some_and(|t| t >= p.added_at) {
                continue; // Forgotten after that device paired it.
            }
            self.conn.execute(
                "INSERT INTO peer (node_id, name, added_at, last_addr) VALUES (?1, ?2, ?3, ?4)",
                params![p.node_id, p.name, p.added_at, p.addr],
            )?;
            self.conn
                .execute("DELETE FROM peer_removed WHERE node_id = ?1", [&p.node_id])?;
            change.added.push(p.name.clone());
        }
        tx.commit()?;
        Ok(change)
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
    use super::{Removal, SharedPeer, TrustList};
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

    #[test]
    fn trust_spreads_and_forgetting_follows() {
        let (me, sender) = ("me", "mac");
        let s = Store::open_in_memory().unwrap();
        s.add_peer(sender, "mac", None, 10).unwrap();

        // The Mac trusts the phone (paired at 20) and this device: we learn the phone only.
        let list = TrustList {
            peers: vec![
                SharedPeer {
                    node_id: "phone".into(),
                    name: "phone".into(),
                    added_at: 20,
                    addr: Some("{}".into()),
                },
                SharedPeer {
                    node_id: me.into(),
                    name: "me".into(),
                    added_at: 5,
                    addr: None,
                },
            ],
            removed: vec![],
        };
        let ch = s.merge_trust(&list, me, sender).unwrap();
        assert_eq!(ch.added, ["phone"]);
        assert!(s.is_trusted("phone").unwrap() && !s.is_trusted(me).unwrap());
        assert_eq!(s.peer("phone").unwrap().unwrap().added_at, 20);

        // The Mac forgot the phone at 30 (after it was paired): we drop it, and remember.
        let forgot = TrustList {
            peers: vec![],
            removed: vec![Removal {
                node_id: "phone".into(),
                removed_at: 30,
            }],
        };
        assert_eq!(
            s.merge_trust(&forgot, me, sender).unwrap().removed,
            ["phone"]
        );
        assert!(!s.is_trusted("phone").unwrap());
        // A stale list from another device (phone paired at 20) can't bring it back...
        assert!(s.merge_trust(&list, me, sender).unwrap().added.is_empty());
        // ...but a newer pairing somewhere (at 40) does.
        let repaired = TrustList {
            peers: vec![SharedPeer {
                node_id: "phone".into(),
                name: "phone".into(),
                added_at: 40,
                addr: None,
            }],
            removed: vec![],
        };
        assert_eq!(
            s.merge_trust(&repaired, me, sender).unwrap().added,
            ["phone"]
        );

        // Nobody removes the sender or this device through a list.
        let hostile = TrustList {
            peers: vec![],
            removed: vec![
                Removal {
                    node_id: sender.into(),
                    removed_at: 99,
                },
                Removal {
                    node_id: me.into(),
                    removed_at: 99,
                },
            ],
        };
        assert!(
            s.merge_trust(&hostile, me, sender)
                .unwrap()
                .removed
                .is_empty()
        );
        assert!(s.is_trusted(sender).unwrap());
    }

    #[test]
    fn forgetting_is_remembered_and_explicit_pairing_clears_it() {
        let s = Store::open_in_memory().unwrap();
        s.add_peer("phone", "phone", None, 1).unwrap();
        assert!(s.remove_peer("phone").unwrap());
        let list = s.trust_list().unwrap();
        assert_eq!(list.removed.len(), 1);
        assert_eq!(list.removed[0].node_id, "phone");
        s.add_peer("phone", "phone", None, i64::MAX).unwrap();
        assert!(s.trust_list().unwrap().removed.is_empty());
    }

    #[test]
    fn compaction_keeps_state_vector_and_replay() {
        let a = Store::open_in_memory().unwrap();
        let t = a.add_task("v1").unwrap();
        for i in 2..6 {
            a.set_title(&t.id, &format!("v{i}")).unwrap();
        }
        a.complete_task(&t.id).unwrap();
        a.reopen_task(&t.id).unwrap();
        let (dump, vv, before) = (
            a.state_dump().unwrap(),
            a.version_vector().unwrap(),
            a.op_count().unwrap(),
        );

        let dropped = a.compact().unwrap();
        assert!(dropped >= 5, "dropped {dropped} of {before}");
        assert_eq!(a.op_count().unwrap(), before - dropped);
        assert_eq!(a.state_dump().unwrap(), dump);
        assert_eq!(a.version_vector().unwrap(), vv);
        assert_eq!(a.compact().unwrap(), 0);

        // A new device bootstraps from the compacted log alone.
        let b = Store::open_in_memory().unwrap();
        b.apply_remote(&a.ops_missing(&b.version_vector().unwrap()).unwrap())
            .unwrap();
        assert_eq!(b.state_dump().unwrap(), dump);
        assert_eq!(
            b.version_vector()
                .unwrap()
                .get(&a.device_id())
                .map(|h| h <= &vv[&a.device_id()]),
            Some(true)
        );
        // And it doesn't ask for the dropped ops again: nothing is missing either way.
        assert!(
            a.ops_missing(&a.version_vector().unwrap())
                .unwrap()
                .is_empty()
        );
    }
}
