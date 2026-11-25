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
   - Wired into the daemon (2026-11-26, `daemon/src/google.rs`): `GoogleStatus`, `GoogleSignIn`, `GoogleSignOut` and `GoogleSync` requests on the socket (the same control hook as sync), a fetch every 5 minutes while signed in, `--no-google`. The cache is also readable through the `api!` table (`gcal_calendars`, `gcal_select`, `gcal_events`). CLI: `tt google status | sign-in | sign-out | sync | calendars | select | events`.
   - **Real sign-in tested** (2026-11-26, throwaway daemon and DB): the browser flow worked, 14 calendars listed, one selected, its events fetched (35 cached) and the second fetch used the saved sync token. The token file is owner-only.
   - Still to do for this slice: the app's Google button and calendar picker (slice 3), "on window focus" fetch (the app calls `GoogleSync`), and the keychain for tokens.
3. ✅ **Events in on the calendar** (2026-11-26). Desktop only: the phone has no daemon, so it has no Google yet (slice 5).
   - Bridge (`TaskDb`): `googleAvailable`, `googleStatus`, `googleSignIn`, `googleSignOut`, `googleSync`, `gcalCalendars`, `gcalSelect`, `gcalEvents` (mirrors of `GoogleStatus`, `GcalCalendar`, `GcalEvent`; bindings regenerated).
   - Calendar (`app/lib/calendar/calendar_page.dart`): `GoogleEvent` is a read-only `KalenderEvent` (no drag, resize or drop), loaded with the tasks on every reload, so the daemon's fetches show live through the change stream. A paler tint than a task block, a hollow dot in Month. A tap opens a dialog (title, time, calendar name, "Copy link"); there is no "Open in Google Calendar" button because the app has no URL-launcher dependency yet.
   - Page (`app/lib/desk/google.dart`, sidebar entry "Google Calendar"): not set up / sign in (with a "copy the sign-in link" fallback) / signed in with "Fetch now", "Sign out" (asks first) and a switch per calendar (turning one on fetches at once).
4. **Tasks out**: the "Tasks" calendar, push and reconcile, backoff. Decide how repeats go to Google (recurring event vs a window of occurrences).
5. **Android**: Google Identity AuthorizationClient, polling in the WorkManager job.

## Open
- Schema v8 means an older build refuses a v8 database (per device, so only a concern if a DB file is copied between machines).
- **The app is unverified** (the user's call, 2026-11-26: fix later). Sign-in works now behind Google's "unverified app" warning (Advanced, then continue). Verification is Google's review process: https://support.google.com/cloud/answer/7454865. Check what it asks for the two scopes before starting; likely a verified domain (`utre.party` is live), the privacy policy and homepage already published, and a demo of the sign-in. Until then: the warning on every new device, and the 100-user cap (irrelevant here). Also check the consent screen is **In production**, or refresh tokens last 7 days.
- Tokens go in the keychain (libsecret on Linux, Keychain on macOS); check the crate and its version before use.
