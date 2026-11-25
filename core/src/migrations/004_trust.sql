-- M3: forgotten devices (plan/05 "Removing a device deletes it from trusted peers on all
-- devices at the next sync"). Local like `peer`; shared with paired devices during sync.
CREATE TABLE peer_removed (
    node_id    TEXT PRIMARY KEY,
    removed_at INTEGER NOT NULL      -- epoch ms
);
