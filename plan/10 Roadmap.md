---
tags: [project/utre, plan, roadmap]
---

# 10 Roadmap

Back to [[00 Overview]].

Each milestone ends with something usable. Building the core first means the CLI is usable before any UI exists.

## M0: Decisions and spike
- [x] Choose the stack ([[02 Tech Stack Options]]): Option A, Rust core + Flutter + Kotlin Glance widget.
- [x] Choose the UI style ([[09 UI Style Options]]): Soft with Material tag chips.
- [x] Confirm current versions: iroh, Flutter, flutter_rust_bridge, UniFFI, Glance. See the table in [[02 Tech Stack Options]].
- [ ] Spike 1: Rust core library plus `flutter_rust_bridge` running on an Android phone, macOS and Hyprland. ✅ Android (emulator); ✅ Hyprland (native Wayland); ✅ macOS (2026-11-25, the M2 app). See [[13 M0 Spike Log]].
- [ ] Spike 2: iroh connects phone ↔ Mac on LAN and across networks (phone on mobile data). ✅ Emulator ↔ Mac both directions (direct after hole punching); ⚠️ Android relay connects only sometimes; ⏸ real phone on mobile data. See [[13 M0 Spike Log]].
- [x] Spike 3: Glance widget calls the Rust core through UniFFI and completes a task. ✅ Emulator, release build; see [[13 M0 Spike Log]].
- [ ] Spike 4: tray icon and notification on Hyprland (waybar) and macOS. ✅ Hyprland: Rust tray in `utre-daemon` (ksni + notify-rust), Done and Snooze work through dunst; ✅ macOS: menu-bar icon (tray-icon) and UNUserNotificationCenter from the bundled daemon, Done tested (M2 slice 5). See [[13 M0 Spike Log]].
- [x] Spike 5: Flutter week grid with drag and resize (evaluate the `kalender` package against a custom build). ✅ Use kalender 0.34; see [[13 M0 Spike Log]].

**Exit:** the stack is confirmed, or we switch to Option B.

## M1: Core and CLI
- [x] SQLite schema and migrations. Model for tasks, lists, tags (nested), checklist and subtasks.
- [x] Inbox rule view.
- [x] Filter AST, SQL compiler and tests.
- [x] Recurrence (fixed and after-completion) and the completion history.
- [x] Reminder schedule computation.
- [x] Op-log and HLC from day one, even before sync exists. Convergence is property-tested.
- [x] The `utre` CLI: add, ls, done, tags, filters (plus edit, show, lists, repeat, remind, check, completed, trash).

**Exit:** I can manage tasks from the terminal on Arch. ✅ on the Mac; ✅ on Arch the 53 tests pass, clippy is clean, and every `utre` command works; ⏸ the user tries it for real. See [[14 M1 Core Log]].

## M2: Desktop app (Mac and Arch)
- [ ] Tray daemon with socket API and systemd / LaunchAgent setup. 🟡 The socket API is done (slice 2) and so is the systemd user unit (slice 3: restart on failure, clients reconnect, single-instance app). On the Mac (slice 5) the daemon runs with a menu-bar icon and has a LaunchAgent (`scripts/install-launch-agent.sh`). See [[15 M2 Desktop Log]].
- [ ] Three-pane UI: sidebar, list, detail. Tag tree, lists, filter builder, Completed and Trash. 🟡 Slices 1 and 4 done: sidebar (tag tree, lists, saved filters, Completed, Trash), task cards, detail editing (dates, repeat, reminders), keyboard, the visual filter builder, and tag and list colours and icons. Trash purge waits for the core. See [[15 M2 Desktop Log]].
- [ ] Desktop notifications with Done and Snooze. 🟡 Linux: dunst (Spike 4). macOS: from the bundled daemon, Done tested, Snooze not yet (M2 slice 5).
- [ ] README and website icon: replace the old sunrise logo (`docs/logo.svg`, `docs/logo.png`, used by `README.md` and as the logo and favicon of `docs/index.html`) with the u-tick app icon from `packaging/icon/utre-icon.svg` (plan/20 round 8). Asked by the user 2026-11-27.
- [ ] CI runs only the checks a change needs: give each `ci.yml` job a path filter, so a docs-only or plan-only PR doesn't run the Rust, Flutter and macOS jobs. Rough map: Rust test/clippy/fmt/deny on `**.rs`, `Cargo.*`; the macOS daemon job on `daemon/**`, `api/**`, `core/**`, `sync/**`, `Cargo.*`; Flutter analyze/test/goldens/integration on `app/**` (and on Rust changes the bridge depends on); shellcheck on `scripts/**`, `**.sh`; plus each workflow's own file. Required checks must still report on skipped PRs, or merges hang on "Expected — waiting": filter inside the jobs (e.g. a `changes` job with `dorny/paths-filter` and `if:` on each job, or one always-running summary job as the only required check), not with workflow-level `paths:`. Which checks are required is set in GitHub settings, the user's call. Asked by the user 2026-11-27.
- [ ] More caching in GitHub Actions, where it pays off. Already cached in `ci.yml`: Rust (`Swatinem/rust-cache`, saved only from `main`) in the Linux Rust and macOS daemon jobs, the Flutter SDK and pub cache in the three Flutter jobs, and cargokit's build for the integration job. Missing:
  - `macos-release.yml`: the Rust targets (cargokit and `utre-daemon` for arm64 and x86_64), CocoaPods (`app/macos/Pods`, keyed on `Podfile.lock`), and Xcode's derived data if it helps.
  - `android-release.yml`: Gradle (`setup-java` `cache: gradle`), Flutter (`cache: true` plus `pub-cache-key`), and cargokit's Android Rust build.
  - The `fmt` job (rustup only, probably not worth it). `deny` and `shellcheck` are already fast.
  - Catch: caches saved on a tag run are scoped to that tag, so the next release can't reuse them. Release workflows can restore `main`'s caches but must save under keys a `main` job also writes. So either add a `main` job that warms the release caches (e.g. weekly, or when `Cargo.lock`/`pubspec.lock`/`Podfile.lock` change), or accept cold release builds. Measure before and after (the macOS release build took ~N min cold; fill in from the first run).
  Asked by the user 2026-11-27.
