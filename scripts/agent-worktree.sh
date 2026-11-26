#!/usr/bin/env bash
# Create (or remove) an isolated worktree for one agent.
#
#   scripts/agent-worktree.sh new <name> [base-branch]   # default base: main
#   scripts/agent-worktree.sh rm  <name>
#
# New worktrees live in ../utre-wt/<name> on branch agent/<name>. Next to the checkout,
# not inside it, so Cargo and Flutter don't scan them. Each gets an `agent.env` to source:
# it points $UTRE_DB and $UTRE_SOCKET at a private scratch DB and a short socket, so an agent
# never touches your running daemon or its database (see "Gotchas": macOS caps socket paths
# at 104 bytes, so the socket path stays short).
set -euo pipefail

cmd="${1:-}"; name="${2:-}"
[ -n "$cmd" ] && [ -n "$name" ] || { sed -n '2,10p' "$0"; exit 2; }

# The main checkout, even when run from inside a worktree (whose --show-toplevel is the worktree).
common="$(git rev-parse --path-format=absolute --git-common-dir)"
repo="$(dirname "$common")"
wt_root="$(dirname "$repo")/utre-wt"
wt="$wt_root/$name"
branch="agent/$name"

case "$cmd" in
  new)
    base="${3:-main}"
    mkdir -p "$wt_root"
    git -C "$repo" worktree add -b "$branch" "$wt" "$base"
    # Same commit identity as the main checkout (the global git email is a work address).
    git -C "$wt" config user.email "$(git -C "$repo" config user.email)"
    git -C "$wt" config user.name  "$(git -C "$repo" config user.name)"
    sock="/tmp/utre-$name.sock"
    cat > "$wt/agent.env" <<EOF
# source this in the agent's shell: source agent.env
source "$wt/scripts/env.sh"
export UTRE_DB="$wt/.agent/utre.db"
export UTRE_SOCKET="$sock"
export CARGO_TARGET_DIR="$wt/target"
mkdir -p "$wt/.agent"
EOF
    # Keep the private bits out of git without touching the shared .gitignore.
    printf 'agent.env\n.agent/\n' >> "$(git -C "$wt" rev-parse --git-path info/exclude)"
    echo "worktree: $wt (branch $branch)"
    echo "start the agent there, and have it run: source agent.env"
    ;;
  rm)
    # Stop anything the agent left running there (loops, watchers, test daemons) first.
    "$repo/scripts/agent-reap.sh" --kill --worktree "$name" || true
    rm -f "/tmp/utre-$name.sock"
    if [ -d "$wt" ]; then git -C "$repo" worktree remove --force "$wt"; else git -C "$repo" worktree prune; fi
    echo "removed $wt; the branch $branch is kept (delete with: git branch -D $branch)"
    ;;
  *)
    sed -n '2,10p' "$0"; exit 2 ;;
esac
