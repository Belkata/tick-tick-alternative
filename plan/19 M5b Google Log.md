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
2. ✅ **HTTP client and OAuth (desktop)** (2026-11-26), new crate `google/` (`tt-google`). Tested against a fake local Google, so no real sign-in has run yet.
   - `oauth`: loads Google's `client_secret_*.json` (kept in `~/.config/utre/google-client.json`, never in the repo), PKCE S256 (RFC 7636 test vector), a one-shot `127.0.0.1` loopback listener that checks `state`, code exchange and refresh. Scopes: `calendar.readonly` and `calendar.app.created`.
   - `tokens`: `TokenStore` trait with `FileTokenStore` (owner-only file, atomic write). **Deviation from plan/06:** tokens are in a file for now, not the OS keychain. The `keyring` crate's 4.x API needs checking first; the trait lets it slot in later.
   - `session`: `sign_in(open_browser)`, `access_token` (refresh 60 s early, one refresh at a time; a revoked refresh token clears the tokens and answers `SignedOut`), `get_json` (one forced refresh on 401, back-off on 429, rate-limit 403 and 5xx, 410 becomes `Gone`).
   - `calendar`: `list_calendars`, `sync_events` (window -30/+365 days with `singleEvents=true`, then `syncToken`; 410 falls back to a full fetch), `sync_calendars(session, store, now)` fills the cache for selected calendars.
   - Versions (checked 2026-11-26): reqwest 0.13.5 (already in the lockfile via iroh), sha2 0.11, base64 0.23, getrandom 0.4, url 2.5, tokio 1.54.
   - Next for this slice: wire it into the daemon (Sign in / Sign out, a poll every 5 minutes and on focus), and try a real sign-in with the client file.
3. **Events in on the calendar**: expose `gcal_events` through the `api!` table, draw them read-only in the Soft style, a Settings page to pick calendars.
4. **Tasks out**: the "Tasks" calendar, push and reconcile, backoff. Decide how repeats go to Google (recurring event vs a window of occurrences).
5. **Android**: Google Identity AuthorizationClient, polling in the WorkManager job.

## Open
- Schema v8 means an older build refuses a v8 database (per device, so only a concern if a DB file is copied between machines).
- Tokens go in the keychain (libsecret on Linux, Keychain on macOS); check the crate and its version before use.
