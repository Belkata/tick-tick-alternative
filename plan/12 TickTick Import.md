---
tags: [project/utre, plan, import]
---

# 12 TickTick Import

Back to [[00 Overview]].

**Goal:** a one-time migration of the current TickTick data, about 430 Inbox tasks plus the Work and No Calendar lists and around 24 tags.

## Source: TickTick backup CSV (preferred)
- Get it from TickTick web: **Settings › Account › Generate backup**, which downloads a `.csv`.
- It contains:
  - list name;
  - title and content;
  - checklist items (inside content);
  - tags;
  - start and due dates, all-day flag and time zone;
  - repeat rule (RRULE);
  - reminders;
  - priority;
  - status and completion time;
  - parent id for subtasks.
- Confirm the exact columns on a real export first, during M6.

**Alternative:** the TickTick Open API (OAuth, developer.ticktick.com) can be used if the CSV turns out to be missing fields, such as tag colours.

## Mapping

| TickTick | Utre |
|---|---|
| Inbox | `list_id = NULL` |
| List (Work, No Calendar) | List. Emoji prefixes in names become the list icon. |
| Folder | Ignored (none in use) |
| Tag (with parent) | Tag with `parent_id`. Colours come from the API if the CSV lacks them. |
| Priority 0/1/3/5 | 0/1/2/3 |
| Checklist content | Checklist items |
| Subtask (parentId) | `parent_id` |
| RRULE + "repeat from completion" | RepeatRule `fixed` or `after_completion` |
| Reminders (TRIGGER:-PT10M…) | Relative reminders |
| Completed / Won't Do | `done` (Won't Do is imported as done with a `wont-do` tag) |
| Kanban columns (Work) | Dropped. Optionally becomes tags `work/<column>`. See [[11 Open Questions]]. |
| Filters | **Recreated manually** (only 4: Widget, Real Inbox, Unscheduled, Today + Real Inbox) |

## Inbox side-effect

> [!important]
> With the new [[04 Data Model#Inbox rule]], only tasks with **no tag, no list and no date** will show in Inbox. Many of the current 434 "Inbox" tasks have dates or tags, so they'll leave Inbox automatically. That's the triage behaviour you chose, and it will probably shrink the real Inbox to roughly the size of today's *Real Inbox* filter.

## Steps
1. `utre import ticktick backup.csv --dry-run` prints counts and anything it couldn't map.
2. Run it for real. The import becomes normal ops, so it syncs to the other devices.
3. Manually recreate the 4 filters in the builder.
4. Spot-check: counts per tag and list match TickTick.

## Done (2026-11-25): importer and first import
- **Source used:** not the CSV. The TickTick connector gave JSON in TickTick's own field names (open tasks of the Inbox and both lists, the 200 newest completed tasks, 24 tags). Saved as the backup `~/ticktick-backup/ticktick-export-2026-11-25.json` (outside the repo: it's personal data and the repo is public). A snapshot of the Utre database from just before the import sits beside it.
- **Command:** `utre import ticktick <file> [--dry-run]` (`core/src/import.rs`). A dry run imports into a throwaway in-memory database. Re-running is safe: a task with the same title and creation time is skipped.
- **Result:** 453 open + 200 done tasks, 29 subtasks, 32 checklist items, 11 repeats, 167 reminders, 24 tags (Via-Verde under theater), lists No Calendar and Work (emoji became the icon). Kanban columns dropped.
- **Mapping choices:** `repeatFrom` `1` means after completion, anything else fixed. All-day reminders (`TRIGGER:PT9H`) become offset 0, since Utre fires all-day reminders at 09:00. A start equal to the due date is stored as due only. Priorities 1/3/5 become 1/2/3.
- **Known gaps:** the API caps completed tasks at 200, so older history is missing (about half are occurrences of repeating tasks). Images in notes (`![image](…)`) point at TickTick attachments and are not copied. The 4 filters are still to be recreated by hand.
