---
tags: [project/tick-alt, plan, m2]
created: 2026-11-24
---

# 15 M2 Desktop Log

Back to [[00 Overview]] · [[10 Roadmap]] · [[08 Desktop App]]. The plan for M2 and how it's going.

## Decisions (2026-11-24)
- **Data path: the socket.** As in [[03 Architecture]] rule 3, `tt-daemon` owns the DB on desktop. The app and `tt` talk to it over a Unix socket (JSON lines, with change events). Android keeps opening the DB in-process.
- **The three-pane UI comes first**, built on today's in-process bridge. To avoid rework, the bridge gets the **same shape the socket API will have**: views, a task detail, commands and a change stream. Slice 2 only swaps what is behind it.

## Slices
1. **Three-pane UI** ✅ (2026-11-24; the filter builder moves to slice 4)
   - Bridge API (below).
   - Soft theme with Manrope and Material outlined tag chips, light and dark.
   - Sidebar: Inbox, Lists, Filters, Tags tree, Completed, Trash, with counts.
   - Task list: quick add (plain title, lands in the current list or tag, or Inbox), task cards with priority checkbox, tag chips and date.
   - Detail: title, notes, priority, due date, list, tags, checklist, subtasks, trash and restore.
   - Keyboard: `n` new, `Enter` edit, `Space` complete, `Del` trash, arrows to move.
   - The window title is `Tick Alt`. The class stays `dev.tickalt.tick_alt`, not `tick-alt` (see [[08 Desktop App]]).
   - Narrow windows and phones keep a one-pane layout: the list, then the detail pushed on top. The calendar and sync spikes stay reachable.
2. **Socket API**
   - `tt-daemon` serves `$XDG_RUNTIME_DIR/tick-alt.sock`: one JSON request per line, one reply per line, plus `subscribe` for change events.
   - The bridge gets a desktop backend that uses it. `tt` uses it too, and opens the DB directly when no daemon runs.
3. **Daemon as a service**
   - A `systemd --user` unit (`tick-alt.service`). Reminders only fire from the daemon.
   - The tray's *Open* raises the app.
4. **Filter builder, tag and list colours, Completed and Trash polish.**
5. **Packaging:** a PKGBUILD (app, daemon, CLI, unit, `.desktop`). The DMG waits for Xcode.

## Bridge API (the contract the socket will copy)

| Call | Returns |
|---|---|
| `sidebar()` | Inbox count; lists, filters and tags (tree order, depth, colour, icon), each with its open-task count |
| `tasks(view)` | Task rows for `Inbox`, `List(id)`, `Tag(id)`, `Filter(id)`, `Completed` or `Trash` |
| `task(id)` | The full detail: fields, tags, checklist, subtasks, reminders |
| commands | add (in a view's context), complete, reopen, trash, restore, set title, notes, priority, due, list, tag, untag, checklist add, toggle and remove, add subtask |
| `changes()` | A stream that ticks after every write. In-process now; socket events in slice 2 |

A task row carries what a card shows: title, priority, due (with the all-day flag), done state, its tag chips (name, colour), checklist progress and subtask count.

## Slice 1: three-pane UI ✅ (2026-11-24, on the Arch laptop)

**What was built:**
- **Bridge** (`app/rust/src/api/tasks.rs`): the contract above.
  - `TaskView { kind, id }` is a plain struct, not a data enum: with data enums flutter_rust_bridge wants Dart's `freezed` and build_runner. It's named `TaskView` because `View` clashes with Flutter's `View` widget.
  - The change stream is a `StreamSink<u32>`; a `u64` would arrive in Dart as `BigInt`.
  - `desktop_db_path()`: on Linux and macOS the app now opens the same `tick-alt/tt.db` as `tt` and `tt-daemon` (`~/.local/share/tick-alt/` on Arch). Before, it used `~/.local/share/dev.tickalt.tick_alt/`, a separate DB. Android keeps its files dir, where the widget reads it.
- **Core:** `set_tag_style` and `set_list_style` (colour `#rrggbb`, icon). The date and reminder labels moved from the CLI to `tt-core::dates`, so `tt` and the app print the same `today`, `Fri 14:30` and `15m before`. New CLI command: `tt tags style <tag> [#rrggbb] [--icon 🌱]`.
- **App** (`app/lib/desk/`):
  - `theme.dart`: Soft tokens in light and dark (from `plan/ui-styles`), Manrope (variable font bundled under OFL in `app/assets/fonts/`, with `wght` set through `FontVariation`), `TagPill` (outlined chip with a dot) and `PriorityCheck`.
  - `controller.dart`: the view, the selection and their data. It reloads on every change tick and drops stale results.
  - `sidebar.dart`, `task_list.dart`, `detail.dart` and `shell.dart`.
  - At 760 px and wider, three panes (sidebar 200/230, detail 290/340). Narrower windows get a drawer and a pushed detail page, so the Android app gets the same shell.
  - The calendar and sync spikes stay reachable under "Spikes" in the sidebar. On Android the shell refreshes the widget after every change and reloads on resume.
- **Keyboard:**
  - `n` focuses quick add, the arrow keys move the selection, `Enter` edits the title (on narrow windows it opens the detail), `Space` completes, and `Del` trashes or restores.
  - `Esc` leaves a text field so the keys apply again. Keys are ignored while typing.
  - `t` (today) waits for the calendar, since Today is a user filter (plan/11 question 3).

**Results:**
- `flutter analyze` is clean, the 6 calendar widget tests pass, and all 54 Rust tests pass. Clippy is clean.
- `flutter test integration_test/inbox_test.dart -d linux`: **5/5** against the real core. The tests cover add and complete (the task shows in Completed), the detail's priority and tagging (a tag takes the task out of Inbox, and the tag view shows it), trash and restore, keyboard navigation, and the narrow layout.
  - A `settle()` helper pumps for about 300 ms of real time, because `pumpAndSettle` doesn't wait for the Rust calls behind a reload. Without it the keyboard test was flaky.
- On Hyprland: a native Wayland window titled `Tick Alt`. Sending keys with `hyprctl dispatch 'hl.dsp.send_shortcut({ mods = "", key = "Down", window = "class:dev.tickalt.tick_alt" })'` checked keyboard selection in the live app. That's handy for QA without a mouse; on Hyprland 0.57 `dispatch` takes Lua.

![[m2-three-pane.jpg|400]]

**Not done yet, or not checked:**
- **Not built for Android on this laptop** (no SDK here). The Dart and Rust changes are platform-neutral, and Kotlin and UniFFI are untouched. Build and test on the Mac or once the SDK is installed.
- No manual ordering yet (the `sort_key` fractional index), and no grouping by priority as in the mockup. Tasks come in the core's order.
- The detail pane shows start, repeat and reminders but can't edit them yet. `tt` can.
- No UI for tag colours yet: use `tt tags style`. The bridge has `set_tag_style` for the slice 4 editor.
