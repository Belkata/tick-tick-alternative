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

## Round 2: detail pane priority and phone tap targets

- The four small bordered priority chips are now one segmented control across the pane (flag icon
  and name, the picked one fills in its priority colour). Keys `priority-0..3` are unchanged.
- On touch platforms the task checkbox keeps its look but its tap target is 48 x 48 and rows are at
  least 48 high; the check is centred on the row there (`PriorityCheck.hit`).

Before / after: `attachments/ui2-detail-before.jpg`, `attachments/ui2-detail-after.jpg`.

## Round 3: narrow detail pane and quick add hints

- In the Calendar the detail pane is about 250 px wide, and the Repeat dropdown (as wide as its
  longest rule) and the Calendar switch row overflowed (debug overflow stripe seen in the
  screenshot). The dropdown is now capped to the room there is, the switch is scaled in layout
  (not just painted), and tag chips ellipsize their label.
- Quick add on desktop shows an `N` key hint while idle (the shortcut already existed) and
  "Enter to add" while there is text. Not shown on touch.

Before / after of the narrow pane: `attachments/ui3-narrow-detail-before.jpg`,
`attachments/ui3-narrow-detail-after.jpg`; quick add: `ui3-quickadd-idle.jpg`, `ui3-quickadd-typing.jpg`.

## Round 4: light mode contrast

Checked every screen in light mode (`UTRE_THEME=light` forces a theme for screenshots and QA; the
default is still the system's). The light mute grey (#7A8197, 3.6:1 on the page) and the teal / red
used as small text (3.7-4.0:1) were below WCAG AA for 11-12 px text. Computed ratios and moved to:
mute #646B80 (5.1:1), teal text #0B7A6D (4.9:1), late text #C93540 (4.9:1). The accent fill, priority
colours and dark theme (all above 5.6:1 for text) are unchanged. Empty states also checked in light.

Before / after: `attachments/ui4-light-before.jpg`, `attachments/ui4-light-after.jpg`, `attachments/ui4-empty-light.jpg`.

## Round 5: the phone, seen for the first time

Method: a release x86_64 build on the `tt_pixel` emulator (headless), a seeded DB pushed with
`adb root` + `adb push` into `/data/data/dev.utre.utre/files/` (made with the `utre` CLI on a private
`$UTRE_DB`), screenshots by coordinates, light and dark (`adb shell cmd uimode night yes|no`).
List, drawer, quick add with the keyboard and the calendar views were already good. Fixed:
- Detail: the last section ("Add a reminder") sat under the gesture bar. Scroll lists on the detail,
  task list, Devices and Google pages now add the bottom system inset.
- Detail rows were 43 dp and the tag x and `+ tag` about 20 dp high: rows are 48 dp and the tag chips
  about 30 dp on touch (`isTouch` in `theme.dart`).
- Going back from a task left it tinted as selected on the single-pane list; selection is cleared on return.
- Calendar: a 45 minute block clipped its time line (short threshold 34 -> 42 dp); Month weekday headers
  wrapped ("Wednesd/ay") and are now three letters on narrow screens.
- Google page used 20 dp padding and Devices 24/18: both 24/18, so the titles line up.
- Sidebar: an empty Filters section was just a gap; it now says "Save a search with +".

Seen and left: in Week on a phone the seven columns leave 2-4 letters per title (kalender's layout; the
real answer is probably a 3-day view, a decision for the user), the month FAB covers the last row's
right cell when scrolled to the end, Devices only shows "Starting sync..." on the emulator (no network).

Before / after: `attachments/ui5-phone-detail-before.jpg`, `ui5-phone-detail-after-dark.jpg`,
`ui5-phone-month-before.jpg`, `ui5-phone-month-after.jpg`; also `ui5-phone-week.jpg`, `ui5-phone-list-dark.jpg`.
