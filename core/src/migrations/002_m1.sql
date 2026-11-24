-- v2: the M1 model plus the op-log (plan/04 Data Model, plan/05 P2P Sync).
--
-- Sync-tolerant on purpose: no foreign keys, CHECKs or cross-row UNIQUE
-- constraints. Ops from other devices arrive in any order (a task can name a
-- list that hasn't arrived yet), so the core validates in Rust instead and an
-- invalid op is logged but not applied. Runs inside one transaction.

CREATE TABLE meta (
    key    TEXT PRIMARY KEY,
    value  TEXT NOT NULL
);

-- Every change, local or remote. `hlc` is unique: it embeds the device id.
CREATE TABLE op (
    hlc        TEXT PRIMARY KEY,
    entity     TEXT NOT NULL,
    entity_id  TEXT NOT NULL,
    field      TEXT NOT NULL,
    value      TEXT NOT NULL          -- JSON
);
CREATE INDEX op_entity_idx ON op(entity, entity_id);

-- The HLC of the op that last set each field (last writer wins).
CREATE TABLE field_clock (
    entity     TEXT NOT NULL,
    entity_id  TEXT NOT NULL,
    field      TEXT NOT NULL,
    hlc        TEXT NOT NULL,
    PRIMARY KEY (entity, entity_id, field)
) WITHOUT ROWID;

CREATE TABLE task_v2 (
    id             TEXT PRIMARY KEY,
    title          TEXT NOT NULL DEFAULT '',
    notes          TEXT NOT NULL DEFAULT '',
    list_id        TEXT,
    parent_id      TEXT,
    priority       INTEGER NOT NULL DEFAULT 0,
    start_at       INTEGER,               -- epoch ms; for all-day: UTC midnight of the date
    due_at         INTEGER,
    all_day        INTEGER NOT NULL DEFAULT 0,
    tz             TEXT,                  -- IANA zone of timed tasks; NULL = floating
    duration_min   INTEGER,
    repeat_rule    TEXT,                  -- RRULE body, e.g. FREQ=WEEKLY;BYDAY=MO
    repeat_mode    TEXT NOT NULL DEFAULT 'fixed',   -- fixed | after_completion
    status         TEXT NOT NULL DEFAULT 'open',    -- open | done | deleted
    completed_at   INTEGER,
    deleted_at     INTEGER,
    sort_key       TEXT NOT NULL DEFAULT '',
    gcal_event_id  TEXT,
    created_at     INTEGER NOT NULL DEFAULT 0,
    updated_at     INTEGER NOT NULL DEFAULT 0
);
INSERT INTO task_v2 (id, title, notes, list_id, parent_id, priority, start_at, due_at,
                     status, completed_at, sort_key, created_at, updated_at)
    SELECT id, title, notes, list_id, parent_id, priority, start_at, due_at,
           status, completed_at, sort_key, created_at, updated_at
    FROM task;
DROP TABLE task;
ALTER TABLE task_v2 RENAME TO task;
CREATE INDEX task_status_idx ON task(status);
CREATE INDEX task_list_idx   ON task(list_id);
CREATE INDEX task_parent_idx ON task(parent_id);
CREATE INDEX task_due_idx    ON task(due_at);

CREATE TABLE list_v2 (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL DEFAULT '',
    color       TEXT,
    icon        TEXT,
    sort_key    TEXT NOT NULL DEFAULT '',
    archived    INTEGER NOT NULL DEFAULT 0,
    deleted     INTEGER NOT NULL DEFAULT 0,
    created_at  INTEGER NOT NULL DEFAULT 0
);
INSERT INTO list_v2 (id, name, color, icon, sort_key, archived)
    SELECT id, name, color, icon, sort_key, archived FROM list;
DROP TABLE list;
ALTER TABLE list_v2 RENAME TO list;

CREATE TABLE tag_v2 (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL DEFAULT '',
    parent_id   TEXT,
    color       TEXT,
    icon        TEXT,
    sort_key    TEXT NOT NULL DEFAULT '',
    deleted     INTEGER NOT NULL DEFAULT 0,
    created_at  INTEGER NOT NULL DEFAULT 0
);
INSERT INTO tag_v2 (id, name, parent_id, color, icon, sort_key)
    SELECT id, name, parent_id, color, icon, sort_key FROM tag;
DROP TABLE tag;
ALTER TABLE tag_v2 RENAME TO tag;
CREATE INDEX tag_parent_idx ON tag(parent_id);

CREATE TABLE task_tag_v2 (
    task_id  TEXT NOT NULL,
    tag_id   TEXT NOT NULL,
    PRIMARY KEY (task_id, tag_id)
) WITHOUT ROWID;
INSERT INTO task_tag_v2 SELECT task_id, tag_id FROM task_tag;
DROP TABLE task_tag;
ALTER TABLE task_tag_v2 RENAME TO task_tag;
CREATE INDEX task_tag_tag_idx ON task_tag(tag_id);

CREATE TABLE checklist_item (
    id        TEXT PRIMARY KEY,
    task_id   TEXT NOT NULL DEFAULT '',
    title     TEXT NOT NULL DEFAULT '',
    done      INTEGER NOT NULL DEFAULT 0,
    sort_key  TEXT NOT NULL DEFAULT '',
    deleted   INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX checklist_task_idx ON checklist_item(task_id);

CREATE TABLE reminder (
    id          TEXT PRIMARY KEY,
    task_id     TEXT NOT NULL DEFAULT '',
    kind        TEXT NOT NULL DEFAULT 'relative',  -- relative | absolute
    offset_min  INTEGER,                           -- relative: minutes before the anchor
    at          INTEGER,                           -- absolute: epoch ms
    deleted     INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX reminder_task_idx ON reminder(task_id);

-- One row per completion of a task, including each occurrence of a repeating one.
CREATE TABLE completion (
    id             TEXT PRIMARY KEY,
    task_id        TEXT NOT NULL DEFAULT '',
    completed_at   INTEGER NOT NULL DEFAULT 0,
    occurrence_at  INTEGER                       -- the due (or start) that was completed
);
CREATE INDEX completion_task_idx ON completion(task_id);

CREATE TABLE filter (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL DEFAULT '',
    icon            TEXT,
    rule            TEXT NOT NULL DEFAULT '{"all":[]}',   -- filter AST as JSON
    sort            TEXT NOT NULL DEFAULT '[]',           -- JSON array of sort keys
    grp             TEXT,
    show_completed  INTEGER NOT NULL DEFAULT 0,
    sort_key        TEXT NOT NULL DEFAULT '',
    deleted         INTEGER NOT NULL DEFAULT 0
);

-- Full-text search for the `text` filter. External content: rows live in `task`.
CREATE VIRTUAL TABLE task_fts USING fts5(title, notes, content='task', content_rowid='rowid');
CREATE TRIGGER task_fts_ai AFTER INSERT ON task BEGIN
    INSERT INTO task_fts(rowid, title, notes) VALUES (new.rowid, new.title, new.notes);
END;
CREATE TRIGGER task_fts_ad AFTER DELETE ON task BEGIN
    INSERT INTO task_fts(task_fts, rowid, title, notes) VALUES ('delete', old.rowid, old.title, old.notes);
END;
CREATE TRIGGER task_fts_au AFTER UPDATE OF title, notes ON task BEGIN
    INSERT INTO task_fts(task_fts, rowid, title, notes) VALUES ('delete', old.rowid, old.title, old.notes);
    INSERT INTO task_fts(rowid, title, notes) VALUES (new.rowid, new.title, new.notes);
END;
INSERT INTO task_fts(task_fts) VALUES ('rebuild');
