#!/usr/bin/env bash
# Claude Code PreToolUse hook for the Bash tool (registered in .claude/settings.json).
# Reads the hook JSON on stdin; exit 0 allows, exit 2 blocks and sends stderr to the agent.
# Blocks the commands that left agents stuck for hours: self-matching pgrep wait loops,
# unbounded sleep loops, unbounded `gh ... --watch`, and long bare sleeps.
# Legitimate bypass: put the whole command under `timeout <dur>`.
set -euo pipefail

input="$(cat)"
if command -v jq >/dev/null 2>&1; then
  cmd="$(printf '%s' "$input" | jq -r '.tool_input.command // empty')"
elif command -v python3 >/dev/null 2>&1; then
  cmd="$(printf '%s' "$input" | python3 -I -c 'import json,sys; print(json.load(sys.stdin).get("tool_input",{}).get("command") or "")')"
else
  exit 0
fi
[ -n "$cmd" ] || exit 0
flat="${cmd//$'\n'/;}"

block() {
  printf 'BLOCKED by scripts/hooks/bash-guard.sh: %s\n' "$1" >&2
  exit 2
}

# True when a `timeout <duration>` appears in the text before the first match of $2.
bounded_before() {
  local text="$1" first="$2" prefix
  prefix="${text%%"$first"*}"
  [[ "$prefix" =~ (^|[^[:alnum:]_-])timeout[[:space:]]+(-[^[:space:]]+[[:space:]]+)*[0-9] ]]
}

loop_re='(^|[;&|({[:space:]"'"'"'])(while|until)[[:space:]]'

# 1. Wait loops on pgrep: `pgrep -f` matches the waiting shell's own command line, so it never ends.
if [[ "$flat" =~ ${loop_re}[^\;]*pgrep ]]; then
  block "a while/until loop that uses pgrep never ends (pgrep -f matches the loop's own shell). Do not poll for processes. Run the long command with run_in_background and wait for its completion notification, or wait on a PID you started: 'timeout 30m tail --pid=<PID> -f /dev/null'."
fi

# 2. Any while/until loop with sleep needs the whole command bounded by timeout.
if [[ "$flat" =~ $loop_re ]]; then
  loop_m="${BASH_REMATCH[0]}"
  if [[ "$flat" =~ (^|[^[:alnum:]_-])sleep[[:space:]] ]] && ! bounded_before "$flat" "$loop_m"; then
    block "a while/until loop with sleep has no upper bound. Do not write polling loops: use run_in_background and wait for the notification. If you really need one, wrap it: timeout 10m bash -c '...'."
  fi
fi

# 3. gh watchers must be bounded.
if [[ "$flat" =~ (^|[^[:alnum:]_-])gh[[:space:]]+pr[[:space:]]+checks[^\;\&\|]*(--watch|[[:space:]]-w([[:space:]]|$)) ]] ||
   [[ "$flat" =~ (^|[^[:alnum:]_-])gh[[:space:]]+run[[:space:]]+watch ]]; then
  m="${BASH_REMATCH[0]}"
  bounded_before "$flat" "$m" || block "gh pr checks --watch / gh run watch can hang forever. Prefix it: timeout 30m gh pr checks <n> --watch, and run it with run_in_background."
fi

# 4. Long bare sleeps.
rest="$flat"
sleep_re='(^|[^[:alnum:]_-])sleep[[:space:]]+([0-9]+)([smhd]?)'
while [[ "$rest" =~ $sleep_re ]]; do
  n="${BASH_REMATCH[2]}"; unit="${BASH_REMATCH[3]}"
  case "$unit" in m) n=$((n * 60)) ;; h) n=$((n * 3600)) ;; d) n=$((n * 86400)) ;; *) ;; esac
  if [ "$n" -gt 60 ]; then
    block "sleep longer than 60 s. Use run_in_background and wait for the completion notification instead of sleeping."
  fi
  rest="${rest#*"${BASH_REMATCH[0]}"}"
done

exit 0
