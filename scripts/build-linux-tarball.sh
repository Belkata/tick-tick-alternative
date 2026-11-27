#!/usr/bin/env bash
# Build the prebuilt Linux package contents for the AUR's utre-bin (plan/08 Packaging).
#   scripts/build-linux-tarball.sh [--version X.Y.Z]
# Builds `utre` and `utre-daemon` (cargo release profile) and the Flutter desktop app, then writes
# dist/utre-X.Y.Z-linux-x86_64.tar.gz. The default version is app/pubspec.yaml's, without "+build".
# Uses $CARGO_TARGET_DIR when set (agent worktrees), otherwise <repo>/target.
set -euo pipefail
repo="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo"
# shellcheck source=env.sh
source "$repo/scripts/env.sh"

version=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --version) version="${2:?--version needs a value}"; shift 2 ;;
    *) echo "unknown option: $1" >&2; exit 2 ;;
  esac
done
if [[ -z "$version" ]]; then
  version="$(sed -n 's/^version: *\([^+ ]*\).*/\1/p' app/pubspec.yaml | head -n1)"
fi
[[ -n "$version" ]] || { echo "could not work out the version" >&2; exit 1; }

target="${CARGO_TARGET_DIR:-$repo/target}"
name="utre-$version"
stage="$repo/dist/.stage-$version"
out="$repo/dist/$name-linux-x86_64.tar.gz"

echo "== utre and utre-daemon (release)"
cargo build --release -p utre -p utre-daemon

echo "== desktop app (Flutter release)"
(cd app && flutter build linux --release)

echo "== staging $name"
rm -rf "$stage"
mkdir -p "$stage/$name"/{bin,lib/utre,share/systemd,licenses}
install -m755 "$target/release/utre" "$target/release/utre-daemon" "$stage/$name/bin/"
cp -r app/build/linux/x64/release/bundle/. "$stage/$name/lib/utre/"
install -m644 daemon/utre.service "$stage/$name/share/systemd/utre.service"
install -m644 app/linux/packaging/dev.utre.utre.desktop app/linux/packaging/dev.utre.utre.svg \
  "$stage/$name/share/"
install -m644 app/assets/fonts/OFL.txt "$stage/$name/licenses/Manrope-OFL.txt"
if [[ -f LICENSE ]]; then
  install -m644 LICENSE "$stage/$name/LICENSE"
fi

echo "== packing"
mkdir -p dist
tar --sort=name --owner=0 --group=0 --numeric-owner \
  -C "$stage" -czf "$out" "$name"
rm -rf "$stage"

echo
echo "$out"
sha256sum "$out"
