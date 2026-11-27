#!/usr/bin/env bash
# Publish a release to the AUR as utre-bin (plan/08 Packaging).
#   scripts/aur-publish.sh X.Y.Z [--dry-run]
# Needs dist/utre-X.Y.Z-linux-x86_64.tar.gz (scripts/build-linux-tarball.sh) and, unless --dry-run,
# the same file attached to the GitHub release vX.Y.Z and the user's AUR SSH key.
#
# packaging/aur/utre-bin/ is a template and is never bumped in this repo: the real pkgver, sha256 and
# .SRCINFO exist only in the AUR repo (cloned to ~/.cache/utre/aur-utre-bin).
# --dry-run stops before the clone, commit and push, and prints the result from a temp dir.
set -euo pipefail
repo="$(cd "$(dirname "$0")/.." && pwd)"
template="$repo/packaging/aur/utre-bin"

version=""
dry=0
for arg in "$@"; do
  case "$arg" in
    --dry-run) dry=1 ;;
    -*) echo "unknown option: $arg" >&2; exit 2 ;;
    *) version="$arg" ;;
  esac
done
[[ -n "$version" ]] || { echo "usage: $0 X.Y.Z [--dry-run]" >&2; exit 2; }

tarball="$repo/dist/utre-$version-linux-x86_64.tar.gz"
[[ -f "$tarball" ]] || { echo "missing $tarball (run scripts/build-linux-tarball.sh)" >&2; exit 1; }
sum="$(sha256sum "$tarball" | cut -d' ' -f1)"

if [[ $dry == 0 ]]; then
  assets="$(gh release view "v$version" --json assets --jq '.assets[].name')"
  if ! grep -qx "utre-$version-linux-x86_64.tar.gz" <<<"$assets"; then
    echo "GitHub release v$version has no utre-$version-linux-x86_64.tar.gz asset" >&2
    exit 1
  fi
fi

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
cp "$template/PKGBUILD" "$template/utre-bin.install" "$work/"
sed -i \
  -e "s/^pkgver=.*/pkgver=$version/" \
  -e "s/^pkgrel=.*/pkgrel=1/" \
  -e "s/^sha256sums=.*/sha256sums=('$sum')/" \
  "$work/PKGBUILD"
(cd "$work" && makepkg --printsrcinfo > .SRCINFO)
if command -v namcap >/dev/null; then
  namcap "$work/PKGBUILD" || true
else
  echo "namcap not installed, skipping" >&2
fi

if [[ $dry == 1 ]]; then
  echo "== dry run: nothing cloned, committed or pushed"
  echo "== PKGBUILD"; cat "$work/PKGBUILD"
  echo "== .SRCINFO"; cat "$work/.SRCINFO"
  exit 0
fi

aur="$HOME/.cache/utre/aur-utre-bin"
if [[ -d "$aur/.git" ]]; then
  git -C "$aur" pull --ff-only || true
else
  mkdir -p "$(dirname "$aur")"
  git clone ssh://aur@aur.archlinux.org/utre-bin.git "$aur"
fi
cp "$work/PKGBUILD" "$work/.SRCINFO" "$work/utre-bin.install" "$aur/"
git -C "$aur" add PKGBUILD .SRCINFO utre-bin.install
git -C "$aur" -c user.email=belkata.okfo@gmail.com -c user.name="Boris Belev" \
  commit -m "utre-bin $version"
git -C "$aur" push origin HEAD:master
echo "published utre-bin $version"
