//! Random edits shared by the convergence and network simulation property tests.
#![allow(dead_code)]

use proptest::prelude::*;
use utre_core::Store;

#[derive(Debug, Clone)]
pub enum Edit {
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
    /// Move a task under another (or to the top level); refused locally if it would loop.
    Parent(usize, Option<usize>),
}

pub fn edit() -> impl Strategy<Value = Edit> {
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
        (t.clone(), any::<bool>()).prop_map(|(t, d)| Edit::Check(t, d)),
        (t.clone(), proptest::option::of(t)).prop_map(|(t, p)| Edit::Parent(t, p)),
        (0..4usize, 0..4usize).prop_map(|(t, p)| Edit::Parent(t, Some(p))),
    ]
}

pub struct Seed {
    pub tasks: Vec<String>,
    pub tags: Vec<String>,
    pub lists: Vec<String>,
    pub items: Vec<String>,
}

pub fn seed(s: &Store) -> Seed {
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

pub fn run(s: &Store, seed: &Seed, e: &Edit) {
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
        Edit::Parent(t, p) => s.set_parent(&seed.tasks[*t], p.map(|p| seed.tasks[p].as_str())),
    };
}

/// The effective subtask tree has no loops and nothing nested deeper than 3 levels.
pub fn forest_ok(s: &Store, seed: &Seed) -> bool {
    seed.tasks.iter().all(|id| {
        let mut level = 1;
        let mut cur = id.clone();
        while let Some(p) = s.get_task(&cur).unwrap().unwrap().parent_id {
            level += 1;
            if level > 3 {
                return false;
            }
            cur = p;
        }
        true
    })
}

/// Every task's effective parent, for comparing trees between replicas.
pub fn tree(s: &Store, seed: &Seed) -> Vec<Option<String>> {
    seed.tasks
        .iter()
        .map(|id| s.get_task(id).unwrap().unwrap().parent_id)
        .collect()
}
