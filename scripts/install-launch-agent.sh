#!/usr/bin/env bash
# macOS: install dev.utre.daemon.plist into ~/Library/LaunchAgents/, so utre-daemon
# starts at login. The counterpart of scripts/install-user-service.sh on Linux.
#   scripts/install-launch-agent.sh               this checkout's build (scripts/build-macos.sh)
#   scripts/install-launch-agent.sh <Utre.app> an installed copy, e.g. "/Applications/Utre.app"
#   scripts/install-launch-agent.sh --remove      stop it and delete it
# The copy inside the app (Utre.app/Contents/Resources/install-launch-agent.sh, put there by
# build-macos.sh together with the plist template) needs no checkout and defaults to the app
# bundle it sits in:  "/Applications/Utre.app/Contents/Resources/install-launch-agent.sh"
# It runs the daemon bundled in the app (Utre Daemon.app, needed for notifications), else
# this checkout's target/release/utre-daemon.
# Logs: ~/Library/Logs/utre/daemon.log
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
repo="$(cd "$here/.." && pwd)"
label="dev.utre.daemon"
agent="$HOME/Library/LaunchAgents/$label.plist"
domain="gui/$(id -u)"

if [[ "${1:-}" == "--remove" ]]; then
  launchctl bootout "$domain/$label" 2>/dev/null || true
  rm -f "$agent"
  echo "Removed $agent"
  exit 0
fi

# Inside a bundle (Contents/Resources): the template sits next to this script and the app
# is the bundle itself. In a checkout: the template is in daemon/ and the app is the build.
if [[ -f "$here/$label.plist" ]]; then
  template="$here/$label.plist"
  default_app="$(cd "$here/../.." && pwd)"
else
  template="$repo/daemon/$label.plist"
  default_app="$repo/app/build/macos/Build/Products/Release/Utre.app"
fi
app="${1:-$default_app}"
app="${app%/}"
daemon="$app/Contents/Helpers/Utre Daemon.app/Contents/MacOS/utre-daemon"
log="$HOME/Library/Logs/utre/daemon.log"
if [[ ! -x "$daemon" ]]; then
  daemon="$repo/target/release/utre-daemon"
  [[ -x "$daemon" ]] || { echo "No daemon in $app. Build it first: scripts/build-macos.sh" >&2; exit 1; }
  echo "Note: no bundled daemon in $app; using $daemon (no notifications)." >&2
fi
[[ -d "$app" ]] || echo "Note: $app isn't built yet; the menu's Open won't work until it is." >&2

mkdir -p "$(dirname "$agent")" "$(dirname "$log")"
sed -e "s|@DAEMON@|$daemon|" \
    -e "s|@APP@|$app|" \
    -e "s|@LOG@|$log|g" \
    "$template" > "$agent"
plutil -lint "$agent" >/dev/null
# Reload: bootout fails harmlessly when it isn't loaded yet, and finishes asynchronously.
launchctl bootout "$domain/$label" 2>/dev/null || true
for _ in $(seq 50); do
  launchctl print "$domain/$label" >/dev/null 2>&1 || break
  sleep 0.1
done
launchctl bootstrap "$domain" "$agent"
echo "Installed $agent"
sleep 1
launchctl print "$domain/$label" | grep -E "^\s*(state|pid|last exit code) =" || true
