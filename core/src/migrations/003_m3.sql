-- M3: devices this one syncs with (plan/05 "Pairing"). Local to each device: not an
-- op-logged table, never synced, and not part of state_dump.
CREATE TABLE peer (
    node_id    TEXT PRIMARY KEY,     -- the iroh endpoint id (Ed25519 public key)
    name       TEXT NOT NULL,        -- what the device calls itself
    added_at   INTEGER NOT NULL,     -- epoch ms
    last_seen  INTEGER,              -- epoch ms of the last successful sync
    last_addr  TEXT                  -- its EndpointAddr JSON, for dialing it again
);
