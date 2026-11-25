#!/usr/bin/env bash
# Make this laptop's daemon, `utre` and desktop app match the checkout, rebuilding only what
# changed since the last run (each part is fingerprinted by the content of its files).
#   scripts/update-desktop.sh           build what changed, restart the daemon, relaunch the app
#   scripts/update-desktop.sh --all     rebuild everything regardless
#   scripts/update-desktop.sh --no-restart   build only
#
# The daemon and `utre` use the `fast` profile (Cargo.toml); the app is Flutter's release build.
# Before restarting the daemon the database is backed up (once per schema version and day),
# since a new build may migrate it.
set -euo pipefail
repo="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo"
# shellcheck source=env.sh
source "$repo/scripts/env.sh"

force=0
restart=1
for arg in "$@"; do
  case "$arg" in
    --all) force=1 ;;
    --no-restart) restart=0 ;;
    *) echo "unknown option: $arg" >&2; exit 2 ;;
  esac
done

stamps="$repo/target/.update-desktop"
mkdir -p "$stamps"

# A hash of the files' contents, uncommitted edits included.
fingerprint() {
  git ls-files -z -- "$@" | xargs -0 sha1sum | sha1sum | cut -d' ' -f1
}
# Run $2... when part $1's files changed (or --all); remember the new fingerprint after.
changed() {
  local part="$1"; shift
  local now; now="$(fingerprint "$@")"
  [[ $force == 1 || ! -f "$stamps/$part" || "$(cat "$stamps/$part")" != "$now" ]] \
    && { echo "$now" > "$stamps/$part.new"; return 0; } || return 1
}
done_part() { mv "$stamps/$1.new" "$stamps/$1"; }

rust_paths=(core api sync google daemon cli Cargo.toml Cargo.lock)
app_paths=("${rust_paths[@]}" app/lib app/rust app/linux app/pubspec.yaml app/pubspec.lock)

daemon_built=0
if changed daemon "${rust_paths[@]}"; then
  echo "== utre-daemon and utre (fast profile)"
  cargo build --profile fast -p utre-daemon -p utre
  done_part daemon
  daemon_built=1
fi

app_built=0
if changed app "${app_paths[@]}"; then
  echo "== desktop app (Flutter release)"
  (cd app && flutter build linux --release)
  done_part app
  app_built=1
fi

[[ $restart == 1 ]] || { echo "Built; not restarting."; exit 0; }

if [[ $daemon_built == 1 || $force == 1 ]]; then
  db="${UTRE_DB:-$HOME/.local/share/utre/utre.db}"
  if [[ -f "$db" ]] && command -v sqlite3 >/dev/null; then
    version="$(sqlite3 "$db" 'PRAGMA user_version;')"
    backup_dir="$(dirname "$db")/backups"
    backup="$backup_dir/utre-v$version-$(date +%F).db"
    mkdir -p "$backup_dir"
    [[ -f "$backup" ]] || { sqlite3 "$db" ".backup '$backup'"; echo "Backed up the database to $backup"; }
  fi
  echo "== restarting utre-daemon"
  "$repo/scripts/install-user-service.sh"
fi

if [[ $app_built == 1 || $force == 1 ]]; then
  if [[ -n "${WAYLAND_DISPLAY:-}${DISPLAY:-}" ]]; then
    echo "== relaunching the app"
    pkill -x utre || true
    sleep 1
    setsid -f "$repo/app/build/linux/x64/release/bundle/utre" >/dev/null 2>&1
  else
    echo "No display here: start the app yourself."
  fi
fi
echo "Up to date."
