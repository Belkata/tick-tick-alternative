#!/usr/bin/env bash
# Publish utre-bin to our own pacman repository, hosted as assets of the GitHub release
# `arch-repo` on Belkata/utre (repo name `utre`; see packaging/arch/README.md).
#   scripts/pacman-repo-publish.sh X.Y.Z [--dry-run] [--unsigned]
# Needs dist/utre-X.Y.Z-linux-x86_64.tar.gz (scripts/build-linux-tarball.sh) and gh logged in.
# Packages and databases are signed with UTRE_REPO_GPGKEY (default: the Utre repository key, whose
# public half is packaging/arch/utre-repo.asc). A missing secret key is an error; --unsigned opts out.
# --dry-run builds everything into a temp dir and lists what would change; GitHub is only read.
# The packaging/aur/utre-bin/ template is never bumped; versions exist only in the built package.
set -euo pipefail
repo="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=lib/utre-bin.sh
source "$repo/scripts/lib/utre-bin.sh"

gh_repo="Belkata/utre"
tag="arch-repo"
keep=2

version=""
dry=0
unsigned=0
for arg in "$@"; do
  case "$arg" in
    --dry-run) dry=1 ;;
    --unsigned) unsigned=1 ;;
    -*) echo "unknown option: $arg" >&2; exit 2 ;;
    *) version="$arg" ;;
  esac
done
[[ -n "$version" ]] || { echo "usage: $0 X.Y.Z [--dry-run] [--unsigned]" >&2; exit 2; }

tarball="$repo/dist/utre-$version-linux-x86_64.tar.gz"
[[ -f "$tarball" ]] || { echo "missing $tarball (run scripts/build-linux-tarball.sh)" >&2; exit 1; }
if [[ $dry == 0 ]]; then
  tree="$(sed -n 's/^version: *\([^+ ]*\).*/\1/p' "$repo/app/pubspec.yaml" | head -n1)"
  [[ "$tree" == "$version" ]] || {
    echo "working tree is version $tree, not $version" >&2
    exit 1
  }
fi

sign=()
if [[ $unsigned == 1 ]]; then
  echo "warning: --unsigned, building without signatures" >&2
else
  key="${UTRE_REPO_GPGKEY:-8D9A9E543BB6D88237D174A78BE15C9843ADD341}"
  gpg --list-secret-keys "$key" >/dev/null 2>&1 || {
    echo "secret key $key is not in the keyring (use --unsigned to publish without signing)" >&2
    exit 1
  }
  sign=(--sign --key "$key")
fi

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
build="$work/build"; out="$work/repo"
mkdir -p "$build" "$out"

# makepkg finds the tarball under the local name of the PKGBUILD's source entry and skips the download.
fill_utre_bin_template "$build" "$version" "$tarball"
cp "$tarball" "$build/utre-bin-$version.tar.gz"
(cd "$build" && PKGDEST="$out" makepkg -f --noconfirm "${sign[@]}")
pkg="$(basename "$(ls "$out"/utre-bin-"$version"-1-x86_64.pkg.tar.zst)")"

# The current index, if the release exists yet.
existing=()
if gh release view "$tag" --repo "$gh_repo" >/dev/null 2>&1; then
  gh release download "$tag" --repo "$gh_repo" --dir "$out" \
    --pattern utre.db.tar.gz --pattern utre.files.tar.gz || true
  mapfile -t existing < <(gh release view "$tag" --repo "$gh_repo" --json assets \
    --jq '.assets[].name' | grep -E '^utre-bin-.*\.pkg\.tar\.zst$' || true)
  release_exists=1
else
  release_exists=0
fi

(cd "$out" && repo-add "${sign[@]}" utre.db.tar.gz "$pkg")
# GitHub has no symlinks, so utre.db and utre.files are real copies.
cp --remove-destination -L "$out/utre.db.tar.gz" "$out/utre.db"
cp --remove-destination -L "$out/utre.files.tar.gz" "$out/utre.files"
if [[ ${#sign[@]} -gt 0 ]]; then
  cp --remove-destination -L "$out/utre.db.tar.gz.sig" "$out/utre.db.sig"
  cp --remove-destination -L "$out/utre.files.tar.gz.sig" "$out/utre.files.sig"
fi

# Keep the newest $keep packages (for downgrades), delete older ones with their signatures.
mapfile -t all < <(printf '%s\n' "${existing[@]}" "$pkg" | sort -uV)
stale=()
if [[ ${#all[@]} -gt $keep ]]; then
  stale=("${all[@]:0:${#all[@]}-keep}")
fi

uploads=("$out/$pkg" "$out"/utre.db "$out"/utre.files "$out"/utre.db.tar.gz "$out"/utre.files.tar.gz)
for extra in "$out/$pkg.sig" "$out"/utre.db.sig "$out"/utre.files.sig \
  "$out"/utre.db.tar.gz.sig "$out"/utre.files.tar.gz.sig; do
  [[ -f "$extra" ]] && uploads+=("$extra")
done

echo "== release $tag $([[ $release_exists == 1 ]] && echo exists || echo 'will be created (prerelease)')"
echo "== upload:"; printf '   %s\n' "${uploads[@]##*/}"
echo "== delete:"; printf '   %s\n' "${stale[@]:-}" | sed '/^ *$/d'
if [[ $dry == 1 ]]; then
  echo "== dry run: GitHub untouched. Index: $(tar tzf "$out/utre.db" | tr '\n' ' ')"
  keepdir="$repo/dist/pacman-repo-dryrun"
  rm -rf "$keepdir"; mkdir -p "$keepdir"; cp -L "$out"/* "$keepdir/"
  echo "== files copied to $keepdir"
  exit 0
fi

if [[ $release_exists == 0 ]]; then
  notes="Pacman repository for Utre. Add to /etc/pacman.conf:

    [utre]
    Server = https://github.com/$gh_repo/releases/download/$tag

First trust the signing key (fingerprint 8D9A9E543BB6D88237D174A78BE15C9843ADD341):

    curl -LO https://raw.githubusercontent.com/$gh_repo/main/packaging/arch/utre-repo.asc
    sudo pacman-key --add utre-repo.asc
    sudo pacman-key --lsign-key 8D9A9E543BB6D88237D174A78BE15C9843ADD341

Then: sudo pacman -Sy utre-bin"
  gh release create "$tag" --repo "$gh_repo" --prerelease \
    --target "$(git -C "$repo" rev-parse origin/main)" \
    --title "Arch package repository" --notes "$notes"
fi
gh release upload "$tag" --repo "$gh_repo" --clobber "${uploads[@]}"
for old in "${stale[@]}"; do
  for asset in "$old" "$old.sig"; do
    gh release delete-asset "$tag" "$asset" --repo "$gh_repo" --yes 2>/dev/null || true
  done
done
echo "published utre-bin $version to the $tag release"
