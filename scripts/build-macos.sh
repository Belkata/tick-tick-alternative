#!/usr/bin/env bash
# macOS: build Tick Alt.app with the daemon inside it, signed ad hoc, and a DMG (plan/08).
#   scripts/build-macos.sh          build everything into dist/tick-alt-<version>-macos.dmg
#   scripts/build-macos.sh --no-dmg build the .app only
#
# The bundle:
#   Tick Alt.app                      the Flutter app (dev.tickalt.tickAlt)
#     Contents/Helpers/
#       Tick Alt Daemon.app           tt-daemon as a menu-bar agent (dev.tickalt.daemon, LSUIElement).
#                                     A bundle of its own so UNUserNotificationCenter works and
#                                     Done / Snooze reach the daemon, not the window app.
#       tt                            the CLI
# scripts/install-launch-agent.sh then starts the daemon at login.
set -euo pipefail
repo="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=env.sh
source "$repo/scripts/env.sh"

version="$(sed -n 's/^version: *\([0-9.]*\).*/\1/p' "$repo/app/pubspec.yaml")"
app="$repo/app/build/macos/Build/Products/Release/Tick Alt.app"
helpers="$app/Contents/Helpers"
daemon_app="$helpers/Tick Alt Daemon.app"

echo "==> Rust: tt-daemon and tt"
cargo build --release -p tt-daemon -p tt --manifest-path "$repo/Cargo.toml"

echo "==> Flutter: Tick Alt.app"
(cd "$repo/app" && flutter build macos --release)

echo "==> Tick Alt Daemon.app"
rm -rf "$helpers"
mkdir -p "$daemon_app/Contents/MacOS" "$daemon_app/Contents/Resources"
cp "$repo/target/release/tt-daemon" "$daemon_app/Contents/MacOS/tt-daemon"
cp "$app/Contents/Resources/AppIcon.icns" "$daemon_app/Contents/Resources/AppIcon.icns"
cat > "$daemon_app/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleIdentifier</key>
	<string>dev.tickalt.daemon</string>
	<key>CFBundleName</key>
	<string>Tick Alt Daemon</string>
	<key>CFBundleDisplayName</key>
	<string>Tick Alt</string>
	<key>CFBundleExecutable</key>
	<string>tt-daemon</string>
	<key>CFBundleIconFile</key>
	<string>AppIcon</string>
	<key>CFBundlePackageType</key>
	<string>APPL</string>
	<key>CFBundleShortVersionString</key>
	<string>$version</string>
	<key>CFBundleVersion</key>
	<string>$version</string>
	<key>LSMinimumSystemVersion</key>
	<string>12.0</string>
	<key>LSUIElement</key>
	<true/>
</dict>
</plist>
EOF
plutil -lint "$daemon_app/Contents/Info.plist" >/dev/null
cp "$repo/target/release/tt" "$helpers/tt"

echo "==> Signing (ad hoc)"
codesign --force --sign - "$helpers/tt"
codesign --force --sign - "$daemon_app"
# Re-sign the outer app over its new contents, keeping Flutter's entitlements.
codesign --force --sign - --preserve-metadata=entitlements "$app"
codesign --verify --deep --strict "$app"

if [[ "${1:-}" == "--no-dmg" ]]; then
  echo "Built $app"
  exit 0
fi

echo "==> DMG"
dmg="$repo/dist/tick-alt-$version-macos.dmg"
stage="$(mktemp -d)"
trap 'rm -rf "$stage"' EXIT
cp -R "$app" "$stage/"
ln -s /Applications "$stage/Applications"
mkdir -p "$repo/dist"
rm -f "$dmg"
hdiutil create -quiet -volname "Tick Alt $version" -srcfolder "$stage" -format UDZO "$dmg"
echo "Built $dmg ($(du -h "$dmg" | cut -f1))"
