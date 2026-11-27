#!/usr/bin/env bash
# Shared by aur-publish.sh and pacman-repo-publish.sh: make a releasable copy of the utre-bin
# template (packaging/aur/utre-bin/) with the real version and checksum filled in.
# Source it; it defines fill_utre_bin_template <dest-dir> <version> <tarball>.

fill_utre_bin_template() {
  local dest="$1" version="$2" tarball="$3" template sum
  template="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)/packaging/aur/utre-bin"
  sum="$(sha256sum "$tarball" | cut -d' ' -f1)"
  cp "$template/PKGBUILD" "$template/utre-bin.install" "$dest/"
  sed -i \
    -e "s/^pkgver=.*/pkgver=$version/" \
    -e "s/^pkgrel=.*/pkgrel=1/" \
    -e "s/^sha256sums=.*/sha256sums=('$sum')/" \
    "$dest/PKGBUILD"
}
