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

## Spike 1: Rust core + flutter_rust_bridge ✅ Android · ⏸ macOS · ⏸ Hyprland

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

**Still to do for Spike 1:**
- macOS: needs Xcode.
- Hyprland: needs the Arch machine. Run `flutter build linux` there and check native Wayland (no XWayland).

## Spike 2: iroh phone ↔ Mac
Not started. Needs a real phone on mobile data for the "across networks" half. LAN can be tried emulator ↔ Mac first.

## Spike 3: Glance widget → Rust core via UniFFI
Not started.

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
