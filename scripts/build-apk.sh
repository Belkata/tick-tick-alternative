#!/usr/bin/env bash
# Build the release APK and check which key signed it.
#
#   timeout 30m scripts/build-apk.sh [--allow-debug]
#
# Builds `flutter build apk --release --target-platform android-arm64`, copies the result to
# dist/utre-<version>-arm64.apk and compares its signing certificate (SHA-256) with
# packaging/android/release-cert.sha256 (override with $UTRE_RELEASE_CERT_SHA256).
#   - fingerprint set and different: fails (a phone on the release key would refuse the update)
#   - fingerprint empty: the release key isn't in use yet; fails unless --allow-debug is given
# UTRE_SKIP_BUILD=1 skips the build and checks the last APK again.
# Prints the certificate's SHA-256 and SHA-1 (Google's Android OAuth client wants the SHA-1).
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
# shellcheck source=env.sh
source "$root/scripts/env.sh"

allow_debug=0
for a in "$@"; do
  case "$a" in
    --allow-debug) allow_debug=1 ;;
    -h|--help) sed -n '2,14p' "$0"; exit 0 ;;
    *) echo "unknown argument: $a" >&2; exit 2 ;;
  esac
done

norm() { tr -d ': \t\r\n' | tr 'A-F' 'a-f'; }

expected=${UTRE_RELEASE_CERT_SHA256:-}
if [ -z "$expected" ] && [ -f "$root/packaging/android/release-cert.sha256" ]; then
  expected=$(grep -v '^#' "$root/packaging/android/release-cert.sha256" | norm || true)
fi
expected=$(printf '%s' "$expected" | norm)

version=$(sed -n 's/^version: *\([0-9][0-9.]*\).*/\1/p' "$root/app/pubspec.yaml" | head -1)
[ -n "$version" ] || { echo "no version in app/pubspec.yaml" >&2; exit 1; }

apksigner=$(find "${ANDROID_HOME:?ANDROID_HOME is not set}/build-tools" -name apksigner 2>/dev/null | sort -V | tail -1)
[ -n "$apksigner" ] || { echo "apksigner not found under $ANDROID_HOME/build-tools" >&2; exit 1; }

# apksigner is a shell wrapper that needs `java` (Android Studio's JBR, set by env.sh).
[ -z "${JAVA_HOME:-}" ] || export PATH="$JAVA_HOME/bin:$PATH"

# UTRE_SKIP_BUILD=1 re-checks the APK from the last build (used to test the key check).
if [ "${UTRE_SKIP_BUILD:-0}" != 1 ]; then
  (cd "$root/app" && flutter build apk --release --target-platform android-arm64)
fi

built="$root/app/build/app/outputs/flutter-apk/app-release.apk"
[ -f "$built" ] || { echo "missing $built" >&2; exit 1; }
mkdir -p "$root/dist"
out="$root/dist/utre-$version-arm64.apk"
cp "$built" "$out"

certs=$("$apksigner" verify --print-certs "$out")
sha256=$(printf '%s\n' "$certs" | sed -n 's/.*certificate SHA-256 digest: *//p' | head -1 | norm)
sha1=$(printf '%s\n' "$certs" | sed -n 's/.*certificate SHA-1 digest: *//p' | head -1 | norm)
[ -n "$sha256" ] || { echo "could not read the signing certificate of $out" >&2; exit 1; }

echo "APK:            $out"
echo "Signing SHA-256: $sha256"
echo "Signing SHA-1:   $sha1"

if [ -n "$expected" ]; then
  if [ "$sha256" != "$expected" ]; then
    echo "ERROR: $out is NOT signed with the release key." >&2
    echo "  expected SHA-256 $expected" >&2
    echo "  got      SHA-256 $sha256" >&2
    echo "Phones on the release key would refuse it. Check app/android/key.properties." >&2
    rm -f "$out"
    exit 1
  fi
  echo "OK: signed with the release key."
else
  echo "WARNING: packaging/android/release-cert.sha256 is empty, so this APK is signed with the" >&2
  echo "  debug key. Once the release key exists, put its SHA-256 in that file." >&2
  if [ "$allow_debug" -ne 1 ]; then
    echo "Refusing to continue without --allow-debug." >&2
    rm -f "$out"
    exit 1
  fi
fi
