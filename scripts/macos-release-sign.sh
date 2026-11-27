#!/usr/bin/env bash
# Sign the macOS release zip that CI built and upload the signature to the release.
#   scripts/macos-release-sign.sh X.Y.Z
# Downloads utre-X.Y.Z-macos.zip from release vX.Y.Z (macos-release.yml builds it), makes a binary
# detached OpenPGP signature utre-X.Y.Z-macos.zip.sig with UTRE_REPO_GPGKEY (default: the Utre
# repository key, as in scripts/pacman-repo-publish.sh), verifies it and uploads it. The secret
# key lives only on the release laptop, so this never runs in CI. The daemon's updater checks it.
set -euo pipefail

gh_repo="Belkata/utre"
key="${UTRE_REPO_GPGKEY:-8D9A9E543BB6D88237D174A78BE15C9843ADD341}"

version="${1:-}"
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo "usage: $0 X.Y.Z" >&2; exit 2; }
tag="v$version"
zip="utre-$version-macos.zip"

gpg --list-secret-keys "$key" >/dev/null 2>&1 \
  || { echo "secret key $key is not available in this keyring" >&2; exit 1; }

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

gh release download "$tag" --repo "$gh_repo" --pattern "$zip" --dir "$work" \
  || { echo "release $tag has no $zip (is macos-release.yml finished?)" >&2; exit 1; }
[[ -s "$work/$zip" ]] || { echo "$zip is missing or empty" >&2; exit 1; }

gpg --batch --yes --detach-sign --local-user "$key" --output "$work/$zip.sig" "$work/$zip"
gpg --verify "$work/$zip.sig" "$work/$zip"

gh release upload "$tag" --repo "$gh_repo" --clobber "$work/$zip.sig"
echo "Uploaded $zip.sig to $tag"
