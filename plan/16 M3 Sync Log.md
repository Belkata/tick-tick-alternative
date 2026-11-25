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

## Slice 2: sync in the daemon ✅ (2026-11-25, on the Arch laptop)
- **tt-api:**
  - `SyncPair`, `SyncJoin{ticket}`, `SyncNow` and `SyncStatus` requests, answered by the daemon through `server::SyncControl` (tt-api stays free of iroh). Without one they answer "this tt-daemon doesn't run sync".
  - `peers` and `remove_peer` are in the table.
  - `Db::subscribe_writes()` fires only for client writes, so auto-sync ignores its own writes, which arrive as outside commits. An idle 25 s check showed no sync loop.
- **Daemon** (`daemon/src/sync.rs`):
  - Starts the sync node (relays unless `--no-relay`; `--no-sync` turns it off).
  - Auto-sync 5 s after startup, every 90 s, and 3 s after local edits, with no overlapping rounds.
  - Status: last round, last error, peers. Peers' incoming syncs count too.
  - Tray: the tooltip has a sync line and the menu shows the status plus a working **Sync now**.
- **CLI:** while the daemon serves this DB, `tt sync pair | join | now | peers | forget` go through it (only one process uses the key). `serve` and the spike `pull` are refused.
- **App:** desktop no longer starts the Spike 2 `SyncPeer`, which would share `tt.key` with the daemon. The sidebar's "More" section has **Devices**:
  - this device's id and its last sync;
  - Pair (shows the ticket, Copy) and Join (paste a ticket);
  - Sync now, and the paired devices with Forget.
  - Without a daemon it says how to start one. Android keeps the spike tab until M4.
- **Tests:** tt-api 5 socket tests (sync requests need a node; peers over the socket) and app integration **10/10** (Devices without a daemon).
- **Live run:** two throwaway daemons as temporary systemd units, relays off. Paired through `tt sync pair` and `join` via the daemons; a task added on one reached the other through **auto-sync alone** in under 7 s; `peers` and `sync now` worked.
- **The user's service** runs it (node `c382b28c2f…`, no peers yet). The key `~/.local/share/tick-alt/tt.key` came from the earlier spike app run.

## Next
- **Check with the user:** the Devices page and the tray status line in the real app, and a real second device (the Mac, or the phone after M4).
- M3 still open:
  - op-log compaction and snapshots;
  - trust sharing (plan/05 step 4) and removal spreading to all devices;
  - a network simulation test.
- Not done yet: QR pairing (Android, M4).
