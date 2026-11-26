---
tags: [project/tick-alt, plan, m4]
created: 2026-11-25
---

# 17 M4 Android Log

Back to [[00 Overview]] · [[10 Roadmap]] · [[07 Android App & Widget]] · [[16 M3 Sync Log]].

## Scope (the user's choice, 2026-11-25)
- In: the app plus phone sync (with QR pairing), reminders plus background sync, release via CI.
- Not now: widget polish (choosing a list or filter, a scrolling list, **+** quick add, midnight refresh).
- The phone gets one uninstall of v0.1.0 (Mac debug key), then installs builds from the Arch laptop.

## Toolchain on the Arch laptop (2026-11-25)
| Piece | Version | Source |
|---|---|---|
| Android Studio | 2026.2.2 (bundled JBR = JDK 25, used as `JAVA_HOME`) | AUR `android-studio` |
| SDK | platform 36, build-tools 37.0.0, platform-tools 37.0.3, NDK 28.2.13676358, CMake 3.22.1, emulator 37.3 | Google's cmdline-tools in `~/Android/Sdk`, then `android sdk install …` |
| Emulator | `tt_pixel`: Android 36 Google APIs x86_64, Pixel 8, KVM | `avdmanager create avd` |
| WorkManager | 2.12.0 | Google Maven |
| `qr_flutter` / `mobile_scanner` | 4.1.0 / 7.5.0 | pub.dev |

- **`sdkmanager` is deprecated.** The new **`android` CLI** (`android sdk install platform-tools platforms/android-36 …`, with `/` in package names) replaces it. `--licenses` is no longer needed. Its first run downloads itself and shows Google's terms. It collects usage metrics unless given **`--no-metrics`**.
- A release APK for x86_64 took 4 m 43 s the first time (cargokit compiles the core with iroh for the ABI) and about 50 s after that. arm64 is a separate first build.

## What was built
- **Shared sync service:** `tt_sync::service::SyncService` (node, status, rounds, `spawn_auto`) moved out of the daemon, so the daemon and the phone run the same code. The daemon keeps only the socket adapter (`daemon/src/sync.rs`).
- **Phone sync, in-process** (`app/rust/src/phone.rs`):
  - One process-wide node on its own tokio runtime, with relays and auto-sync (5 s after start, every 90 s, 3 s after edits).
  - `TaskDb.startPhoneSync(keyPath, device)` starts it. Sync, pairing and trust events tick the change stream, so the UI refreshes.
  - The Devices calls (`syncStatus`, `syncPair`, `syncJoin`, `syncNow`) go to it when there's no daemon. `syncAvailable()` gates the page.
  - Kotlin's `backgroundSync()` reuses the same node, so only one node ever holds the key.
- **Time zone:** Android has no `/etc/localtime`, so the core thought "today" was UTC. `setTimeZone(name)` (bridge) takes Android's IANA zone at startup.
- **Devices page:** the pairing dialog shows a **QR code** of the ticket. On Android, **Join** offers *Scan QR code* (camera) or *Paste ticket*. Desktop keeps showing the ticket and Copy.
- **Reminders** (`reminders/Reminders.kt`):
  - The core lists reminders firing in the next 48 h (`WidgetDb.upcomingReminders`, zone passed in). Each becomes an exact alarm (`USE_EXACT_ALARM`; `setExactAndAllowWhileIdle`).
  - The alarm posts a notification with **Done**, **Snooze 10 min** and **1 h**. Done completes the task through the core, refreshes the widget and reschedules.
  - Rescheduled after app edits (`WidgetBridge.changed()`), background syncs, boot and app updates. `POST_NOTIFICATIONS` is asked once at startup.
