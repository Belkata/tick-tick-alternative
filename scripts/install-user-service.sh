#!/usr/bin/env bash
# Install a dev copy of daemon/tick-alt.service into ~/.config/systemd/user/, pointing at
# this checkout's release builds (target/release/tt-daemon and the Flutter bundle).
#   scripts/install-user-service.sh            install and (re)start it, enabled at login
#   scripts/install-user-service.sh --remove   stop, disable and delete it
set -euo pipefail
repo="$(cd "$(dirname "$0")/.." && pwd)"
unit_dir="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"
unit="$unit_dir/tick-alt.service"

if [[ "${1:-}" == "--remove" ]]; then
  systemctl --user disable --now tick-alt.service 2>/dev/null || true
  rm -f "$unit"
  systemctl --user daemon-reload
  echo "Removed $unit"
  exit 0
fi

daemon="$repo/target/release/tt-daemon"
app="$repo/app/build/linux/x64/release/bundle/tick_alt"
[[ -x "$daemon" ]] || { echo "Build it first: cargo build --release -p tt-daemon" >&2; exit 1; }
[[ -x "$app" ]] || echo "Note: $app isn't built yet; the tray's Open won't work until it is." >&2

mkdir -p "$unit_dir"
sed -e "s|^ExecStart=.*|ExecStart=$daemon|" \
    -e "s|^Environment=TT_APP=.*|Environment=TT_APP=$app|" \
    "$repo/daemon/tick-alt.service" > "$unit"
systemctl --user daemon-reload
systemctl --user enable tick-alt.service
systemctl --user restart tick-alt.service
echo "Installed $unit"
systemctl --user --no-pager status tick-alt.service | head -5