- [ ] Packaging: PKGBUILD and a DMG. 🟡 PKGBUILD done (`packaging/arch`, test-built with makepkg); DMG done (`scripts/build-macos.sh`, ad hoc signed, M2 slice 5). Neither installed yet.

**Exit:** usable daily on one machine.

## M3: P2P sync (desktop ↔ desktop)
- [ ] Device identity, QR / text pairing, trusted peers. 🟡 Text pairing (`tt2` tickets), trusted peers and trust sharing (with removal spreading) done; QR waits for M4. See [[16 M3 Sync Log]].
- [x] Sync protocol: version vectors and op streaming. Compaction and snapshots. ✅ Version vectors and op streaming; compaction drops superseded ops, and the compacted log is the snapshot.
- [x] Property-based convergence tests and a network simulation. ✅ See [[16 M3 Sync Log]].
- [x] Tray sync status. ✅ Tooltip line plus a working Sync now in the menu (M3 slice 2).

**Exit:** Mac ↔ Arch stay in sync.

## M4: Android app and widget
- [x] Flutter UI on Android: lists, tags, filters, detail, quick add, share target. ✅ The desktop shell's phone layout; share target (M4, see [[17 M4 Android Log]]).
- [x] Glance widget: config, scrolling list, checkbox complete, **+** quick add, midnight refresh. Done on the emulator (release build); see [[17 M4 Android Log]] "Widget polish".
- [x] AlarmManager reminders with notification actions. ✅ Exact alarms, Done, Snooze 10 min and 1 h.
- [x] WorkManager sync, QR scanner pairing. ✅ 15-min job plus expedited after outside edits; QR shown on Pair and scanned on Join (the real-phone scan isn't tested yet).
- [ ] APK release via CI. 🟡 Workflow written (`.github/workflows/android-release.yml`); needs the user's release key and secrets.

**Exit:** phone ↔ desktop sync, and the widget is on the home screen.

## M5: Calendar and Google
Split into **M5a** (local calendar on real tasks) and **M5b** (Google). Plan and decisions in [[18 M5 Calendar Log]].

- [ ] Week, Day and Month views (desktop and Android). All-day row, current-time line, overlap layout. ✅ Desktop (slice 3) and phone (slice 4) on the local calendar; see [[18 M5 Calendar Log]].
- [x] Drag to schedule and resize. Unscheduled sidebar from a filter. ✅ Desktop panel and phone drawer (slices 3–4).
- [x] Google OAuth (desktop loopback PKCE, Android AuthorizationClient). ✅ Both work on real devices (2026-11-26). Keychain storage is still open.
- [x] Events in: calendar picker, incremental `syncToken` polling, cache. ✅ Desktop and phone (plan/19 slices 2, 3, 5).
- [ ] Tasks out: "Tasks" calendar, deterministic event ids, idempotent push and reconcile. 🟡 Code merged (plan/19 slice 4, PR 5), tested only against a fake Google: try it with a real account (switch on the Google page).
- [ ] Put the OAuth app in production to avoid 7-day token expiry.

**Exit:** the calendar replaces TickTick's for daily time-blocking.

## M6: Import and switch-over
- [ ] TickTick CSV importer (dry run first). Recreate the 4 filters.
- [ ] Use only Utre for a week and fix whatever hurts.
- [ ] Back up and export the DB (a JSON dump), so I never get locked in.

**Exit:** TickTick uninstalled. 🎉

## Later (not v1)
- Natural-language quick add.
- Kanban view.
- ~~Markdown notes.~~ ✅ Done in v0.6.0 (rendered in the detail pane; stored as plain text).
- Global quick-add popup.
- Command palette.
- Optional homelab relay or always-on peer.
- DB encryption.

UI iteration rounds: see [[20 UI Iteration Log]].
