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
- [x] README and website icon (✅ PR 60, 2026-11-28): replace the old sunrise logo (`docs/logo.svg`, `docs/logo.png`, used by `README.md` and as the logo and favicon of `docs/index.html`) with the u-tick app icon from `packaging/icon/utre-icon.svg` (plan/20 round 8). Asked by the user 2026-11-27.
- [x] CI runs only the checks a change needs (✅ PR 58, 2026-11-28; a `changes` job plus job-level `if:`, and an always-running `CI result` check; a docs-only PR skips everything else and is still mergeable): give each `ci.yml` job a path filter, so a docs-only or plan-only PR doesn't run the Rust, Flutter and macOS jobs. Rough map: Rust test/clippy/fmt/deny on `**.rs`, `Cargo.*`; the macOS daemon job on `daemon/**`, `api/**`, `core/**`, `sync/**`, `Cargo.*`; Flutter analyze/test/goldens/integration on `app/**` (and on Rust changes the bridge depends on); shellcheck on `scripts/**`, `**.sh`; plus each workflow's own file. Required checks must still report on skipped PRs, or merges hang on "Expected — waiting": filter inside the jobs (e.g. a `changes` job with `dorny/paths-filter` and `if:` on each job, or one always-running summary job as the only required check), not with workflow-level `paths:`. Which checks are required is set in GitHub settings, the user's call. Asked by the user 2026-11-27.
- [ ] More caching in GitHub Actions, where it pays off. Already cached in `ci.yml`: Rust (`Swatinem/rust-cache`, saved only from `main`) in the Linux Rust and macOS daemon jobs, the Flutter SDK and pub cache in the three Flutter jobs, and cargokit's build for the integration job. Missing:
  - `macos-release.yml`: the Rust targets (cargokit and `utre-daemon` for arm64 and x86_64), CocoaPods (`app/macos/Pods`, keyed on `Podfile.lock`), and Xcode's derived data if it helps.
  - `android-release.yml`: Gradle (`setup-java` `cache: gradle`), Flutter (`cache: true` plus `pub-cache-key`), and cargokit's Android Rust build.
  - The `fmt` job (rustup only, probably not worth it). `deny` and `shellcheck` are already fast.
  - Catch: caches saved on a tag run are scoped to that tag, so the next release can't reuse them. Release workflows can restore `main`'s caches but must save under keys a `main` job also writes. So either add a `main` job that warms the release caches (e.g. weekly, or when `Cargo.lock`/`pubspec.lock`/`Podfile.lock` change), or accept cold release builds. Measure before and after (the macOS release build took about 18 min cold on its first run, 0.8.10).
  Asked by the user 2026-11-27.
- [ ] Move all builds to CI, so releases don't depend on this laptop (asked by the user 2026-11-28). Today the laptop builds the APK (release key), the Linux tarball and the pacman package, and signs the pacman repo and the macOS zip with the repo GPG key; only the macOS app builds in CI (`macos-release.yml`). To move:
  - **APK:** `android-release.yml` already builds on tags; it needs the 4 `ANDROID_KEYSTORE_*` secrets (from `~/utre-release.jks`) and `UTRE_GOOGLE_ANDROID_CLIENT_ID`, the `build-apk.sh` certificate check, and to upload `utre-X-arm64.apk` to the release (it has `contents: read` and only uploads an artifact today).
  - **Linux tarball and `utre-bin`:** build in an Arch container on `ubuntu-latest` (`archlinux:base-devel`) so the glibc and libraries match Arch, run `build-linux-tarball.sh` and `makepkg`, upload to the release.
  - **Signing:** the pacman repo and the macOS zip are signed with the repo GPG key (`8D9A9E54…43ADD341`). Either put a CI copy of it in a secret (simplest; a leak lets someone sign Utre packages), or create a separate CI signing subkey that can be revoked on its own, or keep signing on the laptop as a final step. The user's call.
  - **pacman repo publish:** `pacman-repo-publish.sh` in the Arch container after the build (needs `repo-add` and the signing key).
  - **Release step:** a tag push then does everything; the release step shrinks to bump version, tag, wait for CI, verify. The CI caching work (cache-warm.yml) helps keep these builds fast.
- [ ] Packaging: PKGBUILD and a DMG. 🟡 PKGBUILD done (`packaging/arch`, test-built with makepkg); DMG done (`scripts/build-macos.sh`, ad hoc signed, M2 slice 5). Since 2026-11-27 also a signed pacman repo, the macOS release zip and the macOS auto-update exist (see plan/08).

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
- [x] In-app auto-update from GitHub releases (PRs 56, 57). ✅ Daily check, silent install once Utre is the installer of record; emulator only, see [[17 M4 Android Log]] "In-app updater".

**Exit:** phone ↔ desktop sync, and the widget is on the home screen.

## M5: Calendar and Google
Split into **M5a** (local calendar on real tasks) and **M5b** (Google). Plan and decisions in [[18 M5 Calendar Log]].

- [ ] Week, Day and Month views (desktop and Android). All-day row, current-time line, overlap layout. ✅ Desktop (slice 3) and phone (slice 4) on the local calendar; see [[18 M5 Calendar Log]].
- [x] Drag to schedule and resize. Unscheduled sidebar from a filter. ✅ Desktop panel and phone drawer (slices 3–4).
- [x] Google OAuth (desktop loopback PKCE, Android AuthorizationClient). ✅ Both work on real devices (2026-11-26). Keychain storage is still open.
- [x] Events in: calendar picker, incremental `syncToken` polling, cache. ✅ Desktop and phone (plan/19 slices 2, 3, 5).
- [ ] Tasks out: "Tasks" calendar, deterministic event ids, idempotent push and reconcile. 🟡 Code merged (plan/19 slice 4, PR 5), tested only against a fake Google: try it with a real account (switch on the Google page).
- [x] Put the OAuth app in production to avoid 7-day token expiry. ✅ Done by the user (reported 2026-11-28). Google's sign-in screen still says the app isn't verified; it works (calendars show on the phone and laptops). Verification is only needed if other people should sign in without that warning.

**Exit:** the calendar replaces TickTick's for daily time-blocking.

## M6: Import and switch-over
- [x] Switched over: TickTick data imported, filters recreated, and Utre is the user's only task manager (reported 2026-11-28).
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
