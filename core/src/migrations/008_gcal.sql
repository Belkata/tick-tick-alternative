-- Google Calendar cache (plan/06, M5b). Per device, never synced over P2P: each device
-- fetches Google's events itself. Rows here are plain tables, not ops.
CREATE TABLE gcal_calendar (
    id          TEXT PRIMARY KEY,
    summary     TEXT NOT NULL,
    color       TEXT,
    selected    INTEGER NOT NULL DEFAULT 0,
    -- Google's incremental `syncToken`; NULL means "do a full sync".
    sync_token  TEXT
);
CREATE TABLE gcal_event (
    calendar_id TEXT NOT NULL REFERENCES gcal_calendar(id) ON DELETE CASCADE,
    id          TEXT NOT NULL,
    summary     TEXT NOT NULL,
    -- Epoch ms; all-day events hold floating UTC midnights (end exclusive), like tasks.
    start_at    INTEGER NOT NULL,
    end_at      INTEGER NOT NULL,
    all_day     INTEGER NOT NULL DEFAULT 0,
    html_link   TEXT,
    color       TEXT,
    PRIMARY KEY (calendar_id, id)
);
CREATE INDEX gcal_event_time ON gcal_event (start_at, end_at);
