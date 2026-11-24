---
tags: [project/tick-alt, plan]
status: draft
created: 2026-11-24
---

# Tick Alternative: Plan Overview

A personal, local-first task manager. It replaces the parts of TickTick I actually use and nothing more.

- **Android** app with a home-screen widget that shows a chosen list or filter.
- **macOS + Arch Linux (Hyprland)** desktop app.
- **Peer-to-peer sync** between all devices, with no cloud account.
- **Inbox** as a triage area where every new task lands.
- **Tags as a first-class concept**: nested, coloured, with icons, and openable as views.
- **In-app calendar** (Day, Week, Month, plus an unscheduled sidebar) that shows **Google Calendar events**. Timed tasks are pushed out to Google.

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
