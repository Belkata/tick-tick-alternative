---
tags: [project/tick-alt, plan, tech]
decision: option-a-rust-flutter
---

# 02 Tech Stack Options

Back to [[00 Overview]].

> [!success] Decision (2026-11-24)
> **Option A: Rust core + Flutter UI on Android, macOS and Linux + a Kotlin Glance widget.**
> - The core is Rust behind `flutter_rust_bridge` for the Flutter app, and behind UniFFI-generated Kotlin bindings for the widget.
> - The M0 spikes in [[10 Roadmap]] still have to prove it. If Flutter feels wrong there, fall back to **Option B**; the Rust core and the widget carry over unchanged.
>
> The rest of this note is the comparison that led to the decision.

## First: where does Rust actually help?

A task app is **not CPU-bound**. Lists of a few thousand tasks, SQLite queries and calendar rendering are all fast in any modern language, so Rust's raw speed is **not** the main reason to pick it.

Rust is still the right choice for the **core**, for these reasons:
- **The P2P ecosystem is Rust-native.** iroh 1.0 (QUIC, hole punching, relays, LAN discovery) and the CRDT libraries (Loro, Automerge, Yrs) are written in Rust.
- **One core, every platform.** The data model, Inbox rule, filter engine, recurrence, sync and Google mapping are written once. That core compiles for Android (arm64), macOS (arm64/x86) and Linux (x86_64).
- **Small, low-RAM background process.** This matters for the tray daemon and for Android battery.
- **Correctness.** The type system and no data races help a sync engine where subtle bugs corrupt data.
- **Costs:** slower compiles, a steeper learning curve, and an FFI boundary to each UI language.

Hard constraint, whichever stack: the **Android home-screen widget must be native Kotlin (Jetpack Glance)**. No cross-platform framework renders Android widgets. Flutter, Tauri and others only pass data to a native widget.

## Languages: pros and cons

| Language                    | Pros                                                                                                                                                                           | Cons                                                                                                                             | Role here                                                      |
| --------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------- |
| **Rust**                    | Fast and memory-safe. Best P2P and CRDT libraries (iroh, Loro). Compiles everywhere. Small binaries. UniFFI generates Kotlin and Swift bindings.                               | Steep learning curve, slow compiles. GUI frameworks are immature on Android.                                                     | **Core** (data, sync, filters, recurrence, Google client, CLI) |
| **Kotlin**                  | First-class on Android: Compose, Glance widgets, WorkManager, AlarmManager. Compose Multiplatform also targets desktop.                                                        | Desktop runs on the JVM (bundled JRE, about 100 MB and more RAM). Weaker on Wayland and tray (AWT).                              | **Android widget**, plus the Android app or the whole UI       |
| **Dart (Flutter)**          | One UI codebase for Android, macOS and Linux. Native Wayland via the GTK embedder. AOT-compiled and fast. Hot reload. Good calendar packages. `flutter_rust_bridge` is mature. | Doesn't look native (irrelevant for a custom design). Widgets still need Kotlin. Another language to learn.                      | Candidate **UI for all platforms**                             |
| **TypeScript (web)**        | Huge ecosystem. Excellent calendar libraries (FullCalendar, Schedule-X). Fastest UI iteration. Works with Tauri.                                                               | WebKitGTK on Linux has Wayland and NVIDIA rendering quirks and is slower than Chromium. Tauri on Android is the roughest target. | Candidate **desktop UI** (Tauri)                               |
| **Swift**                   | Best macOS feel (SwiftUI).                                                                                                                                                     | Mac only. Doesn't help Linux or Android.                                                                                         | Only in the "fully native" option                              |
| **C++ (Qt/QML)**            | Mature, runs on Android, Mac and Linux, native Wayland.                                                                                                                        | C++ complexity, licensing (LGPL/commercial). Widgets still need Kotlin. Poor fit with a Rust core.                               | Not recommended                                                |
| **Go (Fyne/Wails)**         | Simple language, fast builds.                                                                                                                                                  | Weak mobile story. iroh has only an unofficial Go port. Small CRDT ecosystem.                                                    | Not recommended                                                |
| **C# (.NET MAUI/Avalonia)** | Avalonia runs on desktop and Android.                                                                                                                                          | MAUI has no Linux support. Avalonia's mobile is younger. Small P2P ecosystem.                                                    | Not recommended                                                |

## Stack options (whole-system combinations)

