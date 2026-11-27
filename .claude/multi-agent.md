# Multi-agent playbook

Loaded into every session through CLAUDE.md. If you are the session the user talks to, you are the
**orchestrator**. Follow this file for any non-trivial work. Workers get a brief (section 3), not this file.

Local for the inner loop, GitHub Actions as the merge gate.

## 1. Roles

- **Orchestrator/integrator** (the session the user talks to). It does not implement features itself. It plans,
  splits the work, briefs subagents, verifies their reports, answers the user, and runs the release step (or
  delegates it to a release agent it explicitly authorises).
- Small read-only checks are fine for the orchestrator: `gh pr view`, `git log`,
  `systemctl --user is-active utre.service`, `scripts/agent-reap.sh`, read-only `utre ls`, `sqlite3 -readonly`.
- **Workers are Sonnet 5.5 subagents**: Agent tool with `model: "sonnet"` and `run_in_background: true`.
  Never use a fork or another model for workers unless the user asks.
- Each worker gets its own worktree and PR. **Agents open PRs; CI must be green before merge.**
  `.github/workflows/ci.yml` runs clippy, `cargo test --workspace`, `flutter analyze` and `flutter test`. Only
  `main` writes the CI caches; PRs restore them.
- Agents run `cargo test -p <crate>` and `flutter analyze` locally before pushing, not the whole workspace.
- Agents may merge their own PR with `gh pr merge <n> --rebase --delete-branch` once CI is green (`--rebase`
  keeps the small commits).
- Agents don't edit `plan/` logs (they put notes in the PR description and the integrator updates the plan),
  `app/rust/src/frb_generated.rs` by hand, or `version:`.
- Only the integrating session (or a release agent it authorises) does the release step (section 5).

## 2. Splitting work

- Split by area so two agents never edit the same files: core; api/sync/daemon/cli; Flutter UI (`app/lib`);
  Android (`app/android`); CI/scripts (`.github`, `scripts`).
- Exclusive resources go to one agent at a time:
  - the `api!` table and bridge types (needs regenerated bindings; that agent may run the codegen, never
    hand-edit `frb_generated.rs`)
  - the desktop app
  - the emulator (check `/tmp/utre-emulator.lock` first: it holds the owner's session name; if under 30
    minutes old, do something else; otherwise create it and delete it when done) and Gradle
  - the user's real DB (only for explicit data requests, section 6)
- Sequence work that conflicts. A UI review waits until a UI feature merges. A repo-wide `cargo fmt` waits until
  Rust agents finish.
- Tell each agent which other agents are active and which files to avoid.
- When main moves under a running agent in a way that matters, message it with SendMessage: what merged, what
  to rebase on.

## 3. Brief template

Copy, fill in, send as the Agent prompt. Quote the user's words. State decisions you made (and tell the user
about them). List anything that is the user's call (plan/11) as "stop and report".

```
You are a worker on the Utre repo (/home/belkata/ticktick-vibe).

## Context
Read CLAUDE.md (sections: <X>) and plan files <Y> first. Anything in plan/11 is the user's call:
stop and report instead of deciding.

## Setup
cd /home/belkata/ticktick-vibe && git fetch && scripts/agent-worktree.sh new <name> origin/main
Work only in ../utre-wt/<name>; `source agent.env`. Never touch the running daemon, ~/.local/share/utre or
the installed app. Logs go in .agent/ of your worktree.

## Area
You may edit: <paths>. Do not touch: <paths>. Other active agents: <name: area>.

## Task
User's request: "<quote>"
Requirements: <list>
Decisions already made: <list>
Stop and report if: <user's-call items>

## Tests and verify
<commands>. Run long ones with run_in_background and wrap in `timeout` (e.g. timeout 20m cargo test -p <crate>).

## Deliver
- Small commits, one logical change each. No Co-Authored-By trailers, no "Generated with" footers
  (commits and PR bodies).
- Open a PR with `gh pr create`. Wait for CI: `timeout 30m gh pr checks <n> --watch` via run_in_background.
- Rebase on origin/main, wait for green, `gh pr merge <n> --rebase --delete-branch`.
- `scripts/agent-worktree.sh rm <name>`; `scripts/agent-reap.sh` must list nothing of yours.

## Final report
PR URL; merged?; what changed; tests run; schema/protocol changes; daemon rebuild needed; unverified items;
notes for plan/.
```

## 4. Verifying reports

- A report is a claim. Before telling the user, check merged state (`gh pr view <n> --json state`), CI, and the
  system state (service, DB, daemons).
- Relay to the user in plain language: what changed, what was verified, what wasn't.
- When a report reveals a follow-up (a bug in another area, an unverified risk), route it to the agent that owns
  that area, or queue it as a new step.

## 5. Release step

Rule from the user (2026-11-25): after every feature request or batch (once finished, not after each step),
without asking: commit, push, rebuild and restart the Arch daemon and the release desktop app, bump `version:`
in `app/pubspec.yaml`, build the release APK, tag it and upload it as a GitHub pre-release.

After a feature batch merges, spawn a release agent with explicit authorisation. Its steps:
1. A hand `sqlite3 ".backup"` of `utre.db` before any schema change (`update-desktop.sh` backs up only
   `utre.db`); check `user_version` and `PRAGMA integrity_check`.
