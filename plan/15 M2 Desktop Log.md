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
2. **Socket API** ✅ (2026-11-24)
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

## Slice 2: the socket API ✅ (2026-11-24, on the Arch laptop)

**What was built:**
- **`api/` (`tt-api`)**, the one way into a database:
  - `views`: sidebar, task rows, detail and the composite commands (quick add, toggle done, due from text). They moved here from the bridge, so the daemon serves exactly what the app showed before.
  - `proto`: the `Request` enum (JSON tagged by `cmd`) and `handle(&Store, Request)`, generated by a macro from one table. Each entry gives a core call's wire arguments and the expression to run, plus whether it reads or writes. The macro also generates a typed method on `Api` per entry, taking `&str` and `&T` like `Store`, through a small `IntoWire` trait.
  - `Api`: `local(Store)` or `remote(Client)`; `connect_or_open(db)` uses the daemon only if its `hello` names the same file (canonical path). Local and remote run the same `handle`, so they can't drift.
  - `server` (feature `server`, tokio):
    - One **DB thread** owns the `Store` and runs requests in order. A successful write bumps the change version and broadcasts it.
    - Every second it checks **`PRAGMA data_version`**, so commits by other processes are broadcast too: `tt` while no daemon ran, a second app instance, Android-style direct opens. New core call: `Store::data_version()`.
    - The listener answers one JSON line per request. `subscribe` turns a connection into an event stream.
  - `client`: blocking (the CLI and the bridge are synchronous). One request at a time per connection, plus `subscribe()` on its own connection.
  - `paths`: the DB is `$TT_DB`, else `~/.local/share/tick-alt/tt.db`. The socket is `$TT_SOCKET`, else `$XDG_RUNTIME_DIR/tick-alt.sock`, else `tick-alt.sock` in the data dir on macOS. The socket is created 0600; a stale one is replaced and a live one refused.
- **`tt-daemon`** now owns the DB and serves the socket (`--db`, `--socket`, `--no-tray`). The tray refreshes on change events instead of polling, and Done in a notification goes through the DB thread. The tray no longer needs waybar at start (`assume_sni_available`), so the daemon can come up before the bar (slice 3's systemd unit).
- **Bridge:** `TaskDb.connectOrOpen(path)` on desktop and `TaskDb.open(path)` on Android and in tests. Through the daemon, the change stream relays its events, which cover this app's writes and everyone else's. The bridge's view types are now flutter_rust_bridge **mirrors** of the `tt-api` types: one definition, and the Dart API is unchanged. The sidebar header shows 🔗 when connected to the daemon. The change-stream helper sits outside `api/`, because flutter_rust_bridge scans every type in there, private ones included.
- **`tt`** uses `Api::connect_or_open`: through the daemon when it serves the same DB, else straight to the file. The daemon notices those direct writes. Its commands barely changed, because `Api` mirrors `Store`'s method names.

**Protocol by example** (`socat - UNIX-CONNECT:$XDG_RUNTIME_DIR/tick-alt.sock` works for poking at it):
```text
→ {"cmd":"hello"}
← {"ok":{"db":"/home/…/tick-alt/tt.db","version":0,"pid":210368}}
→ {"cmd":"add_task","title":"From a raw socket"}
← {"ok":{"id":"…","title":"From a raw socket",…}}
→ {"cmd":"set_title","id":"nope","title":"x"}
← {"err":"not found: task nope"}
→ {"cmd":"subscribe"}
← {"event":"changed","version":1}
```
Time-dependent reads (`tasks`, `sidebar`, `run_filter`…) carry the caller's `now_ms` and `tz`, so "today" is the client's.

**Tests:** `api/tests/socket.rs` runs a real DB thread and socket:
- Remote calls match local ones, and core errors arrive as errors.
- Subscribers hear daemon writes, and also an outside commit within the 1 s poll.
- A second daemon can't take a live socket.
- A core test checks that `data_version` ignores a connection's own writes.

**Live run on Hyprland** (demo DB, scratch socket):
- `tt-daemon --no-tray`, then the app, which shows the teal 🔗 (connected).
- `tt add …` went through the socket and appeared in the open window at once.
- `TT_SOCKET=/nowhere tt add …` wrote straight to the file and appeared within the 1 s poll. Inbox went 4 → 5 → 6, and `hello` reported version 4.

**Not done yet, or not checked:**
- **No reconnect.** If the daemon stops, an app connected to it shows errors until restarted. It doesn't fall back to the file or wait for the daemon to return. Slice 3's systemd unit (restart on failure) makes this rare. Handle it properly later.
- The app doesn't start the daemon. Until slice 3 it opens the file directly when none runs, which is safe (WAL plus `data_version`).
- `tt sync` (the Spike 2 iroh commands) still opens the DB itself. M3 replaces it.
- Requests are handled one at a time on the DB thread. That's fine for one user; a slow `sidebar` with many tags would hold up the others. Measure with the real TickTick import (M6).
- Not built for Android since these changes (no SDK on this laptop). Android uses `TaskDb.open`, which takes the in-process path.
