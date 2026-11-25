---
tags: [project/tick-alt, plan, requirements]
---

# 01 Requirements & Scope

Back to [[00 Overview]].

Decisions below come from the interview on 2026-11-24 and from looking at how my current TickTick is set up. That setup has an Inbox of 434 tasks, 2 lists, about 24 tags, filters named *Widget*, *Real Inbox*, *Unscheduled* and *Today + Real Inbox*, and a Google-synced week calendar used for daily time-blocking.

## In scope for v1

### Organisation
- **Inbox (triage)**
  - Every new task goes to Inbox.
  - A task **leaves Inbox automatically once it has a tag, a list or a date**. Inbox therefore always means "unprocessed".
  - Details: [[04 Data Model#Inbox rule]].
- **Flat lists**
  - A few user lists, such as Work.
  - No folders and no sections.
- **Tags (first-class)**
  - Nested tags (for example `theater/via-verde`). Filtering on a parent includes its children.
  - Every tag has a colour and an optional emoji or icon. These show on task rows and calendar blocks.
  - Any tag opens as a full view with grouping and sorting, just like a list.
- **Custom filters**
  - A visual builder with **AND / OR / NOT** and nested groups.
  - Fields: tags, lists, Inbox state, dates (due, start, overdue, none, relative ranges), priority, completed state and text.
  - Filters show in the sidebar, can be pinned, and are the source for widgets and the calendar's unscheduled sidebar.
- **Built-in smart lists**
  - Only **Completed** and **Trash** are built in.
  - *Today*, *Next 7 days* and similar views are ordinary user filters. See [[11 Open Questions]] on shipping them as presets.

### Tasks
- Title and description. Plain text is enough, since Markdown isn't a must-have. See [[11 Open Questions]].
- **Checklist items and nested subtasks**, as in TickTick:
  - Checklist items are lightweight.
  - Subtasks are full tasks, with a depth limit.
- **Priority**: None, Low, Medium or High.
- **Dates**: due date, optional start date, all-day or timed, and duration for calendar blocks.
- **Repeat rules**: daily, weekly, monthly and yearly with custom intervals and weekdays, an end date or count, and **repeat after completion**.
- **Reminders**: several per task, relative ("10 min before") or absolute. They fire as local notifications on phone and desktop.
- Completing, un-completing and deleting a task. Deleted tasks go to Trash.
- Quick add is a **plain title only**, with no natural-language parsing. It lands in Inbox, or in the list, tag or filter being viewed (a filter's requirements are applied, see [[04 Data Model]] Filters). Everything else is set in the detail view.

### Calendar
- **Week and Day** time grids, where you drag to move and drag edges to resize.
- **Month** grid.
- **Unscheduled sidebar**: drag dateless tasks onto the grid to time-block them. It can be sourced from any filter.
- All-day row and a current-time line. Overlapping items sit side by side.
- **Google Calendar: events in, tasks out.** See [[06 Google Calendar]].
  - Google events show read-only in the calendar.
  - Timed tasks are pushed one-way to a dedicated Google calendar.

### Platforms
- **Android** only, with no iOS planned.
  - A **home-screen widget** shows one chosen list or filter.
  - Tapping a checkbox completes the task.
  - A **+** button opens quick add, which saves to Inbox.
- **macOS and Arch Linux (Hyprland, Wayland)** desktop app.
  - A **tray / menu-bar icon** keeps it running in the background for sync and reminders.
  - A **CLI tool**, for example `tt add "Buy milk"` and `tt ls <filter>`.

### Sync and data
- **Pure peer-to-peer**: devices sync when they're online at the same time, over LAN or the internet. There's no always-on node, but the desktop tray app makes that overlap common.
- **One-time import** from TickTick. See [[12 TickTick Import]].

## Out of scope (explicitly declined)

| Feature | Note |
|---|---|
| Kanban / board view | Not now. The data model shouldn't block it later. |
| Pomodoro / focus timer | — |
| Habits | — |
| Eisenhower matrix | — |
| Natural-language quick add | Declined for v1. Could come later. |
| Folders and sections | Flat lists only |
| Today, Next 7 Days and All as built-ins | Built as filters instead |
| Multi-day (3-day) and agenda calendar views | Not selected |
| Global quick-add hotkey | Not selected (the CLI can be bound to a Hyprland key later) |
| Keyboard-driven command palette | Not selected |
| Collaboration, sharing, assignees, comments | Single user |
| Two-way Google event editing | Events are read-only in the app |
| iOS | Android only |
| Countdowns, sticky notes, location reminders, email-to-task, attachments, statistics, AI | Not needed |
| Always-on relay or homelab peer | Pure P2P chosen. Could be added later without protocol changes. |

## Success criteria for v1
1. I can stop using TickTick for a week without missing anything.
2. A task added on the phone shows on the Mac within seconds when both are on the same Wi-Fi, and after the next overlap when they're apart.
3. The widget always shows the right filter results, and completing a task from it takes one tap.
4. The week view shows my Google events and my timed tasks, and dragging a task onto it updates Google within a minute.
