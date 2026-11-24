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

The UI style is decided: **Soft** (rounded cards, teal placeholder accent, Manrope headings) **with Material outlined tag chips**. The mockups are in `plan/ui-styles/styles.html`.

## Where things stand (2026-11-24)

M0 (decisions and spikes) is mostly done:
- ✅ Stack and UI style chosen; library versions confirmed.
- ✅ **Spike 1** passes on the Android emulator: Flutter calls the Rust core through `flutter_rust_bridge`. ⏸ The macOS check needs Xcode, which the user declined for now. ⏸ The Hyprland check needs the user's Arch machine.
- ✅ **Spike 3**: the Glance Inbox widget completes tasks through UniFFI. Refresh works in both directions (app → widget, widget → app). Verified with a release build.
- ✅ **Spike 5**: the calendar uses `kalender` 0.34. Drag, resize and dropping from the unscheduled strip all work, with 6 widget tests.
- ⬜ **Spike 2**: iroh phone ↔ Mac sync. Not started. The LAN half can use the emulator ↔ Mac; the across-networks half needs a real phone on mobile data.
- ⬜ **Spike 4**: tray icon and notifications. Needs the Arch machine (waybar) and Xcode (macOS).

Next step: ask the user which to do, Spike 2 or starting M1 (core and CLI). Everything in the app is spike-quality; the real UI comes in M2 and M4.

## Repo layout

```
core/                 tt-core: Rust domain + SQLite (rusqlite bundled). Store, tasks, lists, tags, Inbox view
cli/                  `tt` CLI (add / ls / done / rm). Short ids = LAST 8 hex chars of the UUIDv7
app/                  Flutter app (android, linux, macos)
  rust/               tt_bridge crate: Dart API (src/api/, flutter_rust_bridge) + Kotlin API (src/kotlin.rs, UniFFI)
  lib/                main.dart (Inbox), calendar/spike_calendar.dart, widget_bridge.dart
  android/app/src/main/kotlin/dev/tickalt/
    core/tt_bridge.kt          GENERATED UniFFI bindings (scripts/gen-kotlin-bindings.sh)
    tick_alt/widget/           Glance widget, TtCore (DB path), CompleteTaskAction
  test/                       calendar widget tests;  integration_test/  on-device Inbox test
tools/uniffi-bindgen/ uniffi-bindgen pinned to the library's UniFFI version
scripts/              env.sh (toolchain env), gen-kotlin-bindings.sh
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
- Xcode isn't installed, so macOS Flutter builds fail and `flutter doctor` complains. That's expected.

## Working rules (from the user)

- **Small commits.** One logical change per commit; never commit large files or big mixed changes at once. Shrink screenshots before committing them (JPEG, about 400 px wide, into `plan/attachments/`).
- No `Co-Authored-By` trailers or "Generated with" footers in commit messages.
- Check a library's actual version (lockfile, crates.io, pub.dev, Google Maven) before using its API, and record new versions in `plan/02`. Use the `context7` MCP for docs.
- Write readable multi-line shell commands with `\` continuations. Use `git -C <path>` outside the repo root.
- Plan before implementing. Interview the user when unsure; anything listed in `plan/11` is the user's call.
- Downloads, installs, licence acceptances and anything outward-facing need the user's OK first.
- Keep `plan/` updated as decisions are made: tick the roadmap and log spike results.

## Plugins to use (installed on the account 2026-11-24, not loaded in the session that wrote this)

- **Dart and Flutter** (official): Dart MCP server for analysis, tests and running the app; also Flutter skills such as `flutter-add-widget-test` and `flutter-add-integration-test`.
- **kotlin-lsp**: Kotlin diagnostics, so widget and Gradle Kotlin errors show up before a 2-minute Gradle build.
- **Android Emulator QA**: launch the app, inspect the UI, take screenshots and collect logs on the emulator, instead of hand-written `adb` scripts.
