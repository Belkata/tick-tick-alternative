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
3. **Daemon as a service** ✅ (2026-11-24)
   - A `systemd --user` unit (`tick-alt.service`). Reminders only fire from the daemon.
   - The tray's *Open* raises the app.
4. **Filter builder, tag and list colours, Completed and Trash polish.** ✅ (2026-11-24; Trash polish deferred, see below)
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

**Bug found when the user first launched it: segfault on start with no daemon running.** Fixed the same day.
- **Cause:** `connect_or_open` hit "connection refused", which became an `anyhow` error. anyhow captures a backtrace whenever `RUST_BACKTRACE` is set, and flutter_rust_bridge's `setup_default_user_utils` sets it to 1. Unwinding inside the Flutter process mixes two unwinders: `_Unwind_GetIP` resolves to the copy of libunwind bundled in `libflutter_linux_gtk.so`, and `_Unwind_Backtrace` to libgcc_s's (`coredumpctl` showed the frames). **Any** error returned to Dart would have crashed the same way, such as a due date that doesn't parse.
- **Missed by:** the integration tests never hit an error, and the live run had the daemon up.
- **Fix:**
  - `init_app` sets `RUST_BACKTRACE=0` and `RUST_LIB_BACKTRACE=0` when they're unset, before flutter_rust_bridge's setup.
  - `Client::for_db` checks the socket with a plain connect, so the normal no-daemon case builds no error at all.
  - New integration test: an unparseable due date shows a snackbar and the app keeps running. It failed ("did not complete") before the fix; **6/6** after.

## Slice 3: the daemon as a service ✅ (2026-11-24, on the Arch laptop)

**What was built:**
- **`daemon/tick-alt.service`**, a systemd user unit:
  - `PartOf` and `After=graphical-session.target`, `WantedBy=graphical-session.target`.
  - `Restart=on-failure` with `RestartSec=2`.
  - `ExecStart=/usr/bin/tt-daemon` and `TT_APP=/usr/bin/tick-alt`, the paths the PKGBUILD will install (slice 5).
  - On this laptop `graphical-session.target` is active, and the user manager already has `WAYLAND_DISPLAY`, `XDG_CURRENT_DESKTOP` and the session bus, so the tray, notifications and Open all work from the unit. waybar isn't a systemd unit; the tray copes with it starting later.
- **`scripts/install-user-service.sh`** installs a dev copy into `~/.config/systemd/user/`, pointing at this checkout's release builds, then enables and restarts it. `--remove` undoes that.
- **Surviving restarts** (`tt-api`):
  - `Api` keeps the DB path and swaps its backend under one lock.
  - When the daemon is gone it recovers: a restarted daemon if one is already back, else the file. Reads are retried. A write is retried only if it never reached the daemon (the new `Disconnected { sent }` error); a write the daemon may have applied before dying returns the error instead of risking a duplicate.
  - `Api::upgrade()` moves back to the daemon when it serves the file again, and `Api::outside_change()` polls `data_version` while the file is open directly.
  - The client sends with **`MSG_NOSIGNAL`** (`SO_NOSIGPIPE` on macOS). Writing to a dead daemon's socket would otherwise raise SIGPIPE and kill the Flutter process: Rust ignores SIGPIPE only in programs whose `main` is Rust.
- **Bridge watcher thread** (desktop): through the daemon it relays events, and when the daemon disappears it triggers a reload, which recovers. While the file is open directly it checks every 2 s, switching back as soon as the daemon serves the file and ticking on other processes' commits. So the app stays live without a daemon too.
- **Single instance** (release builds): `G_APPLICATION_DEFAULT_FLAGS`, and `activate` presents the existing window. Debug builds stay `NON_UNIQUE`, so `flutter run` and `flutter test -d linux` work while a release copy is open.
- **Tray Open and left click** pass the tray host's XDG activation token (`ksni::current_xdg_activation_token`, retried after 1 ms) to the app as `XDG_ACTIVATION_TOKEN` and `DESKTOP_STARTUP_ID`, so Wayland may focus the window. The launched process is reaped on a thread, since a second instance exits right away.

