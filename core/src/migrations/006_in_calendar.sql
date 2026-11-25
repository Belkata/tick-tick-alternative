-- "Show in Calendar" (2026-11-25): a task can stay off the calendar grid and the
-- Unscheduled panel. On by default, so existing tasks show as before.
ALTER TABLE task ADD COLUMN in_calendar INTEGER NOT NULL DEFAULT 1;
