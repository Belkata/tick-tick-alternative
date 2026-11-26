---
tags: [project/utre, plan, android]
---

# 07 Android App & Widget

Back to [[00 Overview]].

## App (Flutter UI, Option A)

**Screens**
- **Sidebar / drawer:** Inbox (with count), Lists, Filters, Tags (as a nested tree), Completed, Trash, Calendar, Settings.
- **Task list:** grouping (priority, date, list, tag), sorting, swipe to complete, swipe to snooze or reschedule.
- **Task detail:** a bottom sheet that expands to full screen, with:
  - title and notes;
  - tags (a picker with a nested tree and search);
  - list;
  - date, time and duration;
  - repeat;
  - reminders;
  - priority;
  - checklist;
  - subtasks.
- **Calendar:** Day, Week and Month. Long-press to drag, and drag the edges to resize. An "Unscheduled" drawer is sourced from a chosen filter.
- **Filter builder:** nested groups and a live count.
- **Settings:**
  - Devices (pairing QR and scanner).
  - Google account and calendars.
  - Default reminder.
  - Trash retention.
  - Theme.

**Quick add:** a floating button opens a one-line title field, and the task saves to Inbox. There's no parsing (see [[01 Requirements & Scope]]). A share-sheet target ("share text to Utre") also creates an Inbox task.

## Home-screen widget (Kotlin, Jetpack Glance)

| Aspect | Design |
|---|---|
| Content | One chosen **list, tag or filter**, set per widget instance |
| Layout | Header (name, count, **+** button), then a scrollable `LazyColumn` of tasks |
| Row | Checkbox, title, then a compact due time or date chip and up to 2 tag colour dots. Overdue items in red. |
| Tap checkbox | `actionRunCallback`, then core `complete_task` via UniFFI. The row is crossed out at once and the widget refreshes. Completion also creates a sync op. |
| Tap row | Opens the app on that task (deep link) |
| Tap **+** | Opens a small translucent quick-add activity (title field and keyboard), which saves to Inbox |
| Configuration | A config activity on placement: choose a list, tag or filter, plus theme (follow system, light or dark) and background opacity. Stored per `appWidgetId` in DataStore. |
| Sizes | Resizable from 2×2 up to 4×6. Responsive `SizeMode.Responsive` with compact and roomy layouts. |
| Multiple widgets | Supported for free, since config is per instance (not required, but costs nothing) |

**When the widget refreshes:**
- After any core write: the core emits a change event, and the app calls `GlanceAppWidget.updateAll()`.
- After a sync batch is applied, including from the WorkManager job.
- At **midnight and at timezone changes**: relative filters such as "today" re-evaluate. A scheduled job plus `ACTION_DATE_CHANGED` / `ACTION_TIMEZONE_CHANGED` receivers trigger this.
- Don't rely on `updatePeriodMillis`, whose minimum is 30 minutes and which isn't precise.

**Performance:** the widget can start while the app process is dead. It opens SQLite read-only through the core with **no** Flutter engine start, so it should render in under 200 ms.

## Reminders on Android
- The core computes the next N reminder times. Kotlin schedules them with `AlarmManager.setExactAndAllowWhileIdle`.
  - This needs `SCHEDULE_EXACT_ALARM`, which the user grants. Or `USE_EXACT_ALARM`, which is fine for a sideloaded personal app.
- The schedule is rebuilt on boot (`BOOT_COMPLETED`), after sync, and after edits.
- **Notification actions:** *Done*, *Snooze 10 min*, *Snooze 1 h*. Both call the core directly without opening the UI.
- **Duplicate reminders across devices:** every device notifies. Dismissing or completing on one device syncs to the others, and they cancel their pending alarms when they next sync.

## Background sync
- A WorkManager periodic job runs every 15 minutes with a network constraint. It tries known peers with a short timeout (about 20 s), then exits.
- An expedited one-off job runs after a local edit, so changes go out quickly if a peer is reachable.
- Distribution is a sideloaded APK, so Play Store policy isn't a concern. Battery optimisation exemption is optional.

## Distribution
- A signed release APK built by CI (GitHub Actions) and installed manually, or through Obtainium from GitHub releases for auto-updates.

## Release signing (M4)
A phone only accepts an update signed with the same key as the installed app, so every build that should update the phone (this laptop, the Mac, CI) must sign with one **release key**. Debug keys differ per machine.

- `app/android/app/build.gradle.kts` signs release builds with the key named in `app/android/key.properties` when that file exists, and with the debug key otherwise. The file and any `*.jks` are git-ignored: **never commit them**, the repo is public.
- Making the key (the user does this once and keeps the `.jks` and passwords safe, e.g. in a password manager):
  ```bash
  keytool -genkeypair -v -keystore ~/utre-release.jks -alias utre \
    -keyalg RSA -keysize 4096 -validity 36500
  ```
- Local release builds: create `app/android/key.properties`:
  ```properties
  storeFile=/home/<you>/utre-release.jks
  storePassword=…
  keyAlias=utre
  keyPassword=…
  ```
- CI (`.github/workflows/android-release.yml`, on `v*` tags): set the repository secrets `ANDROID_KEYSTORE_BASE64` (`base64 -w0 ~/utre-release.jks`), `ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS` and `ANDROID_KEY_PASSWORD`. Without them the workflow skips cleanly (v0.8.0) instead of publishing an APK signed with a throwaway key.
- Switching an installed phone from a debug-signed APK to the release key needs one uninstall (local data is lost; sync brings it back from a paired device).
