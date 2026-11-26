---
tags: [project/utre, plan]
status: draft
created: 2026-11-24
---

# Utre: Plan Overview

A personal, local-first task manager. It replaces the parts of TickTick I actually use and nothing more.

- **Android** app with a home-screen widget that shows a chosen list or filter.
- **macOS + Arch Linux (Hyprland)** desktop app.
- **Peer-to-peer sync** between all devices, with no cloud account.
- **Inbox** as a triage area where every new task lands.
- **Tags as a first-class concept**: nested, coloured, with icons, and openable as views.
- **In-app calendar** (Day, Week, Month, plus an unscheduled sidebar) that shows **Google Calendar events**. Timed tasks are pushed out to Google.

> [!info] Renamed to Utre (2026-11-26)
> The project was called "Tick Alt" until then. The logs (`plan/13`–`19`) keep the old names as written. What changed:
>
> | | Old | New |
> |---|---|---|
> | Product name | Tick Alt | Utre |
> | CLI | `tt` | `utre` |
> | Env vars | `TT_DB`, `TT_TZ`, `TT_SOCKET`, `TT_APP`, `TT_SNOOZE_SECS`, `TT_SYNC_PORT`, `TT_GOOGLE_CLIENT` | `UTRE_*` (the `TT_*` names still work as a fallback) |
> | Crates | `tt-core`, `tt-api`, `tt-sync`, `tt-daemon`, `tt-google`, `tt_bridge` | `utre-*`, `utre_bridge` |
> | Dart package | `tick_alt` | `utre` |
> | Android applicationId, Kotlin package | `dev.tickalt.tick_alt`, `dev.tickalt.core` | `dev.utre.utre`, `dev.utre.core` |
> | Linux app id, desktop entry, binary | `dev.tickalt.tick_alt`, `tick_alt` | `dev.utre.utre`, `utre` (launcher `utre-app` in the Arch package) |
> | macOS bundles | `Tick Alt.app`, `Tick Alt Daemon.app` | `Utre.app`, `Utre Daemon.app` |
> | LaunchAgent label | `dev.tickalt.daemon` | `dev.utre.daemon` |
> | systemd user unit | `tick-alt.service` | `utre.service` |
> | Arch package | `tick-alt-git` | `utre-git` |
> | Data dir | `~/.local/share/tick-alt/`, `~/Library/Application Support/tick-alt/` | `.../utre/` |
> | DB, node key | `tt.db`, `tt.key` | `utre.db`, `utre.key` |
> | Socket | `$XDG_RUNTIME_DIR/tick-alt.sock` | `$XDG_RUNTIME_DIR/utre.sock` |
> | Mac logs | `~/Library/Logs/tick-alt/` | `~/Library/Logs/utre/` |
> | Release files | `tick-alt-<v>-arm64.apk`, `tick-alt-<v>-macos.dmg` | `utre-<v>-...` |
>
> **Unchanged on purpose:** the sync ALPN `tickalt/sync/1` (and `tickalt/spike/0`) and the `tt1`/`tt2` ticket prefixes, so builds from before the rename still pair and sync.
>
> **Migrating by hand:** stop `tick-alt.service`, install and start `utre.service` (`scripts/install-user-service.sh` retires the old unit). On its first run the new daemon, CLI or app moves the old data directory to `utre/` and renames `tt.db`/`tt.key`; it refuses while the old daemon still answers on its socket. The phone has to uninstall the old app and install the new one (a new application id) and re-pair; the Mac treats `Utre.app` as a new app.

> [!important] Status
> Planning done; next is the M0 spikes.
> - **UI style decided:** Soft with Material tag chips. See [[09 UI Style Options]].
> - **Stack decided:** Rust core + Flutter + a Kotlin Glance widget (Option A). See [[02 Tech Stack Options]].
> - **Next:** M0 in [[10 Roadmap]]: confirm versions and run the five spikes.

## Plan files

| # | File | What's in it |
|---|------|--------------|
| 01 | [[01 Requirements & Scope]] | Interview decisions: what's in v1, what's out |
| 02 | [[02 Tech Stack Options]] | Languages and frameworks, pros and cons, recommendation |
| 03 | [[03 Architecture]] | Components, Rust core, how the platforms connect |
| 04 | [[04 Data Model]] | Tasks, lists, tags, filters, the Inbox rule |
| 05 | [[05 P2P Sync]] | Pairing, transport, conflict resolution, Android limits |
| 06 | [[06 Google Calendar]] | Events in, tasks out, OAuth, polling |
| 07 | [[07 Android App & Widget]] | Android app, Glance widget, reminders |
| 08 | [[08 Desktop App]] | Mac and Arch app, tray, CLI |
| 09 | [[09 UI Style Options]] | Style mockups to choose from |
| 10 | [[10 Roadmap]] | Milestones and order of work |
| 11 | [[11 Open Questions]] | Things still to decide |
| 12 | [[12 TickTick Import]] | One-time migration of existing data |
| 13 | [[13 M0 Spike Log]] | Toolchain, spike results and lessons |
| 14 | [[14 M1 Core Log]] | How the core was built; deviations from 04 and 05 |

## Guiding principles

1. **Local-first.** Every device has the full database and works offline. Sync is eventual.
2. **Small scope, done well.** No habits, Pomodoro, matrix, kanban or collaboration in v1.
3. **One core, thin UIs.** Data rules (the Inbox rule, filters, recurrence, sync) live in one shared Rust core. UIs only render and edit.
4. **No server of my own.** Devices talk directly to each other. Google is only an external calendar.

UI iteration log (overnight design rounds): [[20 UI Iteration Log]]
