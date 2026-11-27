#!/usr/bin/env bash
# Make this laptop's daemon, `utre` and desktop app match the checkout, rebuilding only what
# changed since the last run (each part is fingerprinted by the content of its files).
#   scripts/update-desktop.sh           build what changed, restart the daemon, relaunch the app
#   scripts/update-desktop.sh --all     rebuild everything regardless
#   scripts/update-desktop.sh --no-restart   build only
#   scripts/update-desktop.sh --check   print "packaged" or "checkout" and exit
#
# Only for machines running the checkout build. When utre-bin or utre-git (pacman) is installed,
# updates come from `sudo pacman -Syu`; this script then refuses to restart anything.
#
# The daemon and `utre` use the `fast` profile (Cargo.toml); the app is Flutter's release build.
# Before restarting the daemon the database is backed up (once per schema version and day),
# since a new build may migrate it.
set -euo pipefail
repo="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo"
# shellcheck source=env.sh
source "$repo/scripts/env.sh"

# Is utre.service (or a pacman package) the packaged install rather than this checkout?
is_packaged() {
  local frag
  if pacman -Q utre-bin >/dev/null 2>&1 || pacman -Q utre-git >/dev/null 2>&1; then return 0; fi
  frag="$(systemctl --user show -p FragmentPath --value utre.service 2>/dev/null || true)"
  [[ "$frag" == /usr/lib/* ]]
}

# PIDs of processes whose executable is exactly $1 (never matches by name).
pids_of_exe() {
  local p
  for p in /proc/[0-9]*; do
    [[ "$(readlink "$p/exe" 2>/dev/null)" == "$1" ]] && echo "${p#/proc/}"
  done
  return 0
}

force=0
restart=1
for arg in "$@"; do
  case "$arg" in
    --all) force=1 ;;
    --no-restart) restart=0 ;;
    --check) is_packaged && echo packaged || echo checkout; exit 0 ;;
    *) echo "unknown option: $arg" >&2; exit 2 ;;
  esac
done

if [[ $restart == 1 ]] && is_packaged; then
  {
    echo "This machine runs the packaged install (utre-bin), so I won't restart the daemon or"
    echo "relaunch the app: they must come from the same build. Update with:"
    echo "  sudo pacman -Syu"
    echo "  systemctl --user restart utre.service   # then reopen the app"
    echo "(Use --no-restart to build only.)"
  } >&2
  exit 1
fi

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
    bundle="$repo/app/build/linux/x64/release/bundle/utre"
    # Stop only this checkout's app, by executable path (the packaged app is also named utre).
    for pid in $(pids_of_exe "$(readlink -f "$bundle")"); do kill "$pid" || true; done
    sleep 1
    setsid -f "$bundle" >/dev/null 2>&1
  else
    echo "No display here: start the app yourself."
  fi
fi
echo "Up to date."
