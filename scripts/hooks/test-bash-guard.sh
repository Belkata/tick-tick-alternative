#!/usr/bin/env bash
# Feeds sample Bash-tool payloads to bash-guard.sh and checks block (exit 2) vs allow (exit 0).
set -euo pipefail
guard="$(dirname "$0")/bash-guard.sh"
fail=0

run() { # expected-exit command
  local want="$1" cmd="$2" got=0
  jq -n --arg c "$cmd" '{tool_name:"Bash",tool_input:{command:$c}}' | "$guard" >/dev/null 2>&1 || got=$?
  if [ "$got" -ne "$want" ]; then
    printf 'FAIL (want %s, got %s): %s\n' "$want" "$got" "$cmd"
    fail=1
  fi
}

# Blocked: the commands that got agents stuck.
run 2 'while pgrep -f "flutter build apk|gradlew" >/dev/null; do sleep 15; done; echo done'
run 2 'while pgrep -f update-desktop.sh >/dev/null; do sleep 10; done'
run 2 'until grep -q "^exit" /tmp/x/upd.log; do sleep 10; done'
run 2 'until grep -q ALLDONE /tmp/scratch/all.log; do sleep 10; done'
run 2 'until gh pr checks 31 | grep -q "pass\|fail"; do sleep 5; done'
run 2 $'while true\ndo\n  sleep 5\ndone'
run 2 'gh pr checks 31 --watch'
run 2 'cd x && gh pr checks 31 --watch --interval 30'
run 2 'gh run watch 123'
run 2 'sleep 120'
run 2 'sleep 5m'
run 2 'echo hi; sleep 3600 && echo done'
run 2 'pkill -f flutter_tester'
run 2 'pkill utre'
run 2 'killall dart'
run 2 'cd x && pkill -9 -f gradle'

# Allowed: normal work, and bounded variants.
run 0 'cargo test -p utre-core'
run 0 'timeout 20m cargo test --workspace'
run 0 'cd app && flutter analyze && flutter test'
run 0 'git status && git log --oneline -5'
run 0 'gh pr create --title x --body y'
run 0 'gh pr merge 31 --rebase --delete-branch'
run 0 'gh pr view 31 --json state'
run 0 'gh pr checks 31'
run 0 'timeout 30m gh pr checks 31 --watch'
run 0 'timeout 30m gh run watch 123'
run 0 'for f in scripts/*.sh; do shellcheck "$f"; done'
run 0 'for i in 1 2 3; do echo $i; sleep 1; done'
run 0 'sleep 1'
run 0 'sleep 60'
run 0 "timeout 10m bash -c 'until test -f /tmp/done; do sleep 2; done'"
run 0 'scripts/update-desktop.sh --no-restart'
run 0 'git commit -m "fix: stop the wait loop"'
run 0 'kill 12345'
run 0 'kill -9 12345'
run 0 'kill -TERM 12345'
run 0 'scripts/agent-reap.sh --kill --worktree x'
run 0 'systemctl --user restart utre.service'

[ "$fail" -eq 0 ] && echo "bash-guard: all cases ok"
exit "$fail"
