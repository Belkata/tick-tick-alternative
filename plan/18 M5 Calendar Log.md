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
- Dropping a block on the all-day row makes the task all-day; dragging an all-day task into the grid gives it a time. Dropping a block back on the Unscheduled panel clears its start and due, so it really leaves the calendar (the duration is kept for the next drop). Repeating tasks can't be unscheduled until the repeat is stopped.
- Rules settled while building slice 1 (2026-11-25):
  - **The due only moves when the block passes it.** A timed block that starts after the due pushes the due to the block's end; an all-day one pushes it to that day.
  - **Switching kind keeps the due's day.** All-day → timed turns the due into 23:59 that day; timed → all-day turns it into its date.
  - **All-day blocks are one day**, at the start (else due) date. No multi-day bars in M5a: with start + duration, a due-only all-day task dragged earlier would otherwise stretch across days.
- Clicking a block opens the task's detail (the detail pane on desktop, the bottom sheet on phone).

## Slices
1. **Core and API: calendar items.** ✅ (2026-11-25, see below)
   - A `calendar(from, until, tz)` read in the `api!` table (`api/src/proto.rs`), so it reaches the socket, the bridge and the phone. It returns one item per block: task id, title, start, end, all-day, done, colour, whether it's a repeat occurrence and whether it can be dragged.
   - Expanding occurrences in `core/src/recur.rs` (fixed-mode RRULEs inside the window; past occurrences come from the completion history and show as done).
   - The Unscheduled source: the no-date default plus any saved filter, through the existing filter compiler.
   - Moves and resizes go through `set_schedule` (start + duration). Rust tests for expansion, DST days and the edit paths.
2. ✅ **Look first: a static mockup** (2026-11-25, `plan/ui-styles/calendar.html`; approved) of the desktop Week view and the phone Day view in the Soft style, for the user's OK before wiring (added to `plan/ui-styles/`). This is the step that fixes "looks very bad".
3. ✅ **Desktop calendar.** (2026-11-25, see below) Replace the spike: Week, Day and Month on kalender, Soft tiles, the all-day row, the current-time line, side-by-side overlaps, the Today button, the Unscheduled panel on the right with its source picker, drag in, move, resize, click for detail, live updates from the change stream. Integration tests, then visual checks in the real app.
4. ✅ **Phone calendar.** (2026-11-25, see below) Day by default with swipe between days, the Unscheduled panel as a bottom drawer, long-press drag, tap for the detail sheet. Checked on a release build on the emulator, then on the phone.

## Before starting
- Check kalender's current version on pub.dev (the spike used 0.34.0) and record it in [[02 Tech Stack Options]].

## Slice 1: core and API ✅ (2026-11-25, on the Arch laptop)
- `core/src/calendar.rs`: `Store::calendar(from, until, tz)` returns `Block`s (all-day ones as local-midnight spans in the viewer's zone); `place_block(id, start, end?, all_day, tz)`; `unschedule(id)`. `DEFAULT_BLOCK_MIN = 30`.
- `recur::occurrences_after` expands a fixed RRULE inside the window (COUNT counts the current occurrence; capped at 20 000 steps). Later occurrences keep the task's start-to-due gap in local time, so a 09:00 daily stays 09:00 across DST.
- Past occurrences come from the `completion` table (`occurrence_at`) and show as done.
- `tt-api`: `calendar`, `unscheduled(filter_id?)`, `place_block`, `unschedule` in the `api!` table; `CalendarItem` adds the title, priority and colour (first coloured tag, else the list) plus an icon.
- Bridge: `TaskDb.calendar`, `unscheduled`, `placeBlock`, `unschedule` (bindings regenerated).
- Tests: 12 new core tests (start + duration, due-only and all-day, done, weekly repeats, DST, COUNT and after-completion, history, placing, resizing, kind switches, refusals, unscheduling) and a socket round trip. `cargo test --workspace`, clippy, `flutter analyze` and `flutter test` are clean.
- Known approximation: a past occurrence's block uses the task's *current* start-to-due gap and duration.

## Slice 2: the mockup ✅ (2026-11-25, approved)
`plan/ui-styles/calendar.html`: desktop Week, desktop Month and two phone Day screens, light and dark, built from the app's `SoftColors` tokens.

![[m5-calendar-mockup.jpg]]

What it proposes beyond the decisions above:
- **Tiles:** a tint of the tag colour (22% light, 30% dark) with a 3 px coloured edge, a 10 px radius, the title in bold and the time in grey. Blocks of 30 min or less collapse to one line. Only movable blocks have a resize grip.
- **The sidebar folds to an icon rail in calendar mode** (» brings it back), so seven columns fit beside the Unscheduled panel on a laptop screen.
- **Selecting a block** outlines it in teal and opens its detail on the right, in place of the Unscheduled panel.
- **Drag feedback:** a dashed teal box where the task will land, labelled with the time; the card lifts and leaves a dashed placeholder in the panel.
- **Phone:** a week strip under the title (tap or swipe to change day); Unscheduled is a bottom drawer with the source picker; a **+** button for quick add.

Open for the user:
- Month shows every repeat occurrence too, so a daily task takes a line in every cell. Hide daily repeats in Month?

## Slice 3: desktop calendar ✅ (2026-11-25, on the Arch laptop)
`app/lib/calendar/calendar_page.dart` replaces the spike as the Calendar page (the spike and its test were removed in slice 4).

![[m5-calendar-week.jpg]]

- **Data:** the visible range ± 7 days comes from `TaskDb.calendar`; the change stream reloads it, so `tt`, the phone and other windows show up live. `TaskEvent` (a `KalenderEvent`) carries the `CalendarItem`; its id is task + occurrence start.
- **Views:** Day, Week (default on desktop) and Month on kalender 0.34.0 (still the latest), Monday first. The last view and the Unscheduled source are remembered per device in `calendar.json` next to the app's support files (`CalendarPrefs`), never synced.
- **Look:** tiles as in the mockup (tint + 3 px edge, faded when done, ↻ on later repeats, teal outline when selected); hour-only labels; red now-line; month items in time order with all-day first (a custom `MultiDayLayoutStrategy`); the day header stacks in Day view's narrow gutter.
- **Editing:** moves and resizes (15-minute snapping) call `placeBlock`; a card dragged from the panel lands as a timed block with the task's own duration (else 30 min) and opens in the detail pane; a block dropped on the panel is unscheduled (refused with a message for repeats). Event creation by dragging on empty grid is off.
- **Shell:** `ShellPage` builders get the `DeskController`; `railSidebar` folds the sidebar to an icon rail (» unfolds it; picking Calendar again folds it).
- **Tests:** `integration_test/calendar_test.dart` (3 tests: blocks and rail, drag in and back, selection and Month). The inbox shell tests, `flutter test` and `flutter analyze` pass. Integration tests ran under `Xvfb :78` with `GDK_BACKEND=x11`, so no window opened on the desktop.
- **Checked by eye** in a debug build on a hidden X display against a seeded demo DB: Week, Day, Month, selection, drag in, drop back (dark theme).

Limits and follow-ups:
- kalender keeps all-day items in the all-day row and timed ones in the grid, so **switching kind by dragging isn't possible**; use the detail pane's dates. Panel drops are always timed.
- Light theme wasn't checked in the real app (the tokens are the same as the mockup's, which was checked in both).
- The installed release app is still the old build; rebuild it (`flutter build linux --release`) to use the calendar day to day.
- Month still shows daily repeats in every cell (the open question above).

