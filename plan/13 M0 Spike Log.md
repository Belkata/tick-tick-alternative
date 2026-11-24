---
tags: [project/tick-alt, plan, m0]
created: 2026-11-24
---

# 13 M0 Spike Log

Back to [[00 Overview]] · [[10 Roadmap]]. Results of each M0 spike as it runs.

## Toolchain on the Mac (2026-11-24)

| Tool | Version | Source |
|---|---|---|
| Rust | 1.100.0 | `brew install rustup` (keg-only: add `/opt/homebrew/opt/rustup/bin` to `PATH`) |
| Flutter / Dart | 3.50.1 / 3.14.0 | `brew install --cask flutter` |
| flutter_rust_bridge codegen | 2.13.0 | `cargo install --locked` |
| Android Studio | 2026.2.2 | Homebrew cask (its bundled JBR 25 is used as `JAVA_HOME`) |
| Android SDK | platform 36, build-tools 37.0.0, NDK 28.2.13676358 | `sdkmanager` (NDK version is Flutter's default) |
| Emulator | `tt_pixel`, Android 16 (API 36), arm64 | `avdmanager` |
| Xcode | **not installed** | macOS builds skipped for now (your call, 2026-11-24) |

> [!warning] Flutter 3.50 deprecates `flutter/material.dart`, but don't switch to `material_ui` yet
> - Flutter 3.50 marks `package:flutter/material.dart` as deprecated in favour of the `material_ui` package (1.11.1).
> - `material_ui` is a **separate copy** of Material, not a re-export. Packages still built on `flutter/material` (kalender, and most others today) can't see its `Material` ancestors or its `Theme`. Mixing them caused "No Material widget found" errors, and kalender ignored the app's teal theme.
> - **Rule for now:** import `package:flutter/material.dart` with `// ignore: deprecated_member_use`. Revisit once kalender moves to `material_ui`.

## Toolchain on the Arch laptop (2026-11-24)

EndeavourOS, kernel 7.3.5-zen, Hyprland with waybar (tray module on) and dunst.

| Tool | Version | Source |
|---|---|---|
| Rust | 1.100.0 | rustup's installer (`sh.rustup.rs`, `--no-modify-path`; `scripts/env.sh` adds `~/.cargo/bin`) |
| Flutter / Dart | 3.50.1 / 3.14.0 | `git clone --depth 1 -b 3.50.1` of flutter/flutter into `~/flutter` (`env.sh` adds it to `PATH`) |
| clang, cmake, ninja, pkgconf, gtk3 | clang from pacman; the rest were already installed | `sudo pacman -S --needed clang` |
| Android SDK / Studio | **not installed** | not needed for Spikes 1 and 4 |

- `flutter doctor` warns "unknown channel", because a tag checkout has no branch. It's harmless; ignore it.
- `/etc/localtime` is a symlink (Europe/Sofia), so `tt` finds the zone without `$TT_TZ`.

## Spike 1: Rust core + flutter_rust_bridge ✅ Android · ⏸ macOS · ✅ Hyprland

**What was built:**
- `core/` (`tt-core`): SQLite store (rusqlite, bundled), task model, lists, nested tags, and the derived Inbox view. 7 unit tests.
- `cli/` (`tt`): `add`, `ls [--all]`, `done`, `rm`. Short ids are the **last** 8 hex characters of the UUIDv7, because the first ones are a timestamp and collide.
- `app/rust/` (`tt_bridge`): an opaque `TaskDb` (open, inbox, add, complete) behind a `Mutex`, since SQLite connections aren't `Sync`.
- `app/`: a bare Inbox screen and an integration test.

**Results:**
- `flutter test integration_test/inbox_test.dart` on the emulator: ✅ passed. Dart → Rust → SQLite add and complete both work.
- Release APK (arm64 only): **19.6 MB**. It starts in under 4 s on the emulator.
- Data persists across an app reinstall (`install -r`).
- The first Gradle build takes about 6 minutes, because cargokit compiles the core for 4 Android ABIs. After that it takes about 25 s.

![[spike1-android.jpg|300]]

**Learned:**
- Use a **release or profile build** to try the app on the emulator without `flutter run` attached. A debug APK started on its own sat on the splash screen for over a minute and triggered an ANR.
- The emulator's Gboard shows a "Try out your stylus" tutorial on first focus, and it swallows `adb input text`. Dismiss it once.
- `flutter_rust_bridge_codegen integrate` puts the crate at `app/rust`. I kept it there and added it to the root Cargo workspace. [[03 Architecture]] planned `core-ffi/`; that folder now only needs to hold the UniFFI crate for the widget (Spike 3).
- Debug builds compile the core for armv7, arm64, x86_64 and i686. Release builds should pass `--target-platform android-arm64` unless an old phone needs armv7.

**Hyprland (Arch laptop, 2026-11-24): ✅**
- `flutter build linux --release` took about 5 minutes the first time, since cargokit compiles the core once for x86_64. The bundle is **52 MB**: `libtt_bridge.so` 29 MB, `libflutter_linux_gtk.so` 17 MB and `libapp.so` 6 MB.
- `hyprctl clients` shows `xwayland: 0`: GTK runs it as a **native Wayland** client. Impeller uses its OpenGL ES backend.
- `flutter test integration_test/inbox_test.dart -d linux`: ✅ passed. Add and complete both work through Dart → Rust → SQLite.
- `flutter analyze` is clean and the 6 calendar widget tests pass.
- The app keeps its DB and key in `~/.local/share/dev.tickalt.tick_alt/` (path_provider's app support directory).
- Harmless log noise: `Atk-CRITICAL atk_socket_embed` and a cursor-theme message from GDK.
- ⚠️ The Wayland app id is `dev.tickalt.tick_alt` (from `APPLICATION_ID`) and the title is `tick_alt`. [[08 Desktop App]] wants the window class `tick-alt` for window rules. Set it in M2.

![[spike1-hyprland.jpg|300]]

**Still to do for Spike 1:**
- macOS: needs Xcode.

## Spike 2: iroh phone ↔ Mac ✅ emulator ↔ Mac · ⚠️ relay flaky on Android · ⏸ phone on mobile data

**What was built** (spike quality; M3 replaces the protocol):
- `sync/` (`tt-sync`): an iroh 1.5.0 endpoint with the `N0` preset (n0 relays + n0 DNS address lookup).
  - The Ed25519 key is stored next to the DB (`tt.key`, mode 0600), so the node id survives restarts.
  - ALPN `tickalt/spike/0` and one bidirectional stream: the dialer sends `Hello{device}` as JSON and the server answers with its open tasks.
  - The dialer then reports the selected path (direct or relay) when the reply arrives, waits up to 4 s for hole punching, and reports it again.
  - A ticket is `tt1` + hex of the `EndpointAddr` JSON: node id, relay URL and direct addresses. It's about 600–900 characters and pasteable, but too long to type: real pairing needs QR (M3).
  - 3 tests, including a pull between two nodes over loopback with relays off.
- CLI: `tt sync serve` prints the ticket and serves; `tt sync pull <ticket>` dials it and prints the tasks and paths.
- Bridge: an opaque `SyncPeer` that **owns its own tokio runtime** (iroh needs one; frb's worker threads don't provide it). Its methods `block_on` from frb's thread pool.
- App: a Sync tab with the node status, its addresses, its ticket (copy button), a peer-ticket field, Pull, a result card and a "Served" list. Tickets and pull results are also printed to logcat (`TT_TICKET`, `TT_PULL`) for testing.

**Results (release APK, Android 16 emulator, Mac on Wi-Fi):**

| Test | Result |
|---|---|
| Mac ↔ Mac (two CLI nodes) | ✅ 2 tasks in 463 ms. Relay first, then **direct** after hole punching |
| Phone pulls from Mac | ✅ 3 tasks in **192 ms**, direct from the first packet (rtt 4 ms). The emulator can reach the Mac's LAN address |
| Mac pulls from phone | ✅ 3 tasks in 724 ms. **Relay first** (rtt 436 ms), because the Mac can't reach the emulator's private `10.0.2.x` addresses. Then **direct** after hole punching (rtt 16 ms) |
| Phone node id after force-stop + relaunch | ✅ the same id |
| Phone "Served" list | ✅ shows the Mac's pull |
| Phone gets a home relay | ⚠️ **1 of 4 launches** within 10 s. When it misses, it never connects, even after 40 s. The Mac connected every time (5/5) |

![[spike2-sync.jpg|260]]

**Costs:**
- The APK grew from 19.6 MB to **35.4 MB** (arm64 release) with iroh (QUIC, rustls/ring, relay client, DNS, portmapper).
- A clean release Gradle build took about 15.5 minutes, because cargokit rebuilt everything with the new crates.

**Learned:**
- **iroh 1.x has no built-in LAN discovery** (no mDNS). Only the pkarr/DNS lookups and the in-memory one are in core. LAN works anyway because the ticket carries the direct addresses. Automatic peer finding on the LAN needs a separate discovery crate later (M3), or the paired peers' last-known addresses.
- **The release manifest had no `INTERNET` permission.** The Flutter template only adds it to the debug and profile manifests, so a release build would have silently had no network. It's now in `src/main/AndroidManifest.xml`.
- The path API is `Connection::paths()` → `Path::{is_selected, is_relay, is_ip, rtt}`; `Endpoint::online()` waits for a home relay. `iroh::protocol::Router` + `ProtocolHandler` serve an ALPN.
- The Android QA plugin's helper scripts need Python ≥ 3.10. `scripts/env.sh` now puts Homebrew's `python3` (3.14) ahead of macOS's 3.9.

**Still to do:**
1. **Find out why the Android relay sometimes never connects.** Route iroh's `tracing` logs to logcat and look at the relay-dial and DNS errors. One guess to check: iroh's DNS resolver on Android may not use the system's DNS servers (there is no `/etc/resolv.conf`). This matters for the across-networks case, where relays are the fallback.
2. **Across networks:** a real phone on mobile data ↔ the Mac on Wi-Fi. This needs the relay to be reliable first.

## Spike 3: Glance widget → Rust core via UniFFI ✅ (emulator, release build)

**What was built:**
- `app/rust/src/kotlin.rs`: a UniFFI API (`WidgetDb`: open, inbox, add, complete) in the **same library** as the Dart API. The APK carries one `libtt_bridge.so`, so the core and SQLite are bundled once. The Kotlin bindings come from `scripts/gen-kotlin-bindings.sh`, using a `uniffi-bindgen` pinned to the library's UniFFI version (0.32.2).
- `TaskListWidget` (Glance 1.2.0): a 4×2 Inbox widget.
  - The title and **+** open the app.
  - Ticking a checkbox runs `CompleteTaskAction`, which completes the task in the Rust core and redraws the widget.
- The widget and the app each open their own connection to `filesDir/tt.db`, the same file Flutter's `getApplicationSupportDirectory()` uses. SQLite's WAL mode handles both writers.
- App → widget: a `tick_alt/widget` method channel. Refresh runs after every add or complete, and a **pin** button asks the launcher to place the widget.
- Widget → app: the Inbox reloads when the app comes back to the foreground.

**Automated checks:** `./gradlew app:connectedDebugAndroidTest` on the emulator ran 2 Kotlin tests, both passing:
- add, list and complete through UniFFI;
- two connections to one file see each other's writes.

**Learned the hard way:**
1. **Applying the Compose compiler is required, and is easy to get silently wrong.** The Flutter template sets `android.builtInKotlin=false`, so `app/build.gradle.kts` must apply `org.jetbrains.kotlin.android` and then `org.jetbrains.kotlin.plugin.compose`, both before the Flutter plugin.
   - My first edit didn't apply. It still compiled once `buildFeatures.compose` was removed, but failed in confusing ways: a Kotlin "couldn't inline" crash, then `NoSuchMethodError: provideContent(...Function0...)` at runtime.
   - Check with `./gradlew :app:dependencies --configuration kotlinCompilerPluginClasspathRelease`, which must list `kotlin-compose-compiler-plugin-embeddable`.
2. **R8 full mode (the AGP 9 default) breaks WorkManager in release builds.** Glance runs on it. Full mode no longer keeps default constructors, so the release build crashed on start (Room's `WorkDatabase_Impl`), then stuck on "loading" (`OverwritingInputMerger`).
   - The keep rules are in `app/proguard-rules.pro`.
   - Debug builds aren't shrunk, so **always test the widget with a release build**.
3. Flutter's `integration_test` plugin pins `androidx.test:runner` to 1.3.0 on the debug classpath, so don't declare a newer runner.
4. JNA needs keep rules too (`com.sun.jna.**`, `dev.tickalt.core.**`). Glance's `ActionCallback` classes need their no-arg constructor kept as well, or ticking a box silently does nothing (`NoSuchMethodException ... CompleteTaskAction.<init>`).
5. **Glance calls `provideGlance` once per session.** `update()` only recomposes, so a list loaded before `provideContent` never changes. The widget keeps a `version` in its Glance state; `refreshAll()` bumps it, and `produceState(version)` reloads from the core.
6. The emulator's Pixel launcher **removes the widget on every `adb install -r`**, so re-pin it after reinstalling. The emulator's Gboard and Google sign-in screens also pop up over the app now and then.

**Home-screen result (release APK, Android 16 emulator):**
- **Pin:** the in-app button opens the launcher's own "Add to home screen" sheet, and the widget appears.
- **Widget → core → app:** ticking "Renew passport" on the widget completed it in the Rust core. Both placed widgets went from 3 to 2 at once, and the app showed the same list when brought back.
- **App → core → widget:** adding "Water the plants" in the app showed up on both widgets within a second.

![[spike3-widget.jpg|260]]

**Still to do (M4):**
- A config screen per widget, to choose a list or filter.
- An in-widget quick-add sheet.
- A midnight and timezone refresh.
- Showing due dates on rows.

## Spike 4: Tray and notifications (Hyprland/waybar, macOS)
Not started. Needs the Arch machine. The macOS half needs Xcode.

## Spike 5: Flutter week grid (drag and resize) ✅ with kalender 0.34.0

**What was built:** `app/lib/calendar/spike_calendar.dart`, a Calendar tab with in-memory sample data:
- Day, 3-day, Week and Month views, with a Today button.
- Read-only "Google" events in grey and editable tasks in teal.
- An **Unscheduled** strip of chips that can be long-pressed and dragged onto the grid.

**Automated checks** (`flutter test test/calendar_spike_test.dart`, phone-sized viewport): 6 tests, all passing.

| Check | Result |
|---|---|
| Long-press drag moves a task to a new time | ✅ |
| Tap selects a task; dragging the end handle makes it longer (1 h → 2 h) | ✅ |
| Read-only events can't be dragged, and long-pressing one creates nothing | ✅ (needed a fix, see below) |
| Dragging an unscheduled chip onto the grid schedules it in the right column, within 30 min of the drop point | ✅ (needed a fix, see below) |
| Switching Day, 3 days, Week and Month throws nothing and still shows today | ✅ (needed a fix, see below) |

On the emulator, a release build shows the current-time line, overlapping events, all-day events in Month, and the theme colour.

![[spike5-3day.jpg|260]] ![[spike5-month.jpg|260]]

### kalender vs a custom grid: verdict → **use kalender**
It already does what we would otherwise spend weeks building:
- All four views we need.
- Snapping, and touch long-press drag (mouse drag on desktop).
- Resize handles that adapt to touch or mouse.
- The current-time line, overlap layout and an all-day header lane.
- Timezone-aware UTC storage.
- Replaceable tile builders, so the Soft style can be applied.

It's MIT-licensed, with an active release cadence (0.34.0 on 2026-11-17).

**Costs and workarounds found:**
1. **No public "drop from outside" API.** The unscheduled sidebar works by sending kalender's own `Reschedule` payload. That needs an import from `package:kalender/src/...` (not exported), and the drop arrives as `onEventChanged` with an event the controller doesn't know yet. It works, but it can break on any minor version. Consider asking upstream to export it.
2. **The drop time comes from the dragged widget's top-left corner**, not the finger. Outside draggables must use `pointerDragAnchorStrategy`, or the drop lands a column to the left and some minutes early.
3. **A long-press on a read-only event fell through** to the grid and created a new task under it. Read-only tiles now swallow long-press.
4. **Selection is the app's job.** `onEventTapped` must call `selectEvent`, or resize handles never appear.
5. **Changing the view configuration keeps the old first day**, so switching 3-day → Week showed *last* week. The fix is to jump to today after the switch.
6. Breaking changes land in minor versions until 1.0. Pin `^0.34.0` (the caret already stays within 0.34.x) and read `MIGRATION.md` on each bump.

**Polish for M5 (not blockers):**
- 15-minute events are too thin to read, so tiles need a minimum height.
- Month weekday headers wrap ("Wednesd/ay"), so use short names on phones.
- On touch, the default resize handles are nearly invisible. Draw our own in the Soft style.
- The 3-day view pages in fixed blocks (it opened on Sun–Tue). TickTick starts it at today; try `freeScroll`.

**Not covered:** the desktop (mouse/trackpad) feel, which needs the Linux or macOS build.
