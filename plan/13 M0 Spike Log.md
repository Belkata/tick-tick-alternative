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

> [!note] Flutter 3.50 moved Material out of the SDK
> `package:flutter/material.dart` is deprecated. Use the `material_ui` package (1.11.1) instead. Every new Dart file should import `package:material_ui/material_ui.dart`.

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

## Spike 5: Flutter week grid (drag and resize)
Not started. Can be done on Android now, compared against `kalender` 0.34.0.