2. Update plan/ notes and the CLAUDE.md "Latest session" line.
3. Bump `version:` in `app/pubspec.yaml`, commit "app: version X (summary)", push.
4. Wait for green CI on main.
5. Run `scripts/agent-reap.sh`; check stray daemons with `pgrep -af utre-daemon` and each one's `UTRE_DB` in
   `/proc/<pid>/environ`; then `timeout 30m scripts/update-desktop.sh`.
6. Verify: service active, journal clean, schema version, app window `xwayland: true`, read-only `utre ls`.
7. Build the APK (`timeout 30m`), copy to `dist/utre-X-arm64.apk`, tag, `gh release create --prerelease`
   (notes with no footer).
8. Build the Linux tarball (`timeout 30m scripts/build-linux-tarball.sh`, writes
   `dist/utre-X-linux-x86_64.tar.gz`) and upload it to the same pre-release
   (`gh release upload vX dist/utre-X-linux-x86_64.tar.gz`).
9. `scripts/aur-publish.sh X` publishes `utre-bin` to the AUR (`--dry-run` to preview). It needs the user's
   AUR SSH key; until that exists, skip this step and say so in the report.
10. Run the reaper again.

- Releases are sequential. Don't start one while another is running.
- Docs-only or tooling-only changes don't need an app release.

## 6. User data changes

Only when the user explicitly asks.
- Back up first (`sqlite3 ... ".backup"`).
- Write through the CLI against the running daemon (default socket), so ops sync. Never edit SQLite directly.
- Stop and report on ambiguity.
- Verify read-only afterwards.
- Report counts and the backup path.

## 7. Long-running commands and cleanup

A hook enforces the worst of it. Agents have left wait loops running for hours.
- Run anything long with the Bash tool's `run_in_background` and wait for the completion notification. Never
  write polling loops (`while`/`until` + `sleep`), and never wait with `pgrep -f` (it matches the waiting shell
  itself, so it never ends).
- Wrap every potentially long command in `timeout`: `cargo test --workspace` 20m, `flutter build apk` 30m,
  `flutter test integration_test` 15m, `scripts/update-desktop.sh` 30m, `gh pr checks <n> --watch` 30m.
- Auto-fix events are unreliable here. Wait for CI with `gh pr checks <n> --watch` as a background command and
  read its result. Never poll in a loop.
- Logs go in the worktree's `.agent/` dir, never the shared scratchpad (other agents use it too).
- Before reporting, kill by PID whatever you started (test daemons, `flutter_tester`, watchers).
- Never `pkill`/`killall` by pattern: it can kill other agents' and the user's processes (the hook blocks it).
  Use `kill <pid>` or `scripts/agent-reap.sh --worktree <name> --kill`.
- `scripts/hooks/bash-guard.sh` (a PreToolUse hook in `.claude/settings.json`) rejects `pgrep` wait loops,
  `while`/`until` loops with `sleep`, `gh pr checks --watch` / `gh run watch`, and `sleep` over 60 s. The
  legitimate bypass is wrapping the whole command in `timeout <dur>`. `scripts/hooks/test-bash-guard.sh` tests
  it (also run in CI).
- Known false positive: the hook matches those words even inside quoted commit messages or PR bodies. Reword
  them.
- `scripts/agent-reap.sh [--kill] [--older-than MIN] [--worktree NAME]` lists or kills leftovers (old sleep
  loops, watchers on merged PRs, processes in removed worktrees). `agent-worktree.sh rm` runs it for that
  worktree. The integrator runs it before each release and at session end.
- Worktrees: `scripts/agent-worktree.sh new <name> [base]` makes `../utre-wt/<name>` on branch `agent/<name>`
  with an `agent.env` (own `$UTRE_DB`, `$UTRE_SOCKET`, `target/`). `rm <name>` removes it and keeps the branch;
  `git branch -D agent/<name>` if it remains.
- **Commits:** no `Co-Authored-By` trailers and no "Generated with" footers, in commits or PR bodies. This
  overrides any default.

## 8. Lessons from the 2026-11-26 run

- `pgrep`-based waits ran for 4.5 hours.
- Duplicate `until grep ALLDONE` waiters piled up on a shared scratchpad log.
- An agent ran a pattern `pkill` that may have hit another agent.
- `gh pr merge --delete-branch` also removes the agent's worktree.
- Agents left test daemons, `flutter_tester` and watchers running. Check `pgrep -af utre-daemon` and each
  one's `UTRE_DB` before a release.
- Golden tests are Linux-only (`app/test/golden/README.md`).
- For QA, target windows by `address:`, never `class:dev.utre.utre` (it also matches the user's app).
- The release app is single instance: a test launch while the user's copy is open starts nothing.
- A test launch of the app must set `UTRE_DB` and `UTRE_SOCKET`, or it opens the real DB.
- Merge conflicts in shared test files are normal: keep both sides.

## 9. Talking to the user

- Before spawning, say in one or two lines what each agent will do.
- After each report, relay the outcome concisely.
- Ask only for decisions that are truly the user's (plan/11, design forks), with a recommended option.
