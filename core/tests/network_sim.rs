//! Network simulation from plan/05 "Testing the sync engine": replicas edit and sync in
//! random pairs the way `utre-sync` does (version vectors, then the missing ops in HLC
//! order), with connections that drop mid-stream (only a prefix arrives), one-way syncs,
//! duplicated deliveries, long partitions (some pairs just don't meet for a while) and
//! op-log compaction at any moment.
//! Once everyone finally meets, every replica must hold the same state and vector.

mod common;

use common::{Edit, edit, forest_ok, run, seed, tree};
use proptest::prelude::*;
use utre_core::Store;

const REPLICAS: usize = 4;

#[derive(Debug, Clone)]
enum Step {
    Edit(usize, Edit),
    /// `from` sends `to` what it's missing; only the first `keep`/256 of it arrives
    /// (a connection that drops mid-stream delivers a prefix, in HLC order).
    Send {
        from: usize,
        to: usize,
        keep: u8,
    },
    /// A full two-way session.
    Session(usize, usize),
    /// `from` re-sends ops `to` already has (a retried batch): nothing may change.
    Duplicate {
        from: usize,
        to: usize,
    },
    /// A replica compacts its op log (drops superseded ops).
    Compact(usize),
}

fn step() -> impl Strategy<Value = Step> {
    let r = 0..REPLICAS;
    prop_oneof![
        4 => (r.clone(), edit()).prop_map(|(r, e)| Step::Edit(r, e)),
        2 => (r.clone(), r.clone(), any::<u8>()).prop_map(|(from, to, keep)| Step::Send { from, to, keep }),
        2 => (r.clone(), r.clone()).prop_map(|(a, b)| Step::Session(a, b)),
        1 => (r.clone(), r.clone()).prop_map(|(from, to)| Step::Duplicate { from, to }),
        1 => r.prop_map(Step::Compact),
    ]
}

/// What `from` would stream to `to` in a session: the ops `to`'s vector lacks.
fn missing(from: &Store, to: &Store) -> Vec<utre_core::oplog::Op> {
    from.ops_missing(&to.version_vector().unwrap()).unwrap()
}

fn send(from: &Store, to: &Store, keep: usize) {
    let ops = missing(from, to);
    let n = ops.len() * keep / 256;
    to.apply_remote(&ops[..n]).unwrap();
}

fn session(a: &Store, b: &Store) {
    // Both vectors are taken before either side applies anything, as in the protocol.
    let (to_b, to_a) = (missing(a, b), missing(b, a));
    b.apply_remote(&to_b).unwrap();
    a.apply_remote(&to_a).unwrap();
}

