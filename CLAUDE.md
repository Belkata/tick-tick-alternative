# Tick Alt: project guide and handoff

A personal, local-first TickTick replacement. The pieces:
- An Android app with a home-screen widget.
- A macOS and Arch Linux (Hyprland) desktop app.
- Peer-to-peer sync between devices, with no server.
- A triage Inbox and first-class nested tags.
- A Google Calendar view: events come in, timed tasks go out.

**The plan is the source of truth.** It lives in [`plan/`](plan/) as Obsidian notes. Start at `plan/00 Overview.md`.

| Read this | For |
|---|---|
| `plan/01 Requirements & Scope.md` | What is in and out. **Don't add features the user declined** (kanban, pomodoro, habits, matrix, natural-language quick add, …) |
| `plan/02 Tech Stack Options.md` | Decision: **Rust core + Flutter UI + a Kotlin Glance widget**, plus the confirmed library versions |
| `plan/03 Architecture.md`, `plan/04 Data Model.md` | The core design. The Inbox is a *derived* view, not a stored flag |
| `plan/10 Roadmap.md` | Milestones M0–M6 and what is ticked |
| `plan/11 Open Questions.md` | Undecided items. Ask the user before deciding them |
| `plan/13 M0 Spike Log.md` | **Spike results and every gotcha found so far. Read before touching Android or Gradle.** |
| `plan/14 M1 Core Log.md` | How the core is built (op-log, HLC, filters, recurrence) and where it differs from plan/04 and plan/05 |
| `plan/15 M2 Desktop Log.md` | **M2 plan, slices and the bridge contract.** The socket API must copy the bridge API |

The UI style is decided: **Soft** (rounded cards, teal placeholder accent, Manrope headings) **with Material outlined tag chips**. The mockups are in `plan/ui-styles/styles.html`.

## Where things stand (2026-11-24, evening, on the Arch laptop)

