---
tags: [project/tick-alt, plan, m5]
created: 2026-11-25
---

# 18 M5 Calendar Log

Back to [[00 Overview]] · [[10 Roadmap]] · [[06 Google Calendar]]. The plan for M5 and how it's going.

M5 is split in two:
- **M5a, the local calendar:** your real tasks on Day, Week and Month grids, on desktop and phone. Nothing outside the repo is needed.
- **M5b, Google:** events in, tasks out ([[06 Google Calendar]]). It needs a Google Cloud project and OAuth consent screen set up by the user, so it comes after M5a is in daily use.

Today's calendar is still the M0 spike (`app/lib/calendar/spike_calendar.dart`): in-memory sample data, default kalender tiles and no Soft styling. M5a replaces it.

## Decisions (2026-11-25, interview)
- **A block is start + duration.** Dropping or moving a task sets `start_at`; resizing sets `duration_min`. `due_at` is untouched, so overdue filters don't shift. A task with only a due time shows at that time with the default length. This matches the core's anchor rule (start, else due) used by reminders.
- **Default duration: 30 min** when a task without `duration_min` lands on the grid.
- **Unscheduled panel:** by default every open task with no start or due. You can switch it to any saved filter, and the choice is remembered per device.
- **Repeating tasks show every occurrence** in the visible range, on the calendar grid only. Task lists and filters keep showing a repeating task once, at its next date.
  - Only the next occurrence can be dragged; moving it moves the series' next date. Later occurrences are shown but fixed.
  - "Repeat after completion" tasks can only show their next occurrence: the later dates depend on when it gets done.
  - Google gets every occurrence too (M5b), which **changes** [[06 Google Calendar]]'s "next occurrence only" rule. How (a recurring Google event, or pushing a window of occurrences) is decided in M5b.
- **Completed tasks show faded**, with no strike-through, so they stay readable.
- **Phone opens on Day.** Desktop opens on Week (as [[08 Desktop App]] says). Views are Day, Week and Month only; 3-day and agenda were declined in [[01 Requirements & Scope]].

## Defaults (not asked; change any of them)
- Weeks start on **Monday**.
- Blocks take the colour of the task's first tag, else its list, else the teal accent. Future repeat occurrences get a small repeat icon.
- Dropping a block on the all-day row makes the task all-day; dragging an all-day task into the grid gives it a time. Dropping a block back on the Unscheduled panel clears its start (and its due, if the due is the only date).
- Clicking a block opens the task's detail (the detail pane on desktop, the bottom sheet on phone).

## Slices
1. **Core and API: calendar items.**
   - A `calendar(from, until, tz)` read in the `api!` table (`api/src/proto.rs`), so it reaches the socket, the bridge and the phone. It returns one item per block: task id, title, start, end, all-day, done, colour, whether it's a repeat occurrence and whether it can be dragged.
   - Expanding occurrences in `core/src/recur.rs` (fixed-mode RRULEs inside the window; past occurrences come from the completion history and show as done).
   - The Unscheduled source: the no-date default plus any saved filter, through the existing filter compiler.
   - Moves and resizes go through `set_schedule` (start + duration). Rust tests for expansion, DST days and the edit paths.
2. **Look first: a static mockup** of the desktop Week view and the phone Day view in the Soft style, for the user's OK before wiring (added to `plan/ui-styles/`). This is the step that fixes "looks very bad".
3. **Desktop calendar.** Replace the spike: Week, Day and Month on kalender, Soft tiles, the all-day row, the current-time line, side-by-side overlaps, the Today button, the Unscheduled panel on the right with its source picker, drag in, move, resize, click for detail, live updates from the change stream. Integration tests, then visual checks in the real app.
4. **Phone calendar.** Day by default with swipe between days, the Unscheduled panel as a bottom drawer, long-press drag, tap for the detail sheet. Checked on a release build on the emulator, then on the phone.

## Before starting
- Check kalender's current version on pub.dev (the spike used 0.34.0) and record it in [[02 Tech Stack Options]].