**Results:**
- `cargo test`: 59 passed. The new `clients_survive_the_daemon_restarting` test stops and restarts a real daemon. A read falls back to the file, an outside commit is noticed, `upgrade` reconnects, and a write sent to the dead socket is retried safely.
- Flutter: analysis clean, 6 widget and 6 integration tests pass.
- **Live run with a temporary unit** (`systemd-run --user -p Restart=on-failure …`, so nothing was installed):
  - A second launch while the app was open exited in 0.04 s, leaving one window.
  - The tray's `Activate` (called over D-Bus) raised the window and started no new process.
  - `systemctl --user kill -s SIGKILL` restarted the daemon after 2 s (`NRestarts=2`). Meanwhile the open app fell back to the file without an error (🔗 crossed out), then went back to the daemon (🔗 teal). `tt add` worked during and after the restart, and both tasks appeared live (Inbox 8 → 10).

**Not done yet, or not checked:**
- Installed and enabled on the Arch laptop at the user's request (23:03). It serves `~/.local/share/tick-alt/tt.db` on `/run/user/1000/tick-alt.sock`, and its tray is registered with waybar.
- Not checked whether Hyprland actually *focuses* the raised window with a real tray click (`misc:focus_on_activate` decides). D-Bus `Activate` calls carry no token.
- LaunchAgent (macOS): not written; it needs the Mac.

## Desktop entry (2026-11-24, before slice 4, at the user's request)
- `app/linux/packaging/dev.tickalt.tick_alt.desktop` (`Exec=tick-alt`, for packages) and `dev.tickalt.tick_alt.svg`, the same teal check as the tray. The file name matches the Wayland app id, so launchers and Hyprland link the window to the entry. `StartupWMClass` is set too. `desktop-file-validate` passes.
- `scripts/install-desktop-entry.sh` installs a copy pointing at the release bundle, plus the icon (SVG and a 256 px PNG) under `~/.local/share/`, and refreshes the caches. `--remove` undoes it.
- Installed on the Arch laptop. rofi (`SUPER+D`, drun) lists "Tick Alt", and `gtk-launch dev.tickalt.tick_alt` opens the app.
- The PKGBUILD (slice 5) installs these to `/usr/share/applications` and `/usr/share/icons/hicolor`.

## Slice 4: filter builder and editing ✅ (2026-11-24, on the Arch laptop)

**What was built:**
- **Filter builder** (`desk/filter_builder.dart`, with the model and JSON in `desk/filter_rule.dart`):
  - Groups match **all** or **any** of their children, nested two levels deep. A **not** chip negates any condition or group.
  - Each condition row is field → operator → a value control that fits the field:
    - tag paths, with a picker of existing tags;
    - list, priority and status dropdowns, and yes/no for Inbox, has-time and repeats;
    - date tokens (`today`, `+7d`, `this_week`… or a date), and "between" with two dates;
    - text.
  - A **live count** ("3 open tasks match") refreshes about 250 ms after each change, through the core's `run_rule`. An invalid rule shows the core's message in red instead.
  - Also: name, optional icon, include completed, sort.
  - Filters cross the bridge as **JSON strings** (`filter_json`, `save_filter`, `update_filter`, `rule_count`). That keeps the recursive rule enum out of flutter_rust_bridge (which would need `freezed`); the Dart model parses and writes the core's exact JSON (5 unit tests, including the core's documented example).
- **Sidebar:**
  - The Filters section is always shown, with a **+**.
  - **Right-click (or long-press)** a filter to edit it in the builder, or a list or tag to open its edit dialog: rename, colour (12 swatches tuned for light and dark, or none), icon (emoji) and delete (confirmed).
  - Coloured lists tint their icon.
- **Detail pane:**
  - **Start date**, typed like the due date.
  - **Repeat** presets: every day, weekday, week, 2 weeks, month or year. Also any other existing RRULE, or a custom one typed in, plus "from completion" (`after_completion`).
  - **Reminders**: remove, and add one at the time, 5, 15 or 30 minutes, 1 hour or 1 day before, or at a typed date and time.
- **tt-api and the bridge:** new calls `rule_count` and `set_start_text`. `TaskDetail.repeat_after_completion`. Repeat, reminder, filter and list or tag edits use the existing requests.
- **Controller:**
  - If the open list, tag or filter disappears (deleted here, by `tt` or elsewhere), the view falls back to Inbox. Other reload failures show a message instead of throwing.
  - The delete dialogs leave the view before deleting.
  - Found by the new filter test: deleting the open filter made the following reload ask for it, and the unhandled error broke the tests after it.

