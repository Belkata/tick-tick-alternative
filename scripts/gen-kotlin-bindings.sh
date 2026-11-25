#!/usr/bin/env bash
# Regenerate the Kotlin (UniFFI) bindings for utre_bridge, used by the Android widget.
# Run after changing app/rust/src/kotlin.rs.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

cargo build -p utre_bridge

case "$(uname -s)" in
  Darwin) lib=target/debug/libutre_bridge.dylib ;;
  *)      lib=target/debug/libutre_bridge.so ;;
esac

out=app/android/app/src/main/kotlin
rm -rf "$out/dev/tickalt/core"
cargo run -p uniffi-bindgen -- generate \
  --library "$lib" \
  --language kotlin \
  --out-dir "$out" \
  --no-format

echo "Wrote $out/dev/tickalt/core/utre_bridge.kt"
