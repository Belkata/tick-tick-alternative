---
tags: [project/utre, plan, questions]
---

# 11 Open Questions

Back to [[00 Overview]]. Answer these inline, and I'll fold the answers into the plan.

## Blocking (needed before M0 ends)
1. ~~**Stack**~~ ✅ Decided: Option A, Rust core + Flutter + Kotlin Glance widget (fallback: Option B if the M0 spikes fail). See [[02 Tech Stack Options]].
2. ~~**UI style**~~ ✅ Decided: Soft with Material tag chips. See [[09 UI Style Options]].

## Non-blocking (decide before the relevant milestone)
3. ~~**Today and Next 7 Days presets.**~~ ✅ Decided (2026-11-24): **start with zero filters.** You build your own; the TickTick import recreates your 4.
4. ~~**Notes format.**~~ ✅ Decided (2026-11-24): **plain text in v1**, Markdown rendering later. ✅ Rendering added in v0.6.0 (2026-11-25); storage is still plain text.
5. ~~**Subtask depth.**~~ ✅ Decided (2026-11-24): **max 3 levels.** Enforced in the core (`MAX_DEPTH`).
6. ~~**Calendar push scope.**~~ ✅ Decided (2026-11-24): **push all-day tasks too; completed tasks stay on Google, marked with a ✓.**
7. ~~**Work list kanban columns.**~~ ✅ Decided (2026-11-24): **drop them** on import.
8. ~~**Duplicate reminders.**~~ ✅ Decided (2026-11-24): **every device rings.** No "silent desktop" mode.
9. ~~**"Won't Do".**~~ ✅ Decided (2026-11-24): **complete and trash are enough.** Three statuses only: open, done, deleted. TickTick "won't do" imports as done.
10. ~~**DB encryption at rest.**~~ ✅ Decided (2026-11-24): **OS disk encryption is enough** (FileVault, LUKS, Android FBE). No app-level DB encryption in v1.
11. ~~**Inbox rule edge case.**~~ ✅ Decided (2026-11-24): **yes, a priority-only task stays in Inbox.** Priority doesn't triage a task.
12. ~~**Repo hosting.**~~ ✅ Decided (2026-11-24): **GitHub, public:** https://github.com/Belkata/utre. Commits are authored as `belkata.okfo@gmail.com` (set in this repo's git config).
13. ~~**App name.**~~ ✅ Decided (2026-11-25): **Utre** (Bulgarian "утре", tomorrow: the joke is that a planner is where "tomorrow" lives; the tagline idea is "stop planning, do it"). "Utre" was a placeholder. Name check: no GitHub repo named `utre` (only `utreexo`/Utrecht matches), no AUR package, and no Google Play app found. YATA was the runner-up (taken on Play, the App Store and the Microsoft Store). The GitHub repo was renamed to `Belkata/utre` on 2026-11-26. **The app rename is not done yet:** do the display name first (app name, window titles, launcher labels, OAuth consent screen); the internal ids (`dev.utre.*`, socket, DB paths, `utre` CLI) change once, together with the move to a release key, because the Android app id can't change in place.

## Decisions and steps for the user (from the 2026-11-26 reviews)
- **Sync hardening (recommended, not done):** reject far-future HLC wall times; validate trust device ids as 64 hex; add an overall sync-session timeout; close the socket chmod window in the macOS fallback directory.
- **GitHub settings (only you can set these):** protect `main` and require `Rust (test + clippy)`, `Flutter (analyze + test)`, `Shell scripts (shellcheck)` (plus rustfmt and goldens); enable Dependabot alerts and security updates; set Actions workflow permissions to read-only; later, the 4 `ANDROID_KEYSTORE_*` secrets.
