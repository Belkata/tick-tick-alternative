//! Concurrent subtask moves can form a cycle or a too-deep chain once replicas sync.
//! Every replica must resolve that the same way (see `oplog::resolve_parents`).

use utre_core::Store;

/// `utre_core`'s subtask nesting limit.
const MAX_DEPTH: usize = 3;

/// Two replicas sharing `n` tasks.
fn pair(n: usize) -> (Store, Store, Vec<String>) {
    let a = Store::open_in_memory().unwrap();
    let ids: Vec<String> = (0..n)
        .map(|i| a.add_task(&format!("t{i}")).unwrap().id)
        .collect();
    let b = Store::open_in_memory().unwrap();
    sync(&a, &b);
    (a, b, ids)
}

fn sync(a: &Store, b: &Store) {
    let (to_b, to_a) = (
        a.ops_missing(&b.version_vector().unwrap()).unwrap(),
        b.ops_missing(&a.version_vector().unwrap()).unwrap(),
    );
    b.apply_remote(&to_b).unwrap();
    a.apply_remote(&to_a).unwrap();
}

fn parent(s: &Store, id: &str) -> Option<String> {
    s.get_task(id).unwrap().unwrap().parent_id
}

/// No loops and nothing nested deeper than MAX_DEPTH, following effective parents.
fn assert_forest(s: &Store, ids: &[String]) {
    for id in ids {
        let mut level = 1;
        let mut cur = id.clone();
        while let Some(p) = parent(s, &cur) {
            level += 1;
            assert!(level <= MAX_DEPTH, "{id} nests {level} deep");
            cur = p;
        }
    }
}

#[test]
fn concurrent_swap_makes_no_cycle_and_agrees() {
    let (a, b, t) = pair(2);
    a.set_parent(&t[0], Some(&t[1])).unwrap(); // t0 under t1
    b.set_parent(&t[1], Some(&t[0])).unwrap(); // t1 under t0
    sync(&a, &b);
    sync(&a, &b);
    assert_eq!(a.state_dump().unwrap(), b.state_dump().unwrap());
    assert_forest(&a, &t);
    // Exactly one of the two links survives, the same on both.
    let (p0, p1) = (parent(&a, &t[0]), parent(&a, &t[1]));
    assert!(p0.is_some() != p1.is_some());
    assert_eq!((p0, p1), (parent(&b, &t[0]), parent(&b, &t[1])));
}

#[test]
fn concurrent_moves_cannot_nest_too_deep() {
    let (a, b, t) = pair(5);
    a.set_parent(&t[1], Some(&t[0])).unwrap();
    a.set_parent(&t[2], Some(&t[1])).unwrap(); // t0 > t1 > t2: three levels
    b.set_parent(&t[0], Some(&t[3])).unwrap(); // t3 > t0 > t1 > t2 would make four
    b.set_parent(&t[4], Some(&t[3])).unwrap();
    sync(&a, &b);
    assert_eq!(a.state_dump().unwrap(), b.state_dump().unwrap());
    assert_forest(&a, &t);
    assert_forest(&b, &t);
}

#[test]
fn a_losing_link_is_stable_across_compaction_and_new_devices() {
    let (a, b, t) = pair(2);
    a.set_parent(&t[0], Some(&t[1])).unwrap();
    b.set_parent(&t[1], Some(&t[0])).unwrap();
    sync(&a, &b);
    let dump = a.state_dump().unwrap();
    a.compact().unwrap();
    b.compact().unwrap();
    assert_eq!(a.state_dump().unwrap(), dump);
    assert_eq!(b.state_dump().unwrap(), dump);
    let c = Store::open_in_memory().unwrap();
    c.apply_remote(&a.ops_missing(&c.version_vector().unwrap()).unwrap())
        .unwrap();
    assert_eq!(c.state_dump().unwrap(), dump);
    assert_forest(&c, &t);
}

#[test]
fn a_dropped_link_returns_when_the_blocking_one_is_undone() {
    let (a, b, t) = pair(2);
    a.set_parent(&t[0], Some(&t[1])).unwrap();
    b.set_parent(&t[1], Some(&t[0])).unwrap();
    sync(&a, &b);
    // Whichever link lost, clearing the winner brings the loser back, on both replicas.
    let (winner, loser) = if parent(&a, &t[0]).is_some() {
        (&t[0], &t[1])
    } else {
        (&t[1], &t[0])
    };
    a.set_parent(winner, None).unwrap();
    assert_eq!(parent(&a, loser).as_deref(), Some(winner.as_str()));
    sync(&a, &b);
    assert_eq!(a.state_dump().unwrap(), b.state_dump().unwrap());
    assert_eq!(parent(&b, loser).as_deref(), Some(winner.as_str()));
}

#[test]
fn local_moves_still_refuse_cycles_and_depth() {
    let (a, _b, t) = pair(4);
    a.set_parent(&t[1], Some(&t[0])).unwrap();
    assert!(a.set_parent(&t[0], Some(&t[1])).is_err());
    assert!(a.set_parent(&t[0], Some(&t[0])).is_err());
    a.set_parent(&t[2], Some(&t[1])).unwrap();
    assert!(a.set_parent(&t[3], Some(&t[2])).is_err());
}