/// Run `steps`, let everyone meet, and check the replicas agree.
fn check(steps: &[Step]) -> Result<(), TestCaseError> {
    let rs: Vec<Store> = (0..REPLICAS)
        .map(|_| Store::open_in_memory().unwrap())
        .collect();
    let seed = seed(&rs[0]);
    // Replica 0 created the shared tasks; it reaches the others only through syncs.
    for st in steps {
        match st {
            Step::Edit(r, e) => run(&rs[*r], &seed, e),
            Step::Send { from, to, keep } if from != to => {
                send(&rs[*from], &rs[*to], *keep as usize)
            }
            Step::Session(a, b) if a != b => session(&rs[*a], &rs[*b]),
            Step::Duplicate { from, to } if from != to => {
                // A retried batch: ops `to` already holds, sent again by `from`. (By the
                // ops themselves, not the vector: after compaction a vector no longer
                // means "holds every op below this", only "has every winner".)
                let have: std::collections::HashSet<_> =
                    rs[*to].ops().unwrap().into_iter().map(|o| o.hlc).collect();
                let again: Vec<_> = rs[*from]
                    .ops()
                    .unwrap()
                    .into_iter()
                    .filter(|op| have.contains(&op.hlc))
                    .collect();
                let before = rs[*to].state_dump().unwrap();
                prop_assert_eq!(rs[*to].apply_remote(&again).unwrap(), 0);
                prop_assert_eq!(rs[*to].state_dump().unwrap(), before);
            }
            Step::Compact(r) => {
                let (dump, vv) = (
                    rs[*r].state_dump().unwrap(),
                    rs[*r].version_vector().unwrap(),
                );
                rs[*r].compact().unwrap();
                prop_assert_eq!(rs[*r].state_dump().unwrap(), dump);
                prop_assert_eq!(rs[*r].version_vector().unwrap(), vv);
            }
            _ => {}
        }
    }
    let compacted = steps.iter().any(|s| matches!(s, Step::Compact(_)));

    // The invariant behind version vectors: a replica's ops from any device form an
    // unbroken prefix of that device's log, however lossy the syncs were.
    // (Compaction leaves deliberate gaps: superseded ops are gone.)
    for r in rs.iter().filter(|_| !compacted) {
        let vv = r.version_vector().unwrap();
        for (dev, top) in vv {
            let mine: Vec<_> = r
                .ops()
                .unwrap()
                .into_iter()
                .filter(|o| o.hlc.device == dev)
                .map(|o| o.hlc)
                .collect();
            let origin = rs.iter().find(|o| o.device_id() == dev).unwrap();
            let theirs: Vec<_> = origin
                .ops()
                .unwrap()
                .into_iter()
                .filter(|o| o.hlc.device == dev && o.hlc <= top)
                .map(|o| o.hlc)
                .collect();
            prop_assert_eq!(mine, theirs);
        }
    }

    // Partitions heal: a ring of sessions twice reaches everyone. The ring alone must
    // already bring the replicas to the same state.
    for _ in 0..2 {
        for i in 0..REPLICAS {
            session(&rs[i], &rs[(i + 1) % REPLICAS]);
        }
    }
    let dump = rs[0].state_dump().unwrap();
    let shape = tree(&rs[0], &seed);
    for r in &rs {
        prop_assert!(forest_ok(r, &seed), "loop or too deep: {:?}", shape);
        prop_assert_eq!(&tree(r, &seed), &shape);
        prop_assert_eq!(&r.state_dump().unwrap(), &dump);
    }
    // After compaction the ring can leave a *superseded* op stranded: replicas A and C
    // never meet in a ring of A-B-C-D, and if B and D compacted the op away, it only
    // survives on C. Its field is already decided everywhere (that is why they could drop
    // it), so the state agrees, but A's vector may lack the op's device altogether. That
    // is harmless, so the log-level checks below run after every pair has met once.
    if compacted {
        for i in 0..REPLICAS {
            for j in i + 1..REPLICAS {
                session(&rs[i], &rs[j]);
            }
        }
        prop_assert_eq!(&rs[0].state_dump().unwrap(), &dump);
    }
    let vv = rs[0].version_vector().unwrap();
    for r in &rs[1..] {
        if compacted {
            // A compacting replica remembers the HLC of ops it dropped (`device_clock`
            // keeps its vector from going backwards). Nobody can send those ops, so
            // the counter of a dropped op never reaches the others: the vectors agree
            // on which devices they cover and on every op that still exists, which
            // `missing` below checks, but not on that counter. Harmless: no op is
            // owed anywhere.
            // A device may even be missing from one vector altogether: if compaction
            // dropped every op it ever wrote, only the compacting replica remembers
            // its clock. So a device found in just one vector must have no op left
            // anywhere.
            let mine = r.version_vector().unwrap();
            for dev in mine.keys().chain(vv.keys()) {
                if mine.contains_key(dev) != vv.contains_key(dev) {
                    for x in [&rs[0], r] {
                        prop_assert!(
                            x.ops().unwrap().iter().all(|o| o.hlc.device != *dev),
                            "device {} is in one version vector only but still has ops",
                            dev
                        );
                    }
                }
            }
            prop_assert!(missing(r, &rs[0]).is_empty());
        } else {
            prop_assert_eq!(&r.version_vector().unwrap(), &vv);
        }
        prop_assert!(missing(&rs[0], r).is_empty());
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(96))]

    #[test]
    fn replicas_meet_after_lossy_partitioned_syncs(steps in proptest::collection::vec(step(), 0..60)) {
        check(&steps)?;
    }
}

/// Found by CI (the run for the app-icon PR): the ring never joined replicas 0 and 2, 1
/// and 3 compacted device 1's only op away, so it survived on replica 2 alone and replica
/// 0's vector lacked device 1. The state still agrees; only the log-level check was too
/// strict for a ring.
#[test]
fn a_superseded_op_can_be_stranded_by_compaction_in_a_ring() {
    let steps = [
        Step::Session(0, 1),
        Step::Session(0, 3),
        Step::Edit(1, Edit::Parent(0, None)),
        Step::Send {
            from: 0,
            to: 2,
            keep: 67,
        },
        Step::Session(3, 1),
        Step::Edit(2, Edit::Parent(0, None)),
        Step::Edit(2, Edit::Title(0, 0)),
        Step::Send {
            from: 2,
            to: 3,
            keep: 128,
        },
        Step::Compact(3),
        Step::Session(1, 2),
        Step::Compact(1),
    ];
    check(&steps).unwrap();
}
