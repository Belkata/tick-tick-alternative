#!/usr/bin/env bash
# Install a dev copy of the app's desktop entry and icon for the current user, so launchers
# (rofi drun, …) list Tick Alt. The entry runs this checkout's release bundle.
#   scripts/install-desktop-entry.sh            install
#   scripts/install-desktop-entry.sh --remove   uninstall
# The file name matches the app's Wayland app id (dev.tickalt.tick_alt), which ties the
# running window to the entry. Packages install app/linux/packaging/ as is (Exec=tick-alt).
set -euo pipefail
repo="$(cd "$(dirname "$0")/.." && pwd)"
id="dev.tickalt.tick_alt"
data="${XDG_DATA_HOME:-$HOME/.local/share}"
apps="$data/applications"
icons="$data/icons/hicolor"

refresh() {
  command -v update-desktop-database >/dev/null && update-desktop-database "$apps" || true
  command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q -t -f "$icons" || true
}

if [[ "${1:-}" == "--remove" ]]; then
  rm -f "$apps/$id.desktop" "$icons/scalable/apps/$id.svg" "$icons/256x256/apps/$id.png"
  refresh
  echo "Removed the Tick Alt desktop entry and icon"
  exit 0
fi

app="$repo/app/build/linux/x64/release/bundle/tick_alt"
[[ -x "$app" ]] || { echo "Build it first: cd app && flutter build linux --release" >&2; exit 1; }

mkdir -p "$apps" "$icons/scalable/apps" "$icons/256x256/apps"
sed -e "s|^Exec=.*|Exec=$app|" "$repo/app/linux/packaging/$id.desktop" > "$apps/$id.desktop"
cp "$repo/app/linux/packaging/$id.svg" "$icons/scalable/apps/$id.svg"
# A PNG too, for launchers or themes that don't render SVG.
if command -v rsvg-convert >/dev/null; then
  rsvg-convert -w 256 -h 256 "$repo/app/linux/packaging/$id.svg" -o "$icons/256x256/apps/$id.png"
fi
refresh
echo "Installed $apps/$id.desktop"
