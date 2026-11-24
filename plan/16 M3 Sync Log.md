---
tags: [project/tick-alt, plan, m3]
created: 2026-11-24
---

# 16 M3 Sync Log

Back to [[00 Overview]] · [[10 Roadmap]] · [[05 P2P Sync]].

## Slice 1: protocol, pairing and CLI ✅ (2026-11-24, on the Arch laptop, done while the user was away)
- **Core** (`core/src/replica.rs`, schema v3):
  - `version_vector()` (the newest HLC per device, one `GROUP BY` on the HLC's device suffix) and `ops_missing(their_vv)`.
  - A local `peer` table (node id, name, added and last seen, last address). It isn't op-logged, synced or part of `state_dump`.
- **Protocol** (`sync/src/replicate.rs`, ALPN `tickalt/sync/1`, JSON-line frames):
  - Hello (proto, name, address, vector as a list of HLCs) → ops both ways in batches of 500, sent and received at the same time → Done → Ack.
  - An untrusted Hello gets **Denied**. Pairing: a `tt2…` ticket (address, a 16-byte one-time secret, name) is valid for 5 minutes and works once. Pair → Paired, both sides trust each other, then they sync.
  - The database is opened per call (`FileReplica`), so a running daemon notices sync writes through `data_version`.
- **Node:** prefers UDP port **41824** (`$TT_SYNC_PORT`; random if taken), so LAN peers find each other again after restarts without relays. The Spike 2 ALPN is still served.
- **CLI:** `tt sync pair | join <ticket> | now | peers | forget <device> | serve`. `tt sync pull` is the Spike 2 pull.
- **Tests:** 7 in tt-sync over loopback with relays off:
  - pairing then sync converges, and later edits on both sides converge either way;
  - a quiet sync moves nothing;
  - unpaired devices are refused, and a ticket works once;
  - a 2,800-op sync streams in batches both ways.
  - Plus 2 core tests. The core's convergence property tests still pass.
- **Live CLI run** (two scratch DBs): pair and join, then `sync now` after the server restarted. A conflicting rename resolved to the later write on both sides.

## Next (slice 2)
- `tt-daemon` runs the sync node: it serves paired peers, syncs every 1–2 minutes and shortly after local changes, shows sync status in the tray, and enables "Sync now".
- `tt sync` and the app's Devices page go through the daemon (new socket requests), so only one process uses the device key.
- ⚠️ The desktop app's spike Sync tab and the daemon would share `tt.key`: stop starting the spike `SyncPeer` on desktop.
- Not done yet: QR pairing (Android, M4), trust sharing between devices, op-log compaction and snapshots, a network simulation test.
