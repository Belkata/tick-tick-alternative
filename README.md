<p align="center"><img src="docs/logo.png" alt="Utre logo" width="120"></p>

# Utre

Utre is a personal, local-first task manager. There is an Android app with a home-screen widget and a desktop app for Linux and macOS. Devices sync directly with each other, peer to peer, with no server. New tasks land in a triage Inbox, tags can be nested, and a calendar view shows Google Calendar events and lets you drag tasks onto the grid.

Your data lives in a local SQLite database on each device. Nothing is sent anywhere except to your own paired devices and, if you turn it on, Google Calendar.

## Install

### Arch Linux (signed pacman repository, recommended)

Packages and the repository database are signed. Download the repository key.

```bash
curl -LO https://raw.githubusercontent.com/Belkata/utre/main/packaging/arch/utre-repo.asc
```

Add it to pacman's keyring. Check that the fingerprint is `8D9A9E543BB6D88237D174A78BE15C9843ADD341`.

```bash
sudo pacman-key --add utre-repo.asc
```

Trust it locally.

```bash
sudo pacman-key --lsign-key 8D9A9E543BB6D88237D174A78BE15C9843ADD341
```

Add the repository to `/etc/pacman.conf`. This is the block:

```ini
[utre]
Server = https://github.com/Belkata/utre/releases/download/arch-repo
```

Or append it with one command.

```bash
printf '\n[utre]\nServer = https://github.com/Belkata/utre/releases/download/arch-repo\n' | sudo tee -a /etc/pacman.conf
```

Install the package. Use `-Syu`, not `-Sy`: partial upgrades are not supported on Arch.

```bash
sudo pacman -Syu utre-bin
```

Start the daemon. It owns the database and runs the tray icon, reminders and sync.

```bash
systemctl --user daemon-reload
```

```bash
systemctl --user enable --now utre.service
```

Open the app from your launcher (the entry is called "Utre") or run it.

```bash
utre-app
```

Optional: a StatusNotifierItem tray host (for example `waybar`) shows the tray icon, and a notification daemon (for example `dunst`) shows reminders.

### Updating on Arch

```bash
sudo pacman -Syu
```

(or `yay -Syu`). Then restart the daemon and reopen the app, so both run the same version.

```bash
systemctl --user restart utre.service
```

### Arch from source

`packaging/arch/PKGBUILD` (`utre-git`) builds the main branch. It needs Rust and Flutter on your `PATH`. It conflicts with `utre-bin`.

```bash
cd packaging/arch && makepkg -si
```

Then enable the daemon as above. See also [packaging/arch/README.md](packaging/arch/README.md).

### Android

Download the latest `utre-X.Y.Z-arm64.apk` from [GitHub Releases](https://github.com/Belkata/utre/releases) and open it on the phone. Android asks you to allow installs from that source (your browser or file manager). Releases are currently marked as pre-releases. Later versions install over the old one as long as they are signed with the same key.

To add the widget, long-press the home screen, choose Widgets, then Utre.

### macOS

There is no prebuilt download yet. Build from source. You need Xcode, Rust, Flutter and CocoaPods.

```bash
source scripts/env.sh
```

```bash
scripts/build-macos.sh
```

This builds `Utre.app` (with the daemon inside) and a DMG in `dist/`. The app is at `app/build/macos/Build/Products/Release/Utre.app`. Copy it to `/Applications`, then start the daemon at login with a LaunchAgent.

```bash
scripts/install-launch-agent.sh "/Applications/Utre.app"
```

### Pairing devices

Sync is peer to peer, so you pair each device once. On one device, open the Devices page and choose "Pair a device". It shows a QR code and a one-time ticket, valid for 5 minutes. On the other device choose "Join a device", then scan the QR code (phone) or paste the ticket. The two devices sync right away and keep syncing while they are online. "Sync now" on the Devices page forces a round, and "Forget" unpairs a device.

From a terminal the same steps are:

```bash
utre sync pair
```

```bash
utre sync join <ticket>
```

Other subcommands are `utre sync now`, `utre sync peers` and `utre sync forget <device>`.

## The `utre` CLI

The package installs the `utre` command. It talks to the running daemon.

```bash
utre add "Buy milk" --tag home/shopping --due tomorrow
```

```bash
utre ls
```

```bash
utre done <id>
```

```bash
utre remind <id> 15m
```

Task ids are the last 8 characters shown by `utre ls`. Run `utre --help` for all commands, and `utre <command> --help` for one.

```bash
utre --help
```

## Building from source and development

```bash
source scripts/env.sh
cargo test --workspace
cd app && flutter test
```

The plan, decisions and logs live in [plan/](plan/), starting at `plan/00 Overview.md`. [CLAUDE.md](CLAUDE.md) is the developer guide: repo layout, build commands and gotchas.

## License

[MIT](LICENSE). The bundled Manrope font is under the [SIL Open Font License](app/assets/fonts/OFL.txt). The privacy notes are in [PRIVACY.md](PRIVACY.md).
