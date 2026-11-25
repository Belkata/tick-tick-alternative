//! Network simulation from plan/05 "Testing the sync engine": replicas edit and sync in
//! random pairs the way `tt-sync` does (version vectors, then the missing ops in HLC
//! order), with connections that drop mid-stream (only a prefix arrives), one-way syncs,
//! duplicated deliveries and long partitions (some pairs just don't meet for a while).
//! Once everyone finally meets, every replica must hold the same state and vector.

mod common;

use common::{Edit, edit, run, seed};
use proptest::prelude::*;
use tt_core::Store;

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
}

fn step() -> impl Strategy<Value = Step> {
    let r = 0..REPLICAS;
    prop_oneof![
        4 => (r.clone(), edit()).prop_map(|(r, e)| Step::Edit(r, e)),
        2 => (r.clone(), r.clone(), any::<u8>()).prop_map(|(from, to, keep)| Step::Send { from, to, keep }),
        2 => (r.clone(), r.clone()).prop_map(|(a, b)| Step::Session(a, b)),
        1 => (r.clone(), r).prop_map(|(from, to)| Step::Duplicate { from, to }),
    ]
}

/// What `from` would stream to `to` in a session: the ops `to`'s vector lacks.
fn missing(from: &Store, to: &Store) -> Vec<tt_core::oplog::Op> {
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

proptest! {
    #![proptest_config(ProptestConfig::with_cases(96))]

    #[test]
    fn replicas_meet_after_lossy_partitioned_syncs(steps in proptest::collection::vec(step(), 0..60)) {
        let rs: Vec<Store> = (0..REPLICAS).map(|_| Store::open_in_memory().unwrap()).collect();
        let seed = seed(&rs[0]);
        // Replica 0 created the shared tasks; it reaches the others only through syncs.
        for st in &steps {
            match st {
                Step::Edit(r, e) => run(&rs[*r], &seed, e),
                Step::Send { from, to, keep } if from != to => send(&rs[*from], &rs[*to], *keep as usize),
                Step::Session(a, b) if a != b => session(&rs[*a], &rs[*b]),
                Step::Duplicate { from, to } if from != to => {
                    // Everything `to` already holds that `from` also has, sent again.
                    let have = rs[*to].version_vector().unwrap();
                    let again: Vec<_> = rs[*from]
                        .ops()
                        .unwrap()
                        .into_iter()
                        .filter(|op| have.get(&op.hlc.device).is_some_and(|h| op.hlc <= *h))
                        .collect();
                    let before = rs[*to].state_dump().unwrap();
                    prop_assert_eq!(rs[*to].apply_remote(&again).unwrap(), 0);
                    prop_assert_eq!(rs[*to].state_dump().unwrap(), before);
                }
                _ => {}
            }
        }

        // The invariant behind version vectors: a replica's ops from any device form an
        // unbroken prefix of that device's log, however lossy the syncs were.
        for r in &rs {
            let vv = r.version_vector().unwrap();
            for (dev, top) in vv {
                let mine: Vec<_> = r.ops().unwrap().into_iter().filter(|o| o.hlc.device == dev).map(|o| o.hlc).collect();
                let origin = rs.iter().find(|o| o.device_id() == dev).unwrap();
                let theirs: Vec<_> = origin.ops().unwrap().into_iter()
                    .filter(|o| o.hlc.device == dev && o.hlc <= top).map(|o| o.hlc).collect();
                prop_assert_eq!(mine, theirs);
            }
        }

        // Partitions heal: a ring of sessions twice reaches everyone.
        for _ in 0..2 {
            for i in 0..REPLICAS {
                session(&rs[i], &rs[(i + 1) % REPLICAS]);
            }
        }
        let (dump, vv) = (rs[0].state_dump().unwrap(), rs[0].version_vector().unwrap());
        for r in &rs[1..] {
            prop_assert_eq!(&r.state_dump().unwrap(), &dump);
            prop_assert_eq!(&r.version_vector().unwrap(), &vv);
            prop_assert!(missing(&rs[0], r).is_empty());
        }
    }
}