**Results:**
- `flutter analyze` is clean. Unit and widget tests: 11 (6 calendar, 5 filter model). Integration tests on Linux: **9/9**. The new ones:
  - Build a filter (tag has "home"), see "1 open task matches", save it, see its view, then right-click → Delete.
  - Right-click a tag → green swatch → Save.
  - Start, due, repeat "every day" with "from completion", then add and remove a reminder.
- `cargo test`: 59 passed.
- ⚠️ **No screenshot of the new screens.** On desktop the integration-test binding doesn't draw into the window (it shows "Test starting…"), and Hyprland gives no way to click from a script. The user tries it in the real app.

**Not done:**
- **Trash polish:** no "empty Trash" or 30-day purge yet. The core has neither (M1's known gaps); it needs a core purge op first, which belongs with M3's op-log compaction.
- Filter grouping (`group`), manual sort and drag-to-reorder: later.
- Keyboard access to the builder and the edit dialogs: they're mouse-first for now.

## Slice 5: the Mac (2026-11-25, on the Mac) ✅
**Done without Xcode:**
- **`tt-daemon` builds and runs on macOS.** The ksni tray moved to `daemon/src/tray_sni.rs` (Linux only); `tray_mac.rs` is a menu-bar item with the same interface; `tray_none.rs` covers other platforms.
  - `tray_mac.rs` uses **tray-icon 0.26.1** (muda 0.21.2 menus), objc2 0.6.5, objc2-app-kit 0.3.2 and dispatch2 0.3.1. AppKit needs its run loop on the main thread, so on macOS `main` hands the main thread to `tray::run_app` and the tokio runtime runs on a second thread. Updates reach the icon through GCD's main queue; the icon's state lives in a main-thread `thread_local`. The process is an accessory app (no Dock icon).
  - It shows a template icon (filled while the Inbox has tasks, an outline when empty) with the Inbox count beside it. The menu matches Linux: the first 5 Inbox tasks (each sends its own notification, for testing), Open, Refresh, the sync line, Sync now, Quit.
  - **Notifications:** see "With Xcode" below (the first attempt, notify-rust's `NSUserNotificationCenter` backend, never delivered).
  - The device name comes from `scutil --get ComputerName` (macOS has no `/etc/hostname`). Open runs `open $TT_APP`, else `open -b dev.tickalt.tickAlt`.
- **LaunchAgent:** `daemon/dev.tickalt.daemon.plist` and `scripts/install-launch-agent.sh` (`--remove` to undo). It starts at login, restarts after a crash (`KeepAlive.SuccessfulExit = false`) and stays stopped after Quit. Logs go to `~/Library/Logs/tick-alt/daemon.log`.
- **Checked on the Mac:** `cargo test --workspace` passes and clippy is clean. Two daemons on throwaway DBs paired with `tt sync pair`/`join` and synced both ways, including auto-sync after edits.
- ⚠️ **Unix socket paths max out at 104 bytes on macOS.** The default (`~/Library/Application Support/tick-alt/tick-alt.sock`) fits; a long `$TT_SOCKET` fails with "path must be shorter than SUN_LEN".
- ⏸ Not verified: the Linux tray after the move (verbatim, plus a small wrapper). Run `cargo clippy -p tt-daemon` on Arch.
- **App Sandbox off** in both entitlements files: in the sandbox, `dirs::data_dir()` is the app's container, so the app couldn't reach the daemon's socket or the shared `tick-alt/tt.db`. Fine for a personal, ad-hoc-signed app (plan/08).

**With Xcode** (27.2, from the App Store; the user accepted the licence and ran `-runFirstLaunch` with sudo):
- **`flutter build macos` works.** Fixes on the way:
  - The `tt_bridge` pod didn't link **SystemConfiguration** (iroh watches network changes with it): `s.frameworks` in `app/rust_builder/macos/tt_bridge.podspec`.
  - **CocoaPods crashes on a non-UTF-8 locale** (`Unicode Normalization not appropriate for ASCII-8BIT`). `scripts/env.sh` now exports `LANG=en_US.UTF-8` on macOS. After changing a podspec, run `pod install` in `app/macos` (the build doesn't notice).
  - The first build added the usual CocoaPods integration (Podfile, Podfile.lock, xcconfig includes): committed.
- **The app is "Tick Alt"** (`PRODUCT_NAME`; bundle id stays `dev.tickalt.tickAlt`) and opens at 1280×720, centred, then restores its last frame (`MainFlutterWindow.swift`). The bundle is universal (arm64 + x86_64), signed ad hoc. The copyright line is still the template's: it waits for the user's LICENSE decision.
- **Checked in the real app** (screenshots through background computer use): it connects to the daemon, shows the Inbox, and a task added with `tt` appears live. Typing into it from the background didn't reach Flutter's text field (no accessibility tree), so the user tries adding in the app.
- **Notifications** take a bundle:
  - First attempt, notify-rust's default backend (`NSUserNotificationCenter`) from the bare binary: without `set_application`, mac-notification-sys runs AppleScript `get id of application "use_default"`, which pops up a **"Choose Application" dialog**. With it set, macOS **silently dropped** the notifications (that app never got permission).
  - Now: `UNUserNotificationCenter` through **mac-usernotifications 0.3.1** directly. notify-rust's `preview-macos-un` wrapper only has blocking calls, and `block_on_current` fails with **`MainThreadNotRunning` whenever AppKit's run loop isn't idle at that instant** (it checks `CFRunLoop::is_waiting`), both on send and while waiting for the answer. The async `send().await` / `response().await` on tokio don't check; the answer arrives through the main run loop that `tray_mac::run_app` runs. notify-rust is now a non-macOS dependency.
  - The daemon asks for permission at startup (in the background: the first prompt waits for the user). Without a bundle (`target/release/tt-daemon`), `request_auth` fails and reminders are only logged, instead of the crash `UNUserNotificationCenter` would cause.
  - A click on the notification opens the app; Done and Snooze arrive as action ids.
  - ✅ The user allowed notifications, a reminder appeared, and **Done from Notification Centre completed the task**. macOS's default style for a new app is Banners, which hide after a few seconds; **System Settings > Notifications > Tick Alt > Alerts** keeps reminders on screen.
- **`scripts/build-macos.sh`** builds the Rust binaries and the Flutter app, then puts `tt-daemon` into `Tick Alt.app/Contents/Helpers/Tick Alt Daemon.app` (`dev.tickalt.daemon`, `LSUIElement`, display name "Tick Alt") with `tt` beside it. It signs everything ad hoc and makes **`dist/tick-alt-<version>-macos.dmg`** (64 MB for 0.3.0). The daemon has a bundle of its own so notifications work and their actions reach it rather than the window app.
- `install-launch-agent.sh [<Tick Alt.app>]` runs the bundled daemon (else `target/release/tt-daemon`, without notifications). ⏸ The user hasn't installed the LaunchAgent yet (the agent that ran this wasn't allowed to add a login item).
- ⏸ **Not tested:** Snooze on macOS, the menu's Open, Sync now from the menu, pairing the Mac with the phone or the Arch laptop.

## Arranging tags in the sidebar (2026-11-25, on the Arch laptop, at the user's request)
- The **Tags** header has an **Arrange** button (⇅). It swaps each tag's count for a drag handle; dragging a handle moves the tag, with the tags under it, among its siblings. A drop elsewhere snaps to the nearest place under the same parent (re-nesting stays with `tt tags mv`). **Done** (✓) ends it. Same on the phone's drawer.
- Core: `Store::place_tag(id, before)` (plan/04 "Tag"); `load_tags` now walks the tree in Rust, siblings by `sort_key`, then name **ignoring case** (it used to sort by the raw path, so capitalised names came before all lowercase ones). Socket/`Api`: `PlaceTag`. Dart: `TaskDb.placeTag`. CLI: `tt tags order <tag> [--before <tag>]`.
- The drop logic is in `app/lib/desk/tag_order.dart` (unit tests in `app/test/tag_order_test.dart`); the sidebar shows the new order at once and the reload confirms it. Integration test: "arrange tags by dragging them in the sidebar".
- Lists and filters can't be arranged yet; they have `sort_key` too, so the same approach would fit.
- ⏸ Checked by tests only, not by eye in the running app.
