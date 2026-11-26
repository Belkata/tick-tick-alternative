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

## Round 6: wide layouts, dialogs, sidebar state

Method: the emulator again, resized to a tablet (`adb shell wm size 1920x1200; wm density 240`, undone at the end), because
the Wayland desktop can't be clicked from a script here (Hyprland's `sendshortcut` does not deliver mouse buttons) and the
wide shell, calendar and dialogs are the same widgets. Touch input, so hover states were not checked.
- Edge-to-edge: on a wide Android window the status bar covered the sidebar title ("Utre") and the calendar header.
  The wide shell is in a `SafeArea` (zero insets on desktop).
- Two sidebar rows looked selected at once when a page (Calendar, Devices, Google) was open: the task view's tile stayed
  lit. `DeskSidebar.viewSelected` is false while a page is open.
- Dialogs and menus used Material 3's grey-green tinted surface, not the Soft panel. `dialogTheme`, `popupMenuTheme` and
  `canvasColor` now use the panel colour with no tint and 18 / 12 px corners. The filter builder's rule group, which was
  invisible against the tint, now reads as a bordered box.
- Calendar: a 45 minute block in a narrow Week column showed "Review pu... 15:00" on one line after round 5's threshold
  change; back to two lines (threshold 34 dp) with 2 dp top padding below 40 dp so the time is not clipped.
Checked and fine: Week and Month on a wide window, the sort menu, the tag edit dialog, the detail pane inside the calendar,
dark mode of the filter dialog and sidebar.

Before / after: `attachments/ui6-wide-inset-before.jpg` / `ui6-wide-inset-after.jpg`, `ui6-filter-dialog-before.jpg` /
`ui6-filter-dialog-after.jpg`, `ui6-sidebar-two-selected-before.jpg` / `ui6-sidebar-after-dark.jpg`, `ui6-calendar-week-wide.jpg`.

Process note: a first desktop test launch without `$UTRE_DB` made the app open the real database directly (the
daemon-or-file check compares paths), so for about 15 seconds it was open on the real file with no input sent. Always
export `UTRE_DB` for the app as well as the daemon when testing on a desktop.

## Round 7: motion

One helper, `motion(context, ms)` in `theme.dart`, returns a zero duration when the system asks for reduced motion
(`MediaQuery.disableAnimations`), so every transition below respects it.
- Checkbox: the fill and edge colours animate (140 ms) and the tick pops in (`AnimatedScale`, ease-out-back).
- Task card: the selection bar grows from the left edge instead of appearing; the wash already animated through
  `Material.animationDuration`, now 160 ms and reduced-motion aware. Sidebar tiles likewise.
- Segmented priority and the calendar's Unscheduled drop highlight use the same helper.
Checked on the emulator: completing a task from the phone list leaves no stray tick at scale 0. Not verified: the look of the
animations themselves (stills only), and that `disableAnimations` flips with Android's "remove animations" (Flutter reads it).
Left: page routes (detail on phones) keep Material's default transition.

## Round 8: date picker, Flutter review, focus ring, goldens, icon (2026-11-26, v0.8.0)
- **Date and time picker** (`app/lib/desk/date_picker.dart`) for Due and Start: month grid (Monday first), Today / Tomorrow / Next week (the coming Monday) / No date, All day or Time with typed times, keyboard (PageUp/PageDown moves the selected day). A dialog on desktop, a bottom sheet on the phone. Start after due is blocked with an inline message; switching all-day and timed warns that the other date is cleared (the core needs both the same kind). `TaskDetail.start_at` (epoch ms, serde default for old daemons) feeds it; `row.all_day` is the single flag. The daemon needs the rebuild.
- **Flutter review:** No date as a title-row button; dark-theme contrast through `onPrimary`; 2x text wraps; `DeskController` ignores late notifications after dispose; reload at midnight; narrow detail rows drop their icons; tooltips and semantics.
- **Keyboard focus:** `FocusRing` and `SoftColors.ring`; Tab order grouped per pane.
- **Golden tests:** 26, Linux-only, in `app/test/golden/` (procedure in its README), a `goldens` CI job, and a `Flutter (integration, Linux)` job under xvfb (not required). They exposed calendar bugs, fixed in the same release: all-day tasks missing in Day and Month, the "+N" count, narrow overlap columns, "1 days", a green-tinted dark picker.
- **App icon "u-tick":** a teal u whose right arm is a tick, an amber sun in the bowl. Masters in `packaging/icon/`, `scripts/render-icons.sh` makes the Android adaptive and monochrome icons, the status-bar glyph `ic_stat_utre`, the Linux SVG and the macOS appiconset. Tray icons still use the old art.

## Round 9: tray icon and golden-found fixes (2026-11-26, v0.8.1)
- **Tray icons use the u-tick mark** (PR 33): `daemon/src/tray_mark.rs` rasterises it in code from the monochrome SVG path. Linux (SNI): teal rounded square with a white u-check, an amber dot while the Inbox has tasks, grey when empty. macOS: a template image. **Not seen in waybar, and the macOS build is unbuilt and unverified.**
- **Calendar and sheets** (PR 34), found by the goldens:
  - All-day tasks missing in Day and Month came from a zero-length fixture, not a time-zone bug (the core sends full local days, `core/src/calendar.rs::block`). `TaskEvent` now stretches empty all-day blocks as a guard. Month "+N" is correct.
  - Tiles under 60 px are one compact line. A shared `agoText` (`app/lib/desk/ago.dart`) fixes "1 days ago".
  - `bottomSheetTheme` removes the green surface tint from every bottom sheet. The phone view picker has an `arrow_drop_down` icon. Goldens regenerated.
