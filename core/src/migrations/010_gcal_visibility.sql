-- Which Google calendars show (per device, like the rest of the Google cache).
-- `selected` stays the effective visibility. `chosen` says the user set it on this device;
-- until then it follows Google's own `selected` flag (`google_selected`), so the primary
-- calendar shows and new calendars appear as they do in Google Calendar.
ALTER TABLE gcal_calendar ADD COLUMN google_selected INTEGER NOT NULL DEFAULT 0;
ALTER TABLE gcal_calendar ADD COLUMN chosen INTEGER NOT NULL DEFAULT 0;
ALTER TABLE gcal_calendar ADD COLUMN is_primary INTEGER NOT NULL DEFAULT 0;
ALTER TABLE gcal_calendar ADD COLUMN access_role TEXT;
-- Calendars switched on before this change were a choice. Switched-off ones were the old
-- default, so they follow Google from the next fetch on.
UPDATE gcal_calendar SET chosen = 1 WHERE selected = 1;
