#!/usr/bin/env bash
# List (default) or kill (--kill) processes that agents left behind.
#
#   scripts/agent-reap.sh [--kill] [--older-than MIN] [--worktree NAME]
#
# Finds, among YOUR processes only:
#   1. shell sleep-loops (bash/sh -c with while/until + sleep) older than MIN minutes (default 30);
#   2. `gh pr checks --watch` whose PR is merged or closed, and any `gh pr checks --watch` /
#      `gh run watch` older than MIN minutes;
#   3. anything whose cwd or command line is inside ../utre-wt/NAME (with --worktree NAME, regardless
#      of age), or inside any utre-wt/* directory that no longer exists.
# Never touched: this script and its ancestors, utre.service's main process and its children,
# and the installed release app (build/linux/.../bundle/utre).
set -euo pipefail

kill_it=0; older=30; only_wt=""
while [ $# -gt 0 ]; do
  case "$1" in
    --kill) kill_it=1 ;;
    --older-than) older="${2:?minutes}"; shift ;;
    --worktree) only_wt="${2:?name}"; shift ;;
    -h|--help) sed -n '2,12p' "$0"; exit 0 ;;
    *) echo "unknown option: $1" >&2; exit 2 ;;
  esac
  shift
done

repo="$(git -C "$(dirname "$0")" rev-parse --show-toplevel)"
# In a worktree, the main checkout's sibling dir is the same utre-wt root.
wt_root="$(dirname "$repo")/utre-wt"
[ "$(basename "$(dirname "$repo")")" = utre-wt ] && wt_root="$(dirname "$repo")"
limit=$((older * 60))

declare -A args_of ppid_of age_of protect
pids=()
while read -r pid ppid et rest; do
  pids+=("$pid"); ppid_of[$pid]="$ppid"; age_of[$pid]="$et"; args_of[$pid]="$rest"
done < <(ps -u "$(id -u)" -o pid=,ppid=,etimes=,args=)

descendants() { # pid -> prints pid and all descendants
  local p="$1" c
  echo "$p"
  for c in "${pids[@]}"; do
    [ "${ppid_of[$c]}" = "$p" ] && descendants "$c"
  done
}

# Protected: self and ancestors.
p=$$
while [ -n "$p" ] && [ "$p" -gt 1 ]; do
  protect[$p]=1
  p="${ppid_of[$p]:-}"
done
# Protected: the real daemon, its children, and the release app.
dpid="$(systemctl --user show -p MainPID --value utre.service 2>/dev/null || true)"
if [ -n "$dpid" ] && [ "$dpid" != 0 ]; then
  while read -r d; do protect[$d]=1; done < <(descendants "$dpid")
fi
for pid in "${pids[@]}"; do
  case "${args_of[$pid]}" in
    */build/linux/*/bundle/utre*|*/usr/bin/utre-app*) protect[$pid]=1 ;;
  esac
done

declare -A reason=()
mark() { [ -n "${protect[$1]:-}" ] || reason[$1]="${reason[$1]:-$2}"; }

pr_state() { gh pr view "$1" --json state -q .state 2>/dev/null || echo UNKNOWN; }

for pid in "${pids[@]}"; do
  a="${args_of[$pid]}"; et="${age_of[$pid]}"
  # 1. shell sleep loops
  if [[ "$a" =~ ^(/[^[:space:]]*/)?(bash|sh|zsh)[[:space:]] ]] && [[ "$a" =~ (while|until)[[:space:]] ]] \
     && [[ "$a" =~ sleep[[:space:]] ]] && [ "$et" -gt "$limit" ] && [[ "$a" != *agent-reap.sh* ]]; then
    mark "$pid" "sleep loop, $((et / 60)) min old"
  fi
  # 2. gh watchers
  if [[ "$a" =~ ^(/[^[:space:]]*/)?gh[[:space:]]+pr[[:space:]]+checks[[:space:]]+([0-9]+).*(--watch|-w) ]]; then
    st="$(pr_state "${BASH_REMATCH[2]}")"
    if [ "$st" = MERGED ] || [ "$st" = CLOSED ]; then mark "$pid" "gh pr checks --watch on $st PR ${BASH_REMATCH[2]}"
    elif [ "$et" -gt "$limit" ]; then mark "$pid" "gh pr checks --watch, $((et / 60)) min old"; fi
  elif [[ "$a" =~ ^(/[^[:space:]]*/)?gh[[:space:]]+run[[:space:]]+watch ]] && [ "$et" -gt "$limit" ]; then
    mark "$pid" "gh run watch, $((et / 60)) min old"
  fi
  # 3. worktree leftovers
  cwd="$(readlink "/proc/$pid/cwd" 2>/dev/null || true)"
  where=""
  for s in "$cwd" "$a"; do
    if [[ "$s" =~ $wt_root/([^/[:space:]]+) ]]; then where="${BASH_REMATCH[1]}"; break; fi
  done
  if [ -n "$where" ]; then
    if [ -n "$only_wt" ]; then
      [ "$where" = "$only_wt" ] && mark "$pid" "inside worktree $where"
    elif [ ! -d "$wt_root/$where" ]; then
      mark "$pid" "inside removed worktree $where"
    fi
  fi
done

if [ "${#reason[@]}" -eq 0 ]; then echo "nothing to reap"; exit 0; fi

for pid in "${!reason[@]}"; do
  printf '%s pid %s: %s\n    %s\n' "$([ "$kill_it" = 1 ] && echo kill || echo found)" "$pid" "${reason[$pid]}" "${args_of[$pid]:0:200}"
done
[ "$kill_it" = 1 ] || { echo "(list only; re-run with --kill to stop them)"; exit 0; }

victims=()
for pid in "${!reason[@]}"; do
  while read -r d; do [ -n "${protect[$d]:-}" ] || victims+=("$d"); done < <(descendants "$pid")
done
kill -TERM "${victims[@]}" 2>/dev/null || true
sleep 2
for v in "${victims[@]}"; do kill -0 "$v" 2>/dev/null && kill -KILL "$v" 2>/dev/null || true; done
echo "stopped ${#victims[@]} process(es)"
