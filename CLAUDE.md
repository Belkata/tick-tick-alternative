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
| `plan/16 M3 Sync Log.md` | M3 sync protocol, pairing, what's next |
| `plan/17 M4 Android Log.md` | M4: phone sync, reminders, background sync, CI release, emulator results |
| `plan/18 M5 Calendar Log.md` | M5 plan: M5a local calendar (decisions from the 2026-11-25 interview, slices), then M5b Google |
| `plan/15 M2 Desktop Log.md` | **M2 plan, slices and the bridge contract.** The socket API must copy the bridge API |

The UI style is decided: **Soft** (rounded cards, teal placeholder accent, Manrope headings) **with Material outlined tag chips**. The mockups are in `plan/ui-styles/styles.html`.

## Where things stand (handoff, 2026-11-25, Arch laptop)

**Done** (details in the logs named in the table above):
- **M0**: stack and UI style decided; Spikes 1, 3, 4, 5 pass, on macOS too since M2 slice 5.
- **M1**: the core (op-log, HLC, filters, recurrence, reminders) and the full `tt` CLI. ⏸ The user hasn't yet "managed tasks from the terminal for real" (the M1 exit).
- **M2** (desktop, `plan/15`): the three-pane Flutter app, `tt-daemon` owning the DB over `$XDG_RUNTIME_DIR/tick-alt.sock`, the systemd user unit, the filter builder and edit dialogs, the PKGBUILD (`packaging/arch`, test-built, not installed). **Slice 5, the Mac (2026-11-25):** the daemon runs there with a menu-bar icon and `UNUserNotificationCenter` reminders, `scripts/build-macos.sh` makes `Tick Alt.app` (daemon bundled inside) and a DMG, and there's a LaunchAgent script.
- **M3** (sync, `plan/16`): protocol 2 over iroh (tt2 pairing tickets, trusted peers, trust sharing, removal spreading), compaction (schema **v5**), the network simulation test. The daemon runs the sync node with auto-sync, tray status and Sync now.
- **M4** (Android, `plan/17`): in-process phone sync (`app/rust/src/phone.rs` + shared `tt_sync::service::SyncService`), QR pairing, AlarmManager reminders (Done / Snooze 10 min / 1 h), WorkManager background sync (15 min, plus expedited after outside edits), a share target, the CI release workflow. Tested on the emulator.
- **M5a** (local calendar, `plan/18`): Day/Week/Month on real tasks in the Soft style, Unscheduled panel (desktop) and bottom drawer (phone) with drag in and out, `placeBlock`/`unschedule` in the core. M5b (Google) is next.
- **Released**: pre-release **v0.3.0** on GitHub (`tick-alt-0.3.0-arm64.apk`, the calendar; v0.2.0 before it), signed with this laptop's **debug** key, so it updates v0.2.0 in place. Its CI run fails until release-key secrets exist; that's expected.

**On the Mac now (2026-11-25):** Xcode 27.2 installed and set up. Commits up to the M2 slice 5 plan update are **not pushed**. The bundled daemon runs from `app/build/…/Tick Alt.app` (started with `open`, not the LaunchAgent) on a fresh `~/Library/Application Support/tick-alt/tt.db` with 2 test tasks; it isn't paired with anything. The user allowed its notifications. Next on the Mac: install the DMG's app into /Applications and run `scripts/install-launch-agent.sh "/Applications/Tick Alt.app"`, then pair with the laptop or phone. Not tested yet: Snooze, the menu's Open, Sync now.

**Running on the Arch laptop (as of its handoff):**
- The `tick-alt` user service runs this checkout's `target/release/tt-daemon` on `~/.local/share/tick-alt/tt.db` (schema v5, sync node `c382b28c2f…`, UDP 41824). **After rebuilding the daemon: `systemctl --user restart tick-alt`.**
- Desktop app, `tt` and daemon were built from `6e11fb9`. The desktop entry is installed (rofi lists "Tick Alt").
- **The user's phone** (Xiaomi 22101316G) runs v0.2.0 and is **paired with this daemon** (2026-11-25 09:09); tasks sync both ways. A reminder round-trip (09:14 "Reminder test from the laptop": Done on the phone → completed on the laptop) was in progress when this was written; check `tt completed`.
- Toolchain: Rust 1.100, Flutter 3.50.1 (`~/flutter`), clang, `flutter_rust_bridge_codegen` 2.13.0 (+cargo-expand), Android Studio 2026.2.2 (JBR = JDK 25), SDK in `~/Android/Sdk` (platform 36, build-tools 37.0.0, NDK 28.2.13676358), emulator `tt_pixel` (headless: `emulator -avd tt_pixel -no-window -gpu swiftshader_indirect`).

