#!/usr/bin/env bash
# Install a dev copy of daemon/utre.service into ~/.config/systemd/user/, pointing at
# this checkout's release builds (target/release/utre-daemon and the Flutter bundle).
#   scripts/install-user-service.sh            install and (re)start it, enabled at login
#   scripts/install-user-service.sh --remove   stop, disable and delete it
set -euo pipefail
repo="$(cd "$(dirname "$0")/.." && pwd)"
unit_dir="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"
unit="$unit_dir/utre.service"

if [[ "${1:-}" == "--remove" ]]; then
  systemctl --user disable --now utre.service 2>/dev/null || true
  rm -f "$unit"
  systemctl --user daemon-reload
  echo "Removed $unit"
  exit 0
fi

# The daily-driver build (`cargo build --profile fast`) wins when it's newer than the
# release one; scripts/update-desktop.sh builds it.
daemon="$repo/target/release/utre-daemon"
fast="$repo/target/fast/utre-daemon"
if [[ -x "$fast" && ( ! -x "$daemon" || "$fast" -nt "$daemon" ) ]]; then daemon="$fast"; fi
app="$repo/app/build/linux/x64/release/bundle/utre"
[[ -x "$daemon" ]] || { echo "Build it first: cargo build --profile fast -p utre-daemon" >&2; exit 1; }
[[ -x "$app" ]] || echo "Note: $app isn't built yet; the tray's Open won't work until it is." >&2

mkdir -p "$unit_dir"
sed -e "s|^ExecStart=.*|ExecStart=$daemon|" \
    -e "s|^Environment=UTRE_APP=.*|Environment=UTRE_APP=$app|" \
    "$repo/daemon/utre.service" > "$unit"
# The pre-rename unit (tick-alt.service) owned the old data directory; retire it first.
systemctl --user disable --now tick-alt.service 2>/dev/null || true
rm -f "$unit_dir/tick-alt.service"
systemctl --user daemon-reload
systemctl --user enable utre.service
systemctl --user restart utre.service
echo "Installed $unit"
systemctl --user --no-pager status utre.service | head -5
