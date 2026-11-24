-- tt-core schema v1 (M0 spike subset: lists, tags, tasks).
-- Timestamps are Unix epoch milliseconds (UTC).
-- Sync columns (HLC per field, op-log) come in M1; see plan/05 P2P Sync.md.

CREATE TABLE IF NOT EXISTS list (
    id         TEXT PRIMARY KEY,
    name       TEXT NOT NULL,
    color      TEXT,
    icon       TEXT,
    sort_key   TEXT NOT NULL DEFAULT '',
    archived   INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS tag (
    id         TEXT PRIMARY KEY,
    name       TEXT NOT NULL,
    parent_id  TEXT REFERENCES tag(id),
    color      TEXT,
    icon       TEXT,
    sort_key   TEXT NOT NULL DEFAULT '',
    UNIQUE (parent_id, name)
);

CREATE TABLE IF NOT EXISTS task (
    id            TEXT PRIMARY KEY,
    title         TEXT NOT NULL,
    notes         TEXT NOT NULL DEFAULT '',
    list_id       TEXT REFERENCES list(id),
    parent_id     TEXT REFERENCES task(id),
    priority      INTEGER NOT NULL DEFAULT 0 CHECK (priority BETWEEN 0 AND 3),
    start_at      INTEGER,
    due_at        INTEGER,
    status        TEXT NOT NULL DEFAULT 'open'
                  CHECK (status IN ('open', 'done', 'deleted')),
    completed_at  INTEGER,
    sort_key      TEXT NOT NULL DEFAULT '',
    created_at    INTEGER NOT NULL,
    updated_at    INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS task_tag (
    task_id  TEXT NOT NULL REFERENCES task(id),
    tag_id   TEXT NOT NULL REFERENCES tag(id),
    PRIMARY KEY (task_id, tag_id)
);

CREATE INDEX IF NOT EXISTS task_status_idx ON task(status);
CREATE INDEX IF NOT EXISTS task_list_idx   ON task(list_id);