**Next** (ask the user which; anything in `plan/11` is theirs to decide):
1. **Finish the phone checks**: did the reminder's Done reach the laptop? Does scanning the pairing QR work on the real phone camera? If not, make tickets compact (e.g. `tt3` + base64url with short field names; keep decoding `tt2`), with tests.
2. **Visual checks on desktop** (only integration tests have run these): the Devices page, the tray tooltip and Sync now, the filter builder, the right-click edit dialogs, and the start, repeat and reminder fields.
3. **Needs the user**: a release key plus the 4 CI secrets (`plan/07` "Release signing"; switching the phone to it costs one uninstall), a LICENSE file (copyright name), installing the PKGBUILD (sudo), and whether to keep the Mac's debug keystore at all (the phone now uses this laptop's).
4. **Then pick a milestone**: widget polish (choose a list or filter, scrolling, **+** quick add, midnight refresh), **M5** (calendar plus Google: Week/Day/Month, drag to schedule, OAuth, events in and tasks out), **M6** (TickTick CSV import, then switch over).
5. Smaller open items: convergent resolution of concurrent subtask cycles (`plan/14`), mobile-data sync on the real phone, the Android relay flakiness from Spike 2 (not seen on the emulator), Flutter's "migrate to built-in Kotlin" warning.

Repo: **public** at https://github.com/Belkata/tick-tick-alternative (`origin`, pushed through `6e11fb9`). Never commit secrets, tokens, `.key`/`.jks` files, `key.properties` or personal data.

## Setting up another machine
Per-machine state doesn't travel: Claude's memory, the repo-local git identity, keystores, the toolchain.
- Clone, then `git config user.email belkata.okfo@gmail.com` and `git config user.name "Boris Belev"` in the repo (the global email may be a work one). Push as the `Belkata` gh account. Ask before installing anything.
- Toolchain as listed above, then `source scripts/env.sh` (handles macOS and Linux paths).
- A phone accepts updates only when they're signed with the same key as the installed app. v0.2.0 is signed with **this laptop's** debug key (`~/.android/debug.keystore`). Building updates elsewhere needs that file, or better the release key (`plan/07`). **Never commit keystores.**

## Repo layout

```
core/                 tt-core: domain + SQLite. Every write is an op (oplog.rs, hlc.rs) applied with per-field LWW.
                      store (migrations/ v1–v5), task, organize, checklist, reminder, recur, filter, dates,
                      replica (version vectors, peers, trust, compaction). tests/: convergence + network_sim (proptest)
api/                  tt-api: views (sidebar/rows/detail), request protocol (proto.rs table → Request + handle + Api),
                      socket server (feature server; SyncControl hook) and client (reconnect/fallback)
sync/                 tt-sync: iroh node. replicate.rs = sync protocol 2 (tt2 pairing, trust, op streaming);
                      service.rs = SyncService (rounds, status, auto-sync) shared by daemon and phone
daemon/               tt-daemon: owns the desktop DB, serves the socket, runs SyncService, reminders. Tray: tray_sni.rs (Linux, ksni),
                      tray_mac.rs (menu bar, tray-icon), tray_none.rs. tick-alt.service (systemd), dev.tickalt.daemon.plist (LaunchAgent)
cli/                  `tt` CLI (`tt --help`, `tt sync …`). Short ids = LAST 8 hex chars of the UUIDv7. $TT_DB, $TT_TZ, $TT_SOCKET
app/                  Flutter app (android, linux, macos)
  lib/main.dart       startup: desktop → TaskDb.connectOrOpen (daemon); Android → TaskDb.open + startPhoneSync
  lib/desk/           the shell: theme, controller, sidebar, task_list, detail, filter_builder(+filter_rule), style_dialog, devices
  lib/widget_bridge.dart   Android channel: widget refresh, reminders, zone, device name, share target
  rust/               tt_bridge: Dart API (src/api/, frb), Kotlin API (src/kotlin.rs, UniFFI), phone.rs (in-process sync), changes.rs
  linux/packaging/    dev.tickalt.tick_alt.desktop + .svg icon (named after the Wayland app id)
  android/app/src/main/kotlin/dev/tickalt/
    core/tt_bridge.kt          GENERATED UniFFI bindings (scripts/gen-kotlin-bindings.sh)
    tick_alt/                  MainActivity (channels, share target), widget/ (Glance), reminders/ (alarms, Done/Snooze),
                               sync/SyncWorker (WorkManager)
  test/               calendar + filter-rule tests;  integration_test/  shell tests (run with -d linux)
packaging/arch/       PKGBUILD (+ .SRCINFO)
.github/workflows/    android-release.yml (APK on v* tags; needs release-key secrets)
tools/uniffi-bindgen/ uniffi-bindgen pinned to the library's UniFFI version
scripts/              env.sh, gen-kotlin-bindings.sh, install-user-service.sh, install-desktop-entry.sh,
                      build-macos.sh (app + DMG), install-launch-agent.sh
dist/                 built APKs and DMGs for the user (git-ignored)
plan/                 Obsidian plan notes + attachments/
```

