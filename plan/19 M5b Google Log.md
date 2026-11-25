---
tags: [project/tick-alt, plan, gcal, log]
---

# 19 M5b Google Log

Back to [[00 Overview]]. Design: [[06 Google Calendar]]. Calendar context: [[18 M5 Calendar Log]].
Work happens on branch `worktree-google-calendar` (a git worktree under `.claude/worktrees/`).

## Slices
1. ✅ **Cache and mappings, no network** (2026-11-25). `core/src/gcal.rs`, schema **v8** (`008_gcal.sql`).
   - `gcal_calendar` and `gcal_event` are plain per-device tables, not ops, so nothing syncs over P2P.
   - `task_event_id`: UUID in base32hex (26 chars, a–v and 0–9) for deterministic event ids.
   - `task_event_body`: a task as a Google event (✓ prefix when done, all-day uses `date` with an exclusive end, `tt_task_id` in private extended properties). Repeats are **not** expanded yet.
   - `GcalEventUpdate::parse`: one `events.list` item to upsert or remove. Hides cancelled, working-location, out-of-office, focus-time and our own pushed events.
   - `Store::gcal_set_calendars`, `gcal_select`, `gcal_calendars`, `gcal_sync_token`, `gcal_apply` (full resync clears first; the token is saved in the same transaction), `gcal_events(from, until, tz)`.
2. **HTTP client and OAuth (desktop)**: loopback + PKCE, `events.list` with `syncToken` and 410 handling, token storage in the OS keychain. **Needs the user's Google Cloud project** (Calendar API enabled, a *Desktop app* OAuth client id, consent screen In production). Not started.
3. **Events in on the calendar**: expose `gcal_events` through the `api!` table, draw them read-only in the Soft style, a Settings page to pick calendars.
4. **Tasks out**: the "Tasks" calendar, push and reconcile, backoff. Decide how repeats go to Google (recurring event vs a window of occurrences).
5. **Android**: Google Identity AuthorizationClient, polling in the WorkManager job.

## Open
- Schema v8 means an older build refuses a v8 database (per device, so only a concern if a DB file is copied between machines).
- Tokens go in the keychain (libsecret on Linux, Keychain on macOS); check the crate and its version before use.
