//! Property test from plan/05 P2P Sync: replicas that make random concurrent
//! edits and then exchange every op, each in its own random order, end up with
//! identical state.

mod common;

use common::{edit, run, seed};
use proptest::prelude::*;
use utre_core::Store;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn replicas_converge(
        edits in proptest::collection::vec(proptest::collection::vec(edit(), 0..12), 3),
        orders in proptest::collection::vec(any::<prop::sample::Index>(), 3),
        shuffles in proptest::collection::vec(any::<u64>(), 3),
    ) {
        let replicas: Vec<Store> = (0..3).map(|_| Store::open_in_memory().unwrap()).collect();
        let seed = seed(&replicas[0]);
        let base = replicas[0].ops().unwrap();
        for r in &replicas[1..] {
            r.apply_remote(&base).unwrap();
        }

        // Concurrent, offline edits.
        for (r, es) in replicas.iter().zip(&edits) {
            for e in es {
                run(r, &seed, e);
            }
        }

        // Everyone receives everything, each in a different order.
        let all: Vec<_> = replicas.iter().flat_map(|r| r.ops().unwrap()).collect();
        for (i, r) in replicas.iter().enumerate() {
            let mut ops = all.clone();
            let mut state = shuffles[i] | 1;
            let start = orders[i].index(ops.len().max(1));
            let n = ops.len();
            ops.rotate_left(start.min(n));
            for k in (1..ops.len()).rev() {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                ops.swap(k, (state % (k as u64 + 1)) as usize);
            }
            r.apply_remote(&ops).unwrap();
        }

        let dump = replicas[0].state_dump().unwrap();
        for r in &replicas[1..] {
            prop_assert_eq!(&r.state_dump().unwrap(), &dump);
        }
    }
}

/// Golden test: replaying a device's whole op-log into an empty database rebuilds its state.
#[test]
fn op_log_replays_to_the_same_state() {
    let a = Store::open_in_memory().unwrap();
    let seed = seed(&a);
    a.set_title(&seed.tasks[0], "renamed").unwrap();
    a.tag_task(&seed.tasks[1], &seed.tags[2]).unwrap();
    a.complete_task(&seed.tasks[2]).unwrap();
    a.trash_task(&seed.tasks[3]).unwrap();
    a.delete_tag(&seed.tags[0]).unwrap();

    let b = Store::open_in_memory().unwrap();
    b.apply_remote(&a.ops().unwrap()).unwrap();
    assert_eq!(a.state_dump().unwrap(), b.state_dump().unwrap());
}
