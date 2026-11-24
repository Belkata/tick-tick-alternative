---
tags: [project/tick-alt, plan, gcal]
---

# 06 Google Calendar

Back to [[00 Overview]].

**Decision:** *events in, tasks out*.
- Google events show **read-only** in the in-app calendar.
- Timed tasks are pushed **one-way** to a dedicated Google calendar.

## Events in (Google to app)
- In Settings you choose which of your Google calendars to display.
- Each device fetches events **itself**.
  - Events are cached in a local `gcal_event` table and are **not** sent over P2P.
  - That keeps the sync log clean, and every device stays correct even if it hasn't seen a peer for a while.
- **Incremental sync:**
  1. The first run calls `events.list` with a window (−30 to +365 days) and `singleEvents=true`.
  2. Later runs use the `syncToken`. They receive only changes and deletions.
  3. A **410 Gone** response means the token is invalid: drop the cache and do a full resync.
- **When to poll.** Push channels (`events.watch`) need a public HTTPS server, so they're not an option.
  - Desktop polls every 5 minutes, plus on window focus.
  - Android polls on app open and in the periodic WorkManager job.
- **Display:**
  - Events use the colour of their Google calendar.
  - They aren't draggable. Clicking one shows its details and an "Open in Google Calendar" link.
  - Working-location and out-of-office events are hidden.

## Tasks out (app to Google)
- On first setup the app creates a secondary calendar called **"Tasks"**, or you pick an existing one.
- **Pushed:** tasks that have a time (or are all-day, which is configurable) and are `open`. Completed tasks are also pushed if "show completed" is on.
- **Fields:**
  - Event title = task title. A `✓` prefix is added when the task is done, if that option is on.
  - Description = notes plus an app deep-link.
  - Start, end, all-day and the tag colour are mapped onto the closest of Google's 11 event colours.
  - Repeating tasks are pushed **only as the next occurrence**, not as a Google recurring event. That avoids drift between the two different recurrence engines.
- **Deterministic event ids**
  - `event.id = base32hex(task_uuid)`. Google lets the client choose ids with the characters a–v and 0–9, 5–1024 characters long.
  - So **any device can push the same task without creating duplicates**:
    - `insert` returns 409 if the event already exists, then the device does `update`.
    - A task deleted or unscheduled in the app means the event is deleted.
  - The task id is also stored in `extendedProperties.private.tt_task_id` for safety.
- **Which device pushes?** Whichever device makes the change, *if* it's online and signed in. On every successful sync, each device reconciles recent task changes against Google. Writes are idempotent, so concurrent pushes are harmless.
- **Edits made in Google** to "Tasks" events are **overwritten** on the next push, since it's one-way. The UI says so in Settings.

## Auth (OAuth 2.0, no backend)
- **Google Cloud project:** one personal project. The **Calendar API** must be enabled.
- **Desktop:** an OAuth client of type *Desktop app*. Authorization Code flow with PKCE and a loopback redirect `http://127.0.0.1:<random port>`, which opens the system browser.
- **Android:** an OAuth client of type *Android* (package name plus SHA-1).
  - Use the **Google Identity AuthorizationClient** (Credential Manager family) to request scopes. Native Kotlin passes tokens to the core.
- **Scopes (least privilege):**
  - `calendar.readonly` (or `calendar.events.readonly`) to read events.
  - `calendar.app.created` to create and manage only the app's own "Tasks" calendar. If that scope's flow turns out to be awkward, fall back to `calendar.events`.
- **Tokens:**
  - Stored per device in the OS keychain: macOS Keychain, libsecret / Secret Service on Linux (KeePassXC or gnome-keyring on Hyprland), Android Keystore-encrypted DataStore.
  - Tokens are **not** synced over P2P. Each device signs in once.

> [!warning] The 7-day refresh token trap
> If the OAuth consent screen stays in **Testing** mode, refresh tokens expire after **7 days**, and every device would need to sign in again weekly.
> - **Fix:** set the app to **In production** and leave it unverified. Google shows an "unverified app" warning once, which you click through. That's acceptable for personal use, which stays far below the 100-user cap.
> - Re-check Google's current policy for Calendar scopes at M4.

## Fallback for events in
If OAuth on some device is a hassle, it can read a calendar's **secret iCal address** (Google Calendar settings › *Secret address in iCal format*) with no OAuth at all. It's read-only and refreshes more slowly, but it's fine for display.

## Edge cases
- **Time zones:** store UTC plus IANA zone. All-day tasks use `date` (not `dateTime`) in Google.
- **Rate limits:** exponential backoff on 403 `rateLimitExceeded` and on 429.
- **Signed out or revoked:** the calendar shows a banner. Tasks keep working.
