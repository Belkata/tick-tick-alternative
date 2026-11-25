-- M3 op-log compaction (plan/05): the newest HLC seen from each device. Compaction drops
-- superseded ops, which may include a device's newest one; the version vector reads this
-- table too, so it never goes backwards (and peers don't resend what was compacted away).
CREATE TABLE device_clock (
    device  TEXT PRIMARY KEY,       -- 16 hex chars, as in HLC text
    max_hlc TEXT NOT NULL
) WITHOUT ROWID;
