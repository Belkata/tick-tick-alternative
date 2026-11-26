---
tags: [project/tick-alt, plan, m1]
created: 2026-11-24
---

# 14 M1 Core Log

Back to [[00 Overview]] · [[10 Roadmap]]. How M1 was built, and where it differs from [[04 Data Model]] and [[05 P2P Sync]].

## What exists (`core/`, `cli/`)

| Module | What it does |
|---|---|
| `hlc` | Hybrid logical clock. Text form `wall-counter-device` in fixed-width hex, so SQLite string order is HLC order (property-tested). |
| `oplog` | `Op {hlc, entity, entity_id, field, value}` and the single `apply` path: log the op, then set the field if its HLC beats the stored one (per-field LWW). A whitelist of fields and value types. |
| `store` | Opens the DB and runs migrations (`PRAGMA user_version`). It also holds the device id, and runs every write in a `BEGIN IMMEDIATE` transaction that loads and saves the clock (so the app and widget never reuse an HLC). `apply_remote`, `ops`, `state_dump`. |
| `task`, `organize`, `checklist`, `reminder` | The model and its commands. Every command issues ops through the `Writer`; nothing writes tables directly. |
| `recur` | Repeat rules via `rrule` 0.14, both modes, COUNT/UNTIL, completion history, checklist reset. |
| `filter` | JSON rule tree → SQL `WHERE`, saved filters, sort keys, FTS5 text search. |
| `dates` | Device zone, floating all-day dates, relative tokens, the `parse_when` used by the CLI. |
| `tt` CLI | `add ls show edit done reopen rm restore tag tags lists repeat remind reminders check filter completed sync`. |

**Tests:** 46 unit tests, plus `tests/convergence.rs`:
- 3 replicas make random concurrent edits, then exchange every op, each in its own random order. They must end with identical `state_dump`s (64 cases).
- A golden check that replaying the full op-log into an empty DB rebuilds the same state.

## Decisions made while building (not in the earlier notes)
- **The v2 schema has no foreign keys, CHECKs or cross-row UNIQUE constraints.**
  - Ops arrive in any order: a task can name a list that hasn't synced yet, and two devices can create the same tag name at once.
  - So the core validates in Rust instead. An invalid remote op is **logged but not applied**, so it can't wedge sync.
- **Tag membership is a per-pair LWW boolean** (field `tag:<tag id>`), rather than a separate observed-remove set. For one person with a few devices, "the last add or remove wins" is the expected outcome.
- **Filters are evaluated in SQL only.** [[04 Data Model]] also planned a Rust evaluator, but the widget calls the core anyway, so one engine is enough.
  - Tag and list values accept a path or name as well as an id.
  - A rule naming a deleted tag or list behaves as if it had no tasks.
  - Unless the rule mentions `status`, only open tasks match (`show_completed` adds done ones).
- **Dates:**
  - All-day dates are stored as UTC midnight of the date (floating).
  - Timed dates are an instant plus an IANA zone.
  - Filters compare both kinds by the local calendar day they fall on.
  - Repeats shift by local time, so 09:00 stays 09:00 across DST.
- **Relative reminders on all-day tasks** count back from **09:00 local** on the date.
- **Completion history:**
  - A repeating task writes a `completion` row per occurrence.
  - The Completed view is done tasks plus those rows.
  - COUNT is decremented in the stored rule each time.
- **Trash:**
  - Trashing a task trashes its subtasks.
  - Restoring brings back the subtasks trashed with it (same `deleted_at`).
  - Deleting a tag removes it, and the tags under it, from every task.
  - Deleting a list leaves its tasks with no list.
- **Device id:** 64 random bits stored in `meta`. M3 should tie it to the iroh key.
- **Spike databases migrate:** v1 rows get ops backfilled. Tested on the Spike 2 Mac DB: 3 tasks became 21 ops.

## Known gaps, for later milestones
- **Subtask cycles from concurrent moves are resolved convergently (2026-11-26, schema v9).**
  - Local commands refuse cycles and depth over 3.
  - A-under-B on one device and B-under-A on another both apply as ops, so the stored link is plain per-field LWW (`task.parent_raw`). `task.parent_id`, what everything reads, is the *effective* parent, derived by `oplog::resolve_parents` from the links and the HLC of the op that set each: links are taken oldest first, and one is kept only if it closes no loop and keeps the subtree within 3 levels, given those kept so far. A dropped link leaves its task top-level; it is re-evaluated whenever the links change (clearing the winner of a swap brings the loser back).
  - It depends only on converged state (links + field clocks), so it agrees on every replica, after `compact()` and on a fresh device. "Reject the later op" from [[05 P2P Sync]], but as a derived view: no history is rewritten.
  - `set_parent` re-reads the effective parent after writing and rolls back if the move didn't take. Depth counts Trash subtasks too, so every replica counts the same tree.
  - Tests: `core/tests/subtask_cycles.rs`, plus parent moves in the `convergence` and `network_sim` property tests (both state and the effective tree must converge and stay a valid forest).
- No manual ordering yet: `sort_key` exists, but nothing writes fractional indexes (M2 UI).
- No auto-purge of Trash after 30 days, and no op-log compaction (M3).
- Arch: the CLI hasn't been built or run there yet (the M1 exit criterion).
- The Android app hasn't been rebuilt on the new core. The Dart and Kotlin APIs didn't change, and the bridge compiles. The app's spike DB will migrate on first launch.

## Hardening review (2026-11-26, v0.8.0)
- Huge filter offsets return an error instead of panicking. `Store::write` rolls back when the closure panics. Tags caught in a concurrent-move loop show at the top instead of being hidden.
- API socket: 16 MiB request cap, request panics caught, binding never deletes a non-socket path. Daemon: the reminder window only advances on success; SIGTERM shuts down cleanly. CLI: short-id panic fixed.
- Open: the socket's chmod window in the macOS fallback directory.
