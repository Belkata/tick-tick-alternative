//! Property test from plan/05 P2P Sync: replicas that make random concurrent
//! edits and then exchange every op, each in its own random order, end up with
//! identical state.

use proptest::prelude::*;
use tt_core::Store;

#[derive(Debug, Clone)]
enum Edit {
    Title(usize, u8),
    Priority(usize, u8),
    Tag(usize, usize),
    Untag(usize, usize),
    Complete(usize),
    Reopen(usize),
    Trash(usize),
    Restore(usize),
    List(usize, Option<usize>),
    Check(usize, bool),
}

fn edit() -> impl Strategy<Value = Edit> {
    let t = 0..4usize;
    prop_oneof![
        (t.clone(), any::<u8>()).prop_map(|(t, n)| Edit::Title(t, n)),
        (t.clone(), 0..4u8).prop_map(|(t, p)| Edit::Priority(t, p)),
        (t.clone(), 0..3usize).prop_map(|(t, g)| Edit::Tag(t, g)),
        (t.clone(), 0..3usize).prop_map(|(t, g)| Edit::Untag(t, g)),
        t.clone().prop_map(Edit::Complete),
        t.clone().prop_map(Edit::Reopen),
        t.clone().prop_map(Edit::Trash),
        t.clone().prop_map(Edit::Restore),
        (t.clone(), proptest::option::of(0..2usize)).prop_map(|(t, l)| Edit::List(t, l)),
        (t, any::<bool>()).prop_map(|(t, d)| Edit::Check(t, d)),
    ]
}

struct Seed {
    tasks: Vec<String>,
    tags: Vec<String>,
    lists: Vec<String>,
    items: Vec<String>,
}

fn seed(s: &Store) -> Seed {
    let tasks: Vec<String> = (0..4)
        .map(|i| s.add_task(&format!("task {i}")).unwrap().id)
        .collect();
    let items = tasks
        .iter()
        .map(|t| s.add_checklist_item(t, "step").unwrap())
        .collect();
    Seed {
        tags: ["a", "b", "a/c"]
            .iter()
            .map(|p| s.ensure_tag_path(p).unwrap())
            .collect(),
        lists: ["Work", "Home"]
            .iter()
            .map(|n| s.create_list(n).unwrap())
            .collect(),
        tasks,
        items,
    }
}

fn run(s: &Store, seed: &Seed, e: &Edit) {
    // Some edits are refused in the current state (e.g. reopen an open task); that's fine.
    let _ = match e {
        Edit::Title(t, n) => s.set_title(&seed.tasks[*t], &format!("title {n}")),
        Edit::Priority(t, p) => s.set_priority(&seed.tasks[*t], *p),
        Edit::Tag(t, g) => s.tag_task(&seed.tasks[*t], &seed.tags[*g]),
        Edit::Untag(t, g) => s.untag_task(&seed.tasks[*t], &seed.tags[*g]),
        Edit::Complete(t) => s.complete_task(&seed.tasks[*t]).map(|_| ()),
        Edit::Reopen(t) => s.reopen_task(&seed.tasks[*t]).map(|_| ()),
        Edit::Trash(t) => s.trash_task(&seed.tasks[*t]).map(|_| ()),
        Edit::Restore(t) => s.restore_task(&seed.tasks[*t]).map(|_| ()),
        Edit::List(t, l) => s.set_task_list(&seed.tasks[*t], l.map(|l| seed.lists[l].as_str())),
        Edit::Check(t, d) => s.set_checklist_done(&seed.items[*t], *d),
    };
}

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
