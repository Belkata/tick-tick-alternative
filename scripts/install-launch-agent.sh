#!/usr/bin/env bash
# macOS: install a dev copy of daemon/dev.tickalt.daemon.plist into ~/Library/LaunchAgents/,
# pointing at this checkout's release builds (target/release/tt-daemon and the Flutter .app).
# The counterpart of scripts/install-user-service.sh on Linux.
#   scripts/install-launch-agent.sh            install and (re)start it, started at login
#   scripts/install-launch-agent.sh --remove   stop it and delete it
# Logs: ~/Library/Logs/tick-alt/daemon.log
set -euo pipefail
repo="$(cd "$(dirname "$0")/.." && pwd)"
label="dev.tickalt.daemon"
agent="$HOME/Library/LaunchAgents/$label.plist"
domain="gui/$(id -u)"

if [[ "${1:-}" == "--remove" ]]; then
  launchctl bootout "$domain/$label" 2>/dev/null || true
  rm -f "$agent"
  echo "Removed $agent"
  exit 0
fi

daemon="$repo/target/release/tt-daemon"
app="$repo/app/build/macos/Build/Products/Release/tick_alt.app"
log="$HOME/Library/Logs/tick-alt/daemon.log"
[[ -x "$daemon" ]] || { echo "Build it first: cargo build --release -p tt-daemon" >&2; exit 1; }
[[ -d "$app" ]] || echo "Note: $app isn't built yet; the menu's Open won't work until it is." >&2

mkdir -p "$(dirname "$agent")" "$(dirname "$log")"
sed -e "s|@DAEMON@|$daemon|" \
    -e "s|@APP@|$app|" \
    -e "s|@LOG@|$log|g" \
    "$repo/daemon/$label.plist" > "$agent"
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
