# 20 UI Iteration Log

Overnight UI iteration (2026-11-26), Soft style kept (plan/09). One coherent change per round, each a PR.
Screenshots are taken from the real app on a test daemon (private DB), not mockups, when the
change is small enough to try directly.

## Research notes

Sources read (web search, 2026-11-26):
- Blake Crosley, "Design study: Things" and MacStories' Things 3 review: colour is reserved for
  information (priority, overdue), hierarchy is quiet (area, project, heading, to-do), headings
  inside lists make long lists scannable.
- Todoist, "Your new task view": the redesign was about using space well while still showing
  the facts that matter (date, project, labels) under the title.
- Omni Group forum thread on OmniFocus 2 density: two rows per task with wasted space is
  a common complaint, so a second line must carry information and only appear when there is some.
- Material 2 "design states" and Compose M3 expressive list items: selection and hover are
  separate states and both should show when they occur together.
- Not found: authoritative current M3 Expressive selection spec (only API docs), nothing on empty
  states; those decisions are ours.

Patterns borrowed: facts line under the title (Todoist), date headings (Todoist Today/Upcoming,
Things sections), colour only for urgency (overdue red, today teal), tinted selection with an
edge marker, hover wash distinct from selection.

## Round 0: widget row tap opens the task (PR 9)

`MainActivity`'s pending / pushed task id now reaches `DeskShell`: it selects the task and pushes
the detail on narrow windows. One channel handler (`WidgetBridge.listen`) serves shared text and
widget taps. Integration test in `inbox_test.dart`.

## Round 1: the task list

Problems seen in the running app:
- The facts (tags, date) started at the middle of the row: the title (`Expanded`) and the facts
  (`Flexible`) split the width 50/50, so long titles were cut off with plenty of room left.
- A selected row was a full 2 px teal outline, shouting louder than the priorities.
- A due-sorted list was one flat run: no sense of what is late, today or later.
- Empty views were one grey line.

Changes:
- Card = title (up to two lines) with a quiet facts line under it: due date first (calendar icon,
  coloured only when overdue or today), then checklist, subtasks, list, tag chips.
- Selection = teal wash plus a 3 px bar on the left edge; hover = faint wash.
- Due-date sort groups rows under Overdue / Today / Tomorrow / Next 7 days / Later / No date
  (`dueGroupOf`, `groupByDue` in `task_sort.dart`, unit tested). Other sorts and manual order stay flat.
- Empty states: a soft icon disc, the old headline and a one-line hint per view.

Before / after (Work list, dark): `attachments/ui1-list-before.jpg`, `attachments/ui1-list-after.jpg`.