There is one native library, `libtt_bridge.so`, used by Flutter, the widget and the background job. On Android they all open `Context.filesDir/tt.db` (WAL handles the writers) and share one process-wide sync node. On desktop everything goes through `tt-daemon` when it runs.

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
| macOS app + DMG (daemon bundled) | `scripts/build-macos.sh` (`--no-dmg` for the .app only) → `dist/tick-alt-<version>-macos.dmg` |
| macOS LaunchAgent | `scripts/install-launch-agent.sh ["/Applications/Tick Alt.app"]` (`--remove` to undo) |
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
- **Release APKs are signed with the Arch laptop's debug key** (the template's `signingConfig`; v0.1.0 used the Mac's). A phone only accepts updates signed with the same key, so build releases there until a real release key exists (M4: APK release via CI).
- **Release builds need `INTERNET` in `src/main/AndroidManifest.xml`.** The template only puts it in debug and profile.
- The Android QA plugin's Python scripts need Python ≥ 3.10; `scripts/env.sh` puts Homebrew's `python3` first.
- **macOS daemon:** AppKit owns the main thread (`tray_mac::run_app`), so tokio runs on a second thread and the menu-bar icon is updated through GCD's main queue. Don't put `#[tokio::main]` back on macOS.
- **macOS notifications need the bundled daemon** (`Tick Alt Daemon.app` from `scripts/build-macos.sh`); bare `target/release/tt-daemon` only logs reminders. Use mac-usernotifications' **async** API: the blocking calls (and notify-rust's wrapper) fail with `MainThreadNotRunning` whenever the run loop is busy.
- **Unix socket paths max out at 104 bytes on macOS.** Keep test `$TT_SOCKET`s short (the scratchpad path is too long).
- After editing a podspec, run `pod install` in `app/macos`; `flutter build macos` doesn't notice. CocoaPods needs a UTF-8 locale (`scripts/env.sh` sets it).
- The macOS app has **no App Sandbox**: in it the app couldn't reach the daemon's socket or the shared DB.
- **New core calls go in the `api!` table in `api/src/proto.rs`.** That makes them reachable over the socket and as an `Api` method. Bridge view types are flutter_rust_bridge mirrors of `tt-api` types: change both together, then regenerate.
- **Never let a Rust backtrace be captured in the app process.** It segfaults: libflutter_linux_gtk.so bundles libunwind and clashes with libgcc_s. `init_app` sets `RUST_BACKTRACE=0` for that reason, so don't remove it, and don't run the app with `RUST_BACKTRACE=1`.
- **The release app is single instance** (GApplication). Launching it again only raises the open copy, so a test launch of a release build while the user's copy is open doesn't start anything. Debug builds aren't unique. Test the daemon under systemd with a temporary unit (`systemd-run --user --unit=tick-alt-test -p Restart=on-failure …`) rather than installing one.
- Hyprland 0.57 takes Lua in `hyprctl dispatch`. Useful for QA: `hl.dsp.send_shortcut({ mods = "", key = "Down", window = "class:dev.tickalt.tick_alt" })` and `hl.dsp.window.close({ window = "class:dev.tickalt.tick_alt" })`.
- **After compaction a version vector means "has every winner up to here", not "holds every op".** Don't build logic on the latter (see plan/16 slice 3).
- Filter definitions cross the bridge as JSON strings (`app/lib/desk/filter_rule.dart` mirrors `tt_core::FilterRule`'s JSON). Keep the two in step when the core gains a field or operator.
- **Android SDK tooling:** `sdkmanager` is deprecated; use `android --no-metrics --sdk=$HOME/Android/Sdk sdk install <pkg/with/slashes>`. Flutter's accessibility tree on Android is coarse (one merged node), so drive the emulator by coordinates from `adb exec-out screencap` (1080×2400).
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
