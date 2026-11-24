---
tags: [project/tick-alt, plan, questions]
---

# 11 Open Questions

Back to [[00 Overview]]. Answer these inline, and I'll fold the answers into the plan.

## Blocking (needed before M0 ends)
1. ~~**Stack**~~ ✅ Decided: Option A, Rust core + Flutter + Kotlin Glance widget (fallback: Option B if the M0 spikes fail). See [[02 Tech Stack Options]].
2. ~~**UI style**~~ ✅ Decided: Soft with Material tag chips. See [[09 UI Style Options]].

## Non-blocking (decide before the relevant milestone)
3. **Today and Next 7 Days presets.** You didn't want them as built-in smart lists.
   - Should the app ship them as **editable preset filters** that you can delete?
   - Or start with zero filters?
4. **Notes format.** Markdown wasn't a must-have. Is plain text OK for v1, with Markdown rendering later?
5. **Subtask depth.** Is a max of 3 levels enough?
6. **Calendar push scope.** Should **all-day** tasks be pushed to Google too, or only timed ones? Should **completed** tasks stay on Google with a ✓ or be removed?
7. **Work list kanban columns.** On import, drop them or turn them into tags (`work/todo`, `work/doing`…)?
8. **Duplicate reminders.** Every device rings for a reminder. Is that OK, or should only the phone ring (desktop set to "silent" mode)?
9. **"Won't Do".** Is it needed as its own status, or is *complete* / *trash* enough?
10. **DB encryption at rest.** Is relying on OS disk encryption enough?
11. **Inbox rule edge case.** A task with **only a priority**, and no tag, list or date, stays in Inbox. Is that right?
12. **Repo hosting.** GitHub (for CI and Obtainium APK updates) or something self-hosted on the homelab?
