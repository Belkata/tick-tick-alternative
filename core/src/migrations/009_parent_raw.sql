-- Convergent subtask cycles (2026-11-26): `parent_id` becomes the *effective* parent,
-- derived from `parent_raw`, the last-writer-wins value of the synced `parent_id` field.
-- The two differ only where concurrent edits would make a cycle or nest too deep.
ALTER TABLE task ADD COLUMN parent_raw TEXT;
UPDATE task SET parent_raw = parent_id;
