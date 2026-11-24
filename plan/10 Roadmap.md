---
tags: [project/tick-alt, plan, roadmap]
---

# 10 Roadmap

Back to [[00 Overview]].

Each milestone ends with something usable. Building the core first means the CLI is usable before any UI exists.

## M0: Decisions and spike
- [x] Choose the stack ([[02 Tech Stack Options]]): Option A, Rust core + Flutter + Kotlin Glance widget.
- [x] Choose the UI style ([[09 UI Style Options]]): Soft with Material tag chips.
- [ ] Confirm current versions: iroh, Flutter, flutter_rust_bridge, UniFFI, Glance.
- [ ] Spike 1: Rust core library plus `flutter_rust_bridge` running on an Android phone, macOS and Hyprland.
- [ ] Spike 2: iroh connects phone ↔ Mac on LAN and across networks (phone on mobile data).
- [ ] Spike 3: Glance widget calls the Rust core through UniFFI and completes a task.
- [ ] Spike 4: tray icon and notification on Hyprland (waybar) and macOS.
- [ ] Spike 5: Flutter week grid with drag and resize (evaluate the `kalender` package against a custom build).

**Exit:** the stack is confirmed, or we switch to Option B.

## M1: Core and CLI
- [ ] SQLite schema and migrations. Model for tasks, lists, tags (nested), checklist and subtasks.
- [ ] Inbox rule view.
- [ ] Filter AST, SQL compiler and tests.
- [ ] Recurrence (fixed and after-completion) and the completion history.
- [ ] Reminder schedule computation.
- [ ] Op-log and HLC from day one, even before sync exists.
- [ ] The `tt` CLI: add, ls, done, tags, filters.

**Exit:** I can manage tasks from the terminal on Arch.

## M2: Desktop app (Mac and Arch)
- [ ] Tray daemon with socket API and systemd / LaunchAgent setup.
- [ ] Three-pane UI: sidebar, list, detail. Tag tree, lists, filter builder, Completed and Trash.
- [ ] Desktop notifications with Done and Snooze.
- [ ] Packaging: PKGBUILD and a DMG.

**Exit:** usable daily on one machine.

## M3: P2P sync (desktop ↔ desktop)
- [ ] Device identity, QR / text pairing, trusted peers.
- [ ] Sync protocol: version vectors and op streaming. Compaction and snapshots.
- [ ] Property-based convergence tests and a network simulation.
- [ ] Tray sync status.

**Exit:** Mac ↔ Arch stay in sync.

## M4: Android app and widget
- [ ] Flutter UI on Android: lists, tags, filters, detail, quick add, share target.
- [ ] Glance widget: config, scrolling list, checkbox complete, **+** quick add, midnight refresh.
- [ ] AlarmManager reminders with notification actions.
- [ ] WorkManager sync, QR scanner pairing.
- [ ] APK release via CI.

**Exit:** phone ↔ desktop sync, and the widget is on the home screen.

## M5: Calendar and Google
- [ ] Week, Day and Month views (desktop and Android). All-day row, current-time line, overlap layout.
- [ ] Drag to schedule and resize. Unscheduled sidebar from a filter.
- [ ] Google OAuth (desktop loopback PKCE, Android AuthorizationClient). Keychain storage.
- [ ] Events in: calendar picker, incremental `syncToken` polling, cache.
- [ ] Tasks out: "Tasks" calendar, deterministic event ids, idempotent push and reconcile.
- [ ] Put the OAuth app in production to avoid 7-day token expiry.

**Exit:** the calendar replaces TickTick's for daily time-blocking.

## M6: Import and switch-over
- [ ] TickTick CSV importer (dry run first). Recreate the 4 filters.
- [ ] Use only Tick Alt for a week and fix whatever hurts.
- [ ] Back up and export the DB (a JSON dump), so I never get locked in.

**Exit:** TickTick uninstalled. 🎉

## Later (not v1)
- Natural-language quick add.
- Kanban view.
- Markdown notes.
- Global quick-add popup.
- Command palette.
- Optional homelab relay or always-on peer.
- DB encryption.