### Option A: Rust core + Flutter UI everywhere + Kotlin widget ⭐ recommended
- **Android, macOS and Linux all share one Flutter UI.** The Rust core sits behind `flutter_rust_bridge`.
- The widget is Kotlin with Glance. It calls the same Rust core through UniFFI-generated Kotlin bindings, or reads the shared SQLite file.
- ✅ The calendar (drag and resize grid) is **built once** for phone and desktop.
- ✅ Native Wayland on Hyprland. The tray works through `tray_manager` (AppIndicator / StatusNotifierItem, which shows in waybar).
- ✅ Small AOT binaries and hot reload.
- ❌ Dart is a third language, and two FFI bindings (Dart and Kotlin) come out of the same core.
- ❌ It doesn't look like a native Mac app. That doesn't matter much, since we're designing our own look.

### Option B: Rust core + Kotlin/Compose Android + Tauri 2 desktop (web UI)
- Android is fully native: Compose, Glance, WorkManager.
- The desktop is Tauri: the Rust backend links the core directly, with a TypeScript UI.
- ✅ The best-in-class web calendar library can be reused on the desktop. The desktop backend *is* the Rust core, so there's no FFI there.
- ✅ The Android side is as native as it gets.
- ❌ **Two UI codebases**, Compose and web. The calendar is built twice.
- ❌ WebKitGTK quirks on Wayland and NVIDIA, which may need env-var workarounds.

### Option C: Rust core + Compose Multiplatform (Android + desktop JVM)
- One Kotlin UI for Android and desktop. The core goes through UniFFI, whose Kotlin bindings work on both Android and the JVM.
- ✅ One UI language. Glance lives in the same codebase. The calendar is built once.
- ❌ The JVM desktop means a bigger installer and more idle RAM.
- ❌ **On Hyprland it runs through XWayland** unless JetBrains' native Wayland toolkit is ready. AWT's tray doesn't show in waybar, so a third-party SNI tray library is needed.

### Option D: Rust core + fully native UIs
- Kotlin/Compose on Android, SwiftUI on Mac, GTK4/libadwaita or Qt on Linux.
- ✅ The best platform feel and the smallest footprint.
- ❌ **Three UIs**, and the calendar is built three times. GTK has no drag-and-drop calendar widget. This is the slowest path for a solo developer.

### Rejected
- **Electron**: too heavy.
- **React Native**: no real Linux desktop support.
- **Pure-Rust GUI** (Slint, Dioxus, Iced, egui): Android support is immature, and the calendar would be hand-drawn anyway.
- **Go** and **C#**: see the language table.

## Comparison at a glance

| | A Flutter | B Compose + Tauri | C Compose MP | D Native ×3 |
|---|---|---|---|---|
| UI codebases | 1 (+ widget) | 2 (+ widget) | 1 (+ widget) | 3 (+ widget) |
| Calendar built | once | twice | once | three times |
| Hyprland / Wayland | native | native (WebKitGTK quirks) | XWayland (for now) | native |
| Tray on Hyprland (waybar) | ✅ | ✅ | ⚠️ needs a library | ✅ |
| Desktop install size | ~30–50 MB | ~10–20 MB | ~100+ MB | small |
| Languages | Rust, Dart, Kotlin | Rust, Kotlin, TS | Rust, Kotlin | Rust, Kotlin, Swift, C/Vala/Rust |
| Dev speed (solo) | high | medium | high | low |

## Recommendation

**Option A: Rust core + Flutter + a Kotlin Glance widget.**
- It's the only option that gives one UI codebase across all three platforms *and* native Wayland and tray support on Hyprland.
- Rust does the parts where it actually matters: sync, data integrity and the shared core.
- **Fallback:** if Flutter feels wrong after the first spike, switch to **Option B**. The Rust core and the widget carry over unchanged.

Before committing, a short **spike (milestone M0 in [[10 Roadmap]])** checks:
1. `flutter_rust_bridge` + iroh running on an Android phone.
2. Tray and notifications on Hyprland and on macOS.
3. A drag-and-resize week grid in Flutter.
4. The Glance widget calling the Rust core through UniFFI.

> [!warning] Versions to confirm at M0 (per my global rule: check real versions, not assumptions)
> - iroh 1.0.x (shipped 2026-06-15).
> - Flutter stable and `flutter_rust_bridge` v2.
> - Jetpack Glance 1.2.x (reported stable in Aug 2026, but sources conflict).
> - UniFFI.
> - Compose Multiplatform 1.12 (only if Option C is chosen).