- **Background sync** (`sync/SyncWorker.kt`): WorkManager, every 15 min when online, plus an **expedited one-off after edits made outside the app** (a widget tick, a reminder's Done). Each run refreshes the widget and reminders afterwards.
- **Share target:** text or a link shared from any app (ACTION_SEND `text/plain`) becomes an Inbox task, with a snackbar.
- **Phone layout fixes:** pages get a menu-button bar instead of a floating button over their title, "This device" shows a phone icon, and the daemon link icon is desktop-only.
- **Release via CI:** `.github/workflows/android-release.yml` builds the arm64 APK on `v*` tags and publishes a pre-release. It signs with a release keystore from repository secrets and fails without them. Gradle signs with `app/android/key.properties` when present (git-ignored). Key setup: [[07 Android App & Widget#Release signing (M4)]].
- Worked on a feature branch, `m4-android`, plus a second worktree (`m4-features`) for edits while Gradle builds.

## Results on the emulator (release build, Android 16)
- The app starts on the phone shell and asks for notifications. The background job ran at once ("synced with 0 of 0 devices").
- **Pairing phone ↔ laptop** with a scratch `tt-daemon` (relays on): desktop `tt sync pair`, then on the phone Devices → Join → Paste. The phone showed "ma6inata, synced just now"; the daemon logged "paired with Google sdk_gphone64_x86_64" and sent 17 ops.
- **Both directions:** "From the laptop" appeared in the phone's Inbox. "Added on the phone", typed in the app, reached the laptop's database seconds later through auto-sync.
- **Reminder:** a desktop task with a reminder synced to the phone, which scheduled an exact alarm (`dumpsys alarm`: 08:46:00, `exactAllowReason=policy_permission`).
  - The notification appeared at **08:46:01** with Done, Snooze 10 min and 1 h.
  - **Done** dismissed it, completed the task on the phone, and the expedited sync delivered that to the laptop 36 s later (`tt completed`: "✓ Phone reminder test, done today 08:46").
- **Share target:** `am start -a android.intent.action.SEND` with a subject and a link added "Rust async book https://…" to the Inbox, with a snackbar. Auto-sync carried it to the laptop.
- **Background job:** it ran once at enqueue. A forced run (`cmd jobscheduler run -f -n androidx.work.systemjobscheduler dev.tickalt.tick_alt 1`) is declined by WorkManager inside the 15-min period ("executed before schedule"), which is normal.
- After a reinstall (`adb install -r`) the phone keeps its data and its pairing. The phone layout fixes and the QR pairing dialog check out visually.
- Desktop: `flutter analyze` clean, unit 11/11, Linux integration 10/10.

## The phone build for the user
- `dist/tick-alt-v0.2.0-arm64.apk` (git-ignored), **79.9 MB**, up from 35 MB in v0.1.0, mostly the camera and barcode libraries behind `mobile_scanner`. Signed with this laptop's debug key: uninstall v0.1.0 first (its local data goes; pair with the laptop and sync brings the laptop's tasks).
- Published as GitHub pre-release **v0.2.0** (`tick-alt-0.2.0-arm64.apk`, SHA-256 `7e56bb11…118e09d`). Its CI run fails at the signing-secrets check until the release key exists (expected).

## On the user's phone (2026-11-25)
- v0.2.0 installed on a **Xiaomi 22101316G** and paired with the laptop's tt-daemon at 09:09:22 (Devices → Join on the phone). The laptop sent 26 ops (its tasks); 22 s later the phone sent 2 back (a task added on the phone). Periodic syncs follow every 90 s.

## Open
- **Scanning the QR from a real phone** isn't tested (the emulator's camera can't). The code is dense: the ticket is hex-encoded JSON, about 690 chars. If scanning struggles, switch the ticket to base64url and shorter field names (about 2× smaller).
- **Release key and CI secrets:** the user creates the key and adds the secrets (plan/07). Moving from debug-signed to release-signed means one more uninstall.
- The Spike 2 relay flakiness on Android (relay in about 1 of 4 launches) didn't show here: pairing and syncs reached the laptop every time on the emulator. Real phone on mobile data: still to try.
- Flutter warns about migrating the Android build to built-in Kotlin. It isn't blocking (the Compose plugin order in plan/13 depends on today's setup).
- Widget polish (choosing a list or filter, scrolling, **+** quick add, midnight refresh): left out of this push by the user. Done 2026-11-26, see below.

## Widget polish (2026-11-26)
- **Source per widget:** `WidgetConfigActivity` (placement and "reconfigure" on Android 12+) lists Inbox, lists, tags and saved filters from the core (`WidgetDb.sources`); the choice lives in the widget's Glance state (`source_kind`, `source_id`). A deleted list, tag or filter falls back to the Inbox. The launcher's pin flow on the emulator didn't run the config screen (the widget came up as Inbox), so reconfigure from the widget's long-press menu. Not checked on the Xiaomi.
- **List:** `WidgetDb.tasks(kind, id, tz)` returns rows already labelled by the core (due label, overdue, due today, up to 2 tag colours), sorted like the app (the device's sort). Soft colours (light and dark), priority on the checkbox border, due text red when overdue and teal when today. Empty state: "All clear".
- **+** opens `QuickAddActivity`, a small dialog (no app start). It adds through `WidgetDb.addIn`, so the task lands in the shown list or tag, or gets what a filter asks for (as the app's quick add does), then refreshes the widgets and syncs soon.
- **Midnight:** `WidgetRefresh` sets an inexact alarm at the next local midnight + 5 s. `DATE_CHANGED`, `TIMEZONE_CHANGED`, `TIME_SET`, boot and app update also redraw and re-arm (and reschedule reminders). Test: `WidgetRefreshTest` (zones, DST).
- **Open:** tapping a row starts the app with `dev.utre.OPEN_TASK` and the task id. `MainActivity` keeps it (`takeOpenTask` / `openTask` on the `utre/widget` channel), but the Dart UI doesn't select the task yet, so for now the tap only opens the app.