M0 (decisions and spikes) is mostly done:
- ✅ Stack and UI style chosen; library versions confirmed.
- ✅ **Spike 1** passes on the Android emulator: Flutter calls the Rust core through `flutter_rust_bridge`. ⏸ The macOS check needs Xcode: not installed, and the user's Apple account got locked trying the App Store. When it's unlocked, download the `.xip` from developer.apple.com rather than using the App Store (it's a work Mac). ✅ Hyprland: native Wayland (`xwayland: 0`), and the Inbox integration test passes with `-d linux`.
- ✅ **Spike 3**: the Glance Inbox widget completes tasks through UniFFI. Refresh works in both directions (app → widget, widget → app). Verified with a release build.
- ✅ **Spike 5**: the calendar uses `kalender` 0.34. Drag, resize and dropping from the unscheduled strip all work, with 6 widget tests.
- 🟡 **Spike 2**: iroh 1.5 works emulator ↔ Mac in both directions (`tt sync serve/pull`, the app's Sync tab), going direct after hole punching. ⚠️ On Android the relay connects in only about 1 of 4 launches (not yet diagnosed). ⏸ Real phone on mobile data.
- ✅ **Spike 4** on Hyprland: the tray and reminders live in the Rust daemon (`daemon/`, ksni + notify-rust), not in Flutter. It shows in waybar, and Done/Snooze work through dunst. dunst needs a working `dmenu =` picker (set to rofi on the Arch laptop). ⏸ macOS needs Xcode.

M1 (core and CLI) is done on the Mac:
- The schema v2 migrations, op-log, HLC, model, filters, recurrence, reminders and the full `tt` CLI are in place.
- 46 unit tests plus convergence property tests.
- ✅ The Android app is rebuilt on the new core and checked on the emulator: the spike DB migrated, add/complete work, the widget works. Published as pre-release **v0.1.0** on GitHub for the user's phone.
- ✅ On Arch, `cargo test --workspace` passes (53 tests), clippy is clean and every `tt` command works. ⏸ The user still needs to try `tt` for real to close the M1 exit.
- Repo: **public** at https://github.com/Belkata/tick-tick-alternative (`origin`). It's public, so never commit secrets, tokens, `.key` files or personal data.

M2 (desktop app) has started; see `plan/15 M2 Desktop Log.md`:
- ✅ Slice 1: the three-pane UI (Soft theme, sidebar, task cards, detail editing, keyboard). It runs on the in-process bridge, whose API has the shape the socket will copy.
- ✅ Slice 2: the socket API. `tt-daemon` owns the DB and serves `$XDG_RUNTIME_DIR/tick-alt.sock` (JSON lines, change events, outside writes noticed via `data_version`). The app (`TaskDb.connectOrOpen`) and `tt` go through it when it serves their DB, and open the file directly otherwise.
- On desktop the app shares `tt`'s DB (`~/.local/share/tick-alt/tt.db`).
- ⚠️ Not yet built for Android since these changes.

- ✅ Slice 3: `daemon/tick-alt.service` (systemd user unit, restart on failure). Clients survive daemon restarts: they reconnect, or fall back to the file and switch back later. The release app is single instance, and the tray's Open raises it. Installed and enabled on the Arch laptop (2026-11-24) with `scripts/install-user-service.sh`: it runs this checkout's `target/release/tt-daemon` on the real DB. **After rebuilding the daemon, restart it with `systemctl --user restart tick-alt`.**

- ✅ Slice 4: the visual filter builder (all/any/not, nested groups, live count), right-click edit for lists, tags and filters (rename, colour, icon, delete), and start, repeat and reminder editing in the detail pane.

**Next:** slice 5 (PKGBUILD: app, daemon, CLI, unit, `.desktop`), which completes M2 for Arch. Then ask the user: M3 (P2P sync) or the Android catch-up (no SDK on the laptop yet). The Arch laptop has Rust 1.100, Flutter 3.50.1 in `~/flutter`, clang and `flutter_rust_bridge_codegen` 2.13.0 (with cargo-expand); there's no Android SDK yet. After that: M2 (desktop app) or the Android relay diagnosis. Ask the user. Everything in the app is spike-quality; the real UI comes in M2 and M4.

## Continuing on the Arch laptop (handoff 2026-11-24)

The dev Mac's per-machine state does **not** come along: Claude's memory, the repo-local git config, the Android debug keystore and the toolchain. Set these up first:

1. **Clone and set your identity.** The commit email is per-repo, and the user's global email may be a work one.
   ```bash
   git clone https://github.com/Belkata/tick-tick-alternative.git
   ```
   ```bash
   git -C tick-tick-alternative config user.email belkata.okfo@gmail.com
   ```
   ```bash
   git -C tick-tick-alternative config user.name "Boris Belev"
   ```
   Push with the `Belkata` GitHub account (`gh auth login`). Ask the user before installing anything (downloads need their OK).
2. **Toolchain.** Match the Mac's versions where it matters:
   - Rust 1.100 (`rustup`).
   - Flutter 3.50.1 / Dart 3.14.
   - `flutter_rust_bridge_codegen` 2.13.0 (`cargo install --locked`).
   - Flutter's Linux build needs `clang cmake ninja pkgconf gtk3` (pacman).
   - Android work also needs Android Studio, SDK platform 36 and NDK 28.2.13676358 (see plan/13).
   - Then `source scripts/env.sh`; it now handles Linux paths too.
3. **Check the core:**
   ```bash
   cargo test --workspace
   ```
   ```bash
   cargo build --release -p tt
   ```
   - The M1 exit is "I can manage tasks from the terminal on Arch", so let the user try `tt` for real (`tt --help`).
   - `$TT_TZ` overrides the zone if `/etc/localtime` isn't a symlink.
4. **Spike 1 on Hyprland:**
   ```bash
   cd app && flutter build linux
   ```
   Run it and confirm it's **native Wayland** (no XWayland: e.g. `hyprctl clients` shows `xwayland: 0`). Log the result in plan/13 and tick the roadmap.
5. **Spike 4 on Hyprland:** a tray icon in waybar (StatusNotifierItem) and a desktop notification (mako/dunst), with Done and Snooze actions. Compare Flutter `tray_manager` with a Rust tray in the daemon (plan/08).
6. **Android APKs from Arch:** a phone only accepts updates signed with the same key, and v0.1.0 was signed with the Mac's debug key (`~/.android/debug.keystore`).
   - To keep updating the installed app, have the user copy that file to the Arch laptop's `~/.android/`.
   - **Never commit it**: the repo is public.
   - Without it, the phone needs an uninstall (local data is lost) before installing an Arch-built APK.

## Repo layout

```
core/                 tt-core: domain + SQLite. Every write is an op (oplog.rs, hlc.rs) applied with per-field LWW.
                      store (migrations/, write txn), task, organize (lists/tags), checklist, reminder,
                      recur (rrule), filter (JSON AST -> SQL), dates. tests/convergence.rs = proptest
api/                  tt-api: views (sidebar/rows/detail), request protocol (proto.rs table → Request + handle + Api), socket server (feature server) and client
daemon/               tt-daemon: owns the desktop DB, serves the socket; tray (ksni) + reminder notifications (notify-rust) with Done/Snooze
sync/                 tt-sync: Spike 2 iroh node (key file, ticket, pull open tasks). M3 replaces the protocol
cli/                  `tt` CLI (`tt --help`). Short ids = LAST 8 hex chars of the UUIDv7. $TT_DB, $TT_TZ
app/                  Flutter app (android, linux, macos). lib/desk/ = M2 shell (theme, controller, sidebar, task_list, detail, shell)
  linux/packaging/    dev.tickalt.tick_alt.desktop + .svg icon (named after the Wayland app id)
  rust/               tt_bridge crate: Dart API (src/api/, flutter_rust_bridge) + Kotlin API (src/kotlin.rs, UniFFI)
  lib/                main.dart (Inbox), calendar/spike_calendar.dart, widget_bridge.dart
  android/app/src/main/kotlin/dev/tickalt/
    core/tt_bridge.kt          GENERATED UniFFI bindings (scripts/gen-kotlin-bindings.sh)
    tick_alt/widget/           Glance widget, TtCore (DB path), CompleteTaskAction
  test/                       calendar widget tests;  integration_test/  on-device Inbox test
tools/uniffi-bindgen/ uniffi-bindgen pinned to the library's UniFFI version
scripts/              env.sh (toolchain env), gen-kotlin-bindings.sh, install-user-service.sh (systemd unit), install-desktop-entry.sh (launcher entry + icon)
plan/                 Obsidian plan notes + attachments/
```

There is one native library, `libtt_bridge.so`, used by both Flutter and the widget. Both open `Context.filesDir/tt.db`; SQLite's WAL mode handles the two writers.

## Build and test

Always start with `source scripts/env.sh`. It sets up rustup, which is keg-only in Homebrew, plus `JAVA_HOME` (Android Studio's JBR) and `ANDROID_HOME`.

| What | Command |
|---|---|
| Rust tests + lint | `cargo test --workspace` · `cargo clippy --workspace --all-targets` |
| Dart analysis + calendar tests | `cd app && flutter analyze && flutter test` |
| Regenerate Dart bindings (after editing `app/rust/src/api/`) | `cd app && flutter_rust_bridge_codegen generate` |
| Regenerate Kotlin bindings (after editing `app/rust/src/kotlin.rs`) | `scripts/gen-kotlin-bindings.sh` |
| Android emulator | `emulator -avd tt_pixel -no-snapshot-save -no-boot-anim` (run it in the background) |
| On-device Flutter test | `cd app && flutter test integration_test/inbox_test.dart -d emulator-5554` |
| On-device Kotlin tests | `cd app/android && ./gradlew app:connectedDebugAndroidTest -Ptarget-platform=android-arm64` |
| Release APK (what to try on a device) | `cd app && flutter build apk --release --target-platform android-arm64` |
| Publish a test APK | bump `version:` in `app/pubspec.yaml`, build the release APK, then `gh release create vX.Y.Z <apk> --prerelease` (see v0.1.0) |

The first Gradle build takes about 6 minutes, because cargokit compiles Rust for each Android ABI. Long builds are best run in the background.

## Gotchas (each one cost time; details in `plan/13 M0 Spike Log.md`)

- **Use `package:flutter/material.dart`, not `material_ui`.** Flutter 3.50 deprecates the former, but `material_ui` is a separate copy that kalender can't see. Import flutter/material with `// ignore: deprecated_member_use`.
- **Test Android on release builds.** R8 full mode (the AGP 9 default) strips constructors that WorkManager, Room, Glance and JNA load by reflection. The keep rules are in `app/android/app/proguard-rules.pro`; add more there when something new is loaded reflectively. Debug builds hide these bugs, and a debug APK started without `flutter run` sits on the splash screen.
- **Compose compiler for Glance.** `app/android/app/build.gradle.kts` must apply `org.jetbrains.kotlin.android` and then `org.jetbrains.kotlin.plugin.compose`, both before the Flutter plugin, because the template sets `android.builtInKotlin=false`. Verify with `./gradlew :app:dependencies --configuration kotlinCompilerPluginClasspathRelease`. Don't re-add `buildFeatures.compose`.
- **Glance calls `provideGlance` once per session.** Load data inside the composition, keyed on the `version` in the widget's Glance state. Always refresh with `TaskListWidget.refreshAll()`.
- **kalender:**
  - Dropping from outside the calendar uses its internal `Reschedule` type (a `src/` import).
  - Outside draggables need `pointerDragAnchorStrategy`.
  - The app must call `selectEvent` on tap.
  - After changing the view, jump back to today.
- **Emulator quirks:**
  - The Pixel launcher removes the widget on every `adb install -r`; re-pin it with the in-app button.
  - Gboard's stylus tutorial and Google sign-in screens pop up over the app. Dismiss them, skipping sign-in.
  - `adb input` can't do long-press drags; use Flutter widget tests instead.
- Flutter's `integration_test` plugin pins `androidx.test:runner` to 1.3.0. Don't declare a newer one.
- **Core writes go through `Store::write` and its `Writer::set` op path.** Never `UPDATE` tables directly, or sync loses the change. Don't call a public `Store` method from inside `write` (the nested `BEGIN` fails); use the `pub(crate)` helpers that take a `Writer`.
- New syncable fields need an entry in `oplog::field_spec`; anything not listed is rejected.
- `cargo fmt --all` reformats the generated `app/rust/src/frb_generated.rs`. Revert it with `git checkout` and don't commit it.
- **Release APKs are signed with this Mac's debug key** (the template's `signingConfig`). A phone only accepts updates signed with the same key, so build releases on this Mac until a real release key exists (M4: APK release via CI).
- **Release builds need `INTERNET` in `src/main/AndroidManifest.xml`.** The template only puts it in debug and profile.
- The Android QA plugin's Python scripts need Python ≥ 3.10; `scripts/env.sh` puts Homebrew's `python3` first.
- Xcode isn't installed, so macOS Flutter builds fail and `flutter doctor` complains. That's expected.
- **New core calls go in the `api!` table in `api/src/proto.rs`.** That makes them reachable over the socket and as an `Api` method. Bridge view types are flutter_rust_bridge mirrors of `tt-api` types: change both together, then regenerate.
- **Never let a Rust backtrace be captured in the app process.** It segfaults: libflutter_linux_gtk.so bundles libunwind and clashes with libgcc_s. `init_app` sets `RUST_BACKTRACE=0` for that reason, so don't remove it, and don't run the app with `RUST_BACKTRACE=1`.
- **The release app is single instance** (GApplication). Launching it again only raises the open copy, so a test launch of a release build while the user's copy is open doesn't start anything. Debug builds aren't unique. Test the daemon under systemd with a temporary unit (`systemd-run --user --unit=tick-alt-test -p Restart=on-failure …`) rather than installing one.
- Hyprland 0.57 takes Lua in `hyprctl dispatch`. Useful for QA: `hl.dsp.send_shortcut({ mods = "", key = "Down", window = "class:dev.tickalt.tick_alt" })` and `hl.dsp.window.close({ window = "class:dev.tickalt.tick_alt" })`.
- Filter definitions cross the bridge as JSON strings (`app/lib/desk/filter_rule.dart` mirrors `tt_core::FilterRule`'s JSON). Keep the two in step when the core gains a field or operator.
- The integration-test binding doesn't paint into the desktop window, so screenshots of test runs show "Test starting…". Check visuals in the real app.
- flutter_rust_bridge scans every type under `app/rust/src/api/`, private ones too. Keep helpers outside it (see `src/changes.rs`).

## Working rules (from the user)

- **Small commits.** One logical change per commit; never commit large files or big mixed changes at once. Shrink screenshots before committing them (JPEG, about 400 px wide, into `plan/attachments/`).
- No `Co-Authored-By` trailers or "Generated with" footers in commit messages.
- Commit as `belkata.okfo@gmail.com` (this repo's `user.email`; the global git email is a work address). Push with the `Belkata` gh account.
- Check a library's actual version (lockfile, crates.io, pub.dev, Google Maven) before using its API, and record new versions in `plan/02`. Use the `context7` MCP for docs.
- Write readable multi-line shell commands with `\` continuations. Use `git -C <path>` outside the repo root.
- Plan before implementing. Interview the user when unsure; anything listed in `plan/11` is the user's call.
- Downloads, installs, licence acceptances and anything outward-facing need the user's OK first.
- Keep `plan/` updated as decisions are made: tick the roadmap and log spike results.

## Plugins to use (installed on the account 2026-11-24, not loaded in the session that wrote this)

- **Dart and Flutter** (official): Dart MCP server for analysis, tests and running the app; also Flutter skills such as `flutter-add-widget-test` and `flutter-add-integration-test`.
- **kotlin-lsp**: Kotlin diagnostics, so widget and Gradle Kotlin errors show up before a 2-minute Gradle build.
- **Android Emulator QA**: launch the app, inspect the UI, take screenshots and collect logs on the emulator, instead of hand-written `adb` scripts.
