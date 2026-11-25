---
tags: [project/tick-alt, plan, desktop]
---

# 08 Desktop App (macOS + Arch/Hyprland)

Back to [[00 Overview]].

## Window UI (Flutter, Option A)
- **Three-pane layout**, as in TickTick:
  - a sidebar (Inbox, Lists, Filters, Tags tree, Completed, Trash);
  - a task list;
  - a detail pane.
- **Calendar mode:** Week (default), Day and Month. The Unscheduled sidebar is on the right, sourced from a chosen filter. Drag tasks in, and drag edges to resize.
- Keyboard basics:
  - `n` new task
  - `Enter` edit
  - `Space` complete
  - `Del` trash
  - `t` today
  - arrow keys to navigate
- A full command palette is out of scope for v1.
- **Hyprland specifics:**
  - Runs natively on Wayland through GTK.
  - The window class (Wayland app id) is `dev.tickalt.tick_alt`, for window rules such as floating the quick-add window: `class:^(dev\.tickalt\.tick_alt)$`. GTK needs a reverse-DNS id, it matches the `.desktop` file name, and it names the app's data directory, so it stays rather than becoming `tick-alt` (M2, see [[15 M2 Desktop Log]]). The title is `Tick Alt`.

## Tray daemon (`tt-daemon`)
- Owns the DB, iroh endpoint, sync loop, Google polling and reminder scheduling.
- Starts at login:
  - **macOS:** a LaunchAgent / login item.
  - **Arch:** a `systemd --user` service, `tick-alt.service`.
- **Tray / menu-bar icon:**
  - Shows the Inbox count and the sync state: synced, syncing, offline, or "no peers seen in X h".
  - Menu: *Open*, *Quick add…*, *Sync now*, *Devices*, *Quit*.
  - On Hyprland the tray uses StatusNotifierItem, so it shows in **waybar**'s tray module.
- **Notifications:**
  - **macOS:** `UNUserNotificationCenter`, with *Done* and *Snooze* actions.
  - **Linux:** freedesktop notifications to mako, dunst or swaync, with action buttons where the daemon supports them.
- **The UI is a client of the daemon.** It talks over a local socket (Unix domain socket) using the same command and query API, so closing the window keeps sync and reminders running.

## CLI (`tt`)
A single Rust binary. It talks to the daemon's socket, or opens the DB directly if the daemon isn't running.

```
tt add "Buy milk"                 # → Inbox
tt add "Call dentist" -t health   # still no NL parsing; flags instead
tt ls                             # Inbox
tt ls -f "Today + Inbox"          # any filter, list, or tag
tt done <id|#n>
tt tags                           # tag tree with counts
tt tags order <tag> --before <tag>  # arrange among siblings (no --before: last)
tt sync                           # trigger sync now
tt devices                        # paired devices + last seen
tt import ticktick backup.csv     # see [[12 TickTick Import]]
```
- `--json` output works well with scripts and waybar custom modules, for example showing the Inbox count in the bar.
- On Hyprland, `bind = SUPER, T, exec, tt-quickadd` (a tiny popup) gives a global quick add for free, even though it's not a v1 feature.

## Packaging
- **Arch:**
  - A `PKGBUILD` in the repo, published to the AUR later as `tick-alt` / `tick-alt-bin`.
  - It installs the app, the daemon, the CLI, the systemd user unit and a `.desktop` file.
- **macOS:**
  - A `.app` bundle in a DMG, signed ad-hoc for personal use. Notarization only if it's ever shared.
  - Universal or arm64.
- **CI:** GitHub Actions builds Linux x86_64, macOS arm64 and Android APK artifacts on tags.
