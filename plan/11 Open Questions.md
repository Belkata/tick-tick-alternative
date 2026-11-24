---
tags: [project/tick-alt, plan, questions]
---

# 11 Open Questions

Back to [[00 Overview]]. Answer these inline, and I'll fold the answers into the plan.

## Blocking (needed before M0 ends)
1. ~~**Stack**~~ ✅ Decided: Option A, Rust core + Flutter + Kotlin Glance widget (fallback: Option B if the M0 spikes fail). See [[02 Tech Stack Options]].
2. ~~**UI style**~~ ✅ Decided: Soft with Material tag chips. See [[09 UI Style Options]].

## Non-blocking (decide before the relevant milestone)
3. ~~**Today and Next 7 Days presets.**~~ ✅ Decided (2026-11-24): **start with zero filters.** You build your own; the TickTick import recreates your 4.
4. ~~**Notes format.**~~ ✅ Decided (2026-11-24): **plain text in v1**, Markdown rendering later.
5. ~~**Subtask depth.**~~ ✅ Decided (2026-11-24): **max 3 levels.** Enforced in the core (`MAX_DEPTH`).
6. ~~**Calendar push scope.**~~ ✅ Decided (2026-11-24): **push all-day tasks too; completed tasks stay on Google, marked with a ✓.**
7. ~~**Work list kanban columns.**~~ ✅ Decided (2026-11-24): **drop them** on import.
8. ~~**Duplicate reminders.**~~ ✅ Decided (2026-11-24): **every device rings.** No "silent desktop" mode.
9. ~~**"Won't Do".**~~ ✅ Decided (2026-11-24): **complete and trash are enough.** Three statuses only: open, done, deleted. TickTick "won't do" imports as done.
10. ~~**DB encryption at rest.**~~ ✅ Decided (2026-11-24): **OS disk encryption is enough** (FileVault, LUKS, Android FBE). No app-level DB encryption in v1.
11. ~~**Inbox rule edge case.**~~ ✅ Decided (2026-11-24): **yes, a priority-only task stays in Inbox.** Priority doesn't triage a task.
12. ~~**Repo hosting.**~~ ✅ Decided (2026-11-24): **GitHub, public:** https://github.com/Belkata/tick-tick-alternative. Commits are authored as `belkata.okfo@gmail.com` (set in this repo's git config).
