#!/usr/bin/env bash
# Regenerate every raster app icon from the master SVGs in packaging/icon/.
# Needs rsvg-convert; oxipng is used when installed.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
src="$root/packaging/icon"
res="$root/app/android/app/src/main/res"
mac="$root/app/macos/Runner/Assets.xcassets/AppIcon.appiconset"

command -v rsvg-convert >/dev/null || { echo "rsvg-convert is required" >&2; exit 1; }

render() { # <svg> <px> <png>
  mkdir -p "$(dirname "$3")"
  rsvg-convert -w "$2" -h "$2" "$1" -o "$3"
}

# density name, launcher px (48 dp), layer px (108 dp), notification px (24 dp)
while read -r d launcher layer notif; do
  render "$src/utre-icon.svg" "$launcher" "$res/mipmap-$d/ic_launcher.png"
  render "$src/utre-foreground.svg" "$layer" "$res/mipmap-$d/ic_launcher_foreground.png"
  render "$src/utre-background.svg" "$layer" "$res/mipmap-$d/ic_launcher_background.png"
  render "$src/utre-monochrome.svg" "$layer" "$res/mipmap-$d/ic_launcher_monochrome.png"
  render "$src/utre-notification.svg" "$notif" "$res/drawable-$d/ic_stat_utre.png"
done <<'DENSITIES'
mdpi 48 108 24
hdpi 72 162 36
xhdpi 96 216 48
xxhdpi 144 324 72
xxxhdpi 192 432 96
DENSITIES

for px in 16 32 64 128 256 512 1024; do
  render "$src/utre-macos.svg" "$px" "$mac/app_icon_$px.png"
done

if command -v oxipng >/dev/null; then
  find "$res" "$mac" -name '*.png' \( -name 'ic_*' -o -name 'app_icon_*' \) \
    -exec oxipng --quiet --opt 4 --strip safe {} +
else
  echo "oxipng not found: PNGs are not optimised" >&2
fi