## Slice 4: phone calendar ✅ (2026-11-25, on the Arch laptop)
The same `CalendarPage`, with a phone layout below 760 px wide (phones, and narrow desktop windows).

![[m5-calendar-phone.jpg]]

- **Bar:** the page draws its own ☰ (`ShellPage.ownMenuButton`), the weekday and date (Day) or the month (Week, Month), a **Today** pill and a view menu. Day opens by default unless a view was saved.
- **Week strip** (Day view): Monday to Sunday around the shown day, the shown day in teal, today in teal text, a dot on days with blocks; tap a day to go there. Swiping the grid changes the day (kalender's paging). The grid's own day label is hidden here.
- **Unscheduled drawer:** a `DraggableScrollableSheet` that peeks 64 px (title, count, source pill); drag it up or tap its header to open it half way. Long-press a card (Android; a mouse drag on desktop) and the drawer drops back to its peek so the day is visible; drop on the grid to schedule. Dropping a block on the drawer unschedules it. Mouse drags move the sheet too, for narrow desktop windows.
- **Taps:** the first tap selects a block (outline, resize handles); a second tap on the selected block opens the detail as a page. Tapping a card in the drawer opens its detail.
- **Quick add:** the **+** button asks for a title and adds the task to Inbox, so it appears in the default Unscheduled list.
- **No tooltips on Android** (any width) or on the phone layout: on touch a tooltip claims the long-press, which is the drag gesture. The emulator run caught this on a wide Android screen.
- **Keyboard:** when quick add's keyboard leaves less than 360 px, the page keeps its layout and is clipped behind the dialog instead of overflowing.
- **Tests:** a phone-size test in `integration_test/calendar_test.dart` (week strip, drawer, drag out of the drawer, two taps for detail, quick add); each calendar test now starts from a fresh `calendar.json`. All 4 calendar tests, the 10 shell tests, `flutter test` and `flutter analyze` pass.
- **Checked by eye** at 412 × 880 on a hidden X display (Xvfb): Day view, opening the drawer, dragging out of it, the second tap, the week strip and quick add. That found and fixed a crash when quick add's dialog closed (its text controller was disposed during the closing animation).

**On the emulator** (`tt_pixel`, Android 16, x86_64; 2026-11-25):
- A **release** APK (`--target-platform android-x64`, built from a clean worktree at the slice 4 commit, so the TickTick-import work in progress elsewhere stayed out) starts and shows the calendar: Day view in light mode, the week strip, a completed task faded, the drawer and the + button. No R8 problems (nothing new is loaded by reflection).
- `adb input draganddrop` can't long-press, as noted in M0, so the drags were checked by running `integration_test/calendar_test.dart` on the emulator (debug build): **all 4 tests pass with real long-press drags**, the phone test using the emulator's own screen. The tests drag with a long-press on Android and a mouse drag on desktop.
- Published as pre-release **v0.3.0** (`tick-alt-0.3.0-arm64.apk`, same debug key as v0.2.0, SHA-256 `c097ae43…476b`). Not tried yet on the real phone (Xiaomi): the drawer, long-press drags and quick add by hand.
