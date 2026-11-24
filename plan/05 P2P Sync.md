---
tags: [project/tick-alt, plan, sync]
---

# 05 P2P Sync

Back to [[00 Overview]].

**Decision:** pure peer-to-peer with no always-on node. Devices sync whenever any two of them are online at the same time.

## Transport: iroh 1.x
- Each device has an **iroh endpoint** with a persistent Ed25519 key. Its NodeId *is* the device's identity.
- **LAN:** local discovery finds peers on the same network with no internet needed.
- **Internet:** QUIC NAT traversal (hole punching).
  - If a direct path fails, traffic goes through a **relay**. That's n0's public relays by default, or a self-hosted `iroh-relay` later.
  - Relays only forward **end-to-end encrypted** QUIC packets. They store nothing and can't read anything, so this still counts as "no server of my own".
- **No iroh-docs or iroh-gossip.** A small custom protocol runs over one QUIC bidirectional stream. It's easier to reason about, and iroh-docs isn't 1.0-stable.

## Pairing (one time per device pair)
1. On device A: **Settings › Devices › Add device** shows a QR code with A's NodeId, a direct-address hint and a one-time secret (valid 5 minutes).
2. Device B scans it with the phone camera, or pastes it as text on desktop. B connects to A and proves it has the secret.
3. Both store each other in `trusted_peers` (NodeId plus name). Every later connection checks the NodeId against this list.
4. Trust is shared: A sends B its list of trusted peers, so a third device paired to either one learns about the others.
5. **Removing a device** deletes it from trusted peers on all devices at the next sync.

## Data model for sync: an op-log with per-field LWW

I chose this over a full CRDT library (Loro or Automerge) because:
- the data is relational (tasks, tags, lists), not collaborative text;
- with one user and 3 devices, conflicts are rare and *last writer wins per field* is what a human expects;
- it's easy to inspect and debug in SQLite.

How it works:
- Every change is an **op**: `{op_id, hlc, device_id, entity, entity_id, field, value}`.
  - Example: `task:123 / title = "Buy milk"`.
  - Set membership (task tags) uses add/remove ops with HLC tie-breaking, the observed-remove pattern.
- The **HLC** (hybrid logical clock) is a physical time plus a counter plus the device id. That gives a total order even when clocks are skewed.
- **Applying an op:**
  - If `op.hlc > field_hlc[entity_id, field]`, set the value and update the stored HLC.
  - Otherwise, keep the op in the log but leave the state alone.
  - Applying is idempotent and order-independent, which makes the result convergent.
- **Deletes** are a `status = deleted` op, which is a tombstone. A purge op removes the row data after N days. Tombstone ids are kept so a delete can't come back.
- **Ordering** uses fractional-index `sort_key` strings. Two devices reordering concurrently both get a valid order, with ties broken by id.
- **Subtask cycles** (A under B and B under A at the same time) are detected at apply time. The later op is rejected locally and becomes a no-op on every device.

## Sync protocol (per connection)
1. **Hello:** exchange protocol version, device id and a **version vector** (`device_id → max HLC seen`).
2. Each side streams the ops the other is missing, in batches. These are the ops whose HLC is above the peer's entry for that op's device.
3. Ops are applied in a single transaction per batch. Then UI, widget and reminder refreshes are triggered.
4. Each side acks with its updated vector.

**Op-log compaction:** once every trusted peer has acknowledged everything up to some HLC per device, the ops below it can be dropped. The materialised state plus per-field HLCs is enough. If a device has been away for longer than the compaction window, it gets a **full snapshot**.

## When does syncing happen?

| Trigger | Desktop (tray daemon) | Android |
|---|---|---|
| Local change | Push to connected peers immediately | Expedited WorkManager job, plus immediate push while the app is open |
| App foreground | Always running | On resume, connect to known peers |
| Peer appears on LAN | Local discovery event, then connect | Same, while the app is open or a job is running |
| Periodic | Every 1–2 minutes, try known peers | WorkManager periodic job (15-minute minimum, may be deferred by Doze) |
| Manual | Tray › Sync now | Pull to refresh |

> [!note] Android background limits
> - Doze and App Standby stop background networking. So the phone syncs when the app opens, after local edits, and on periodic jobs.
> - There's no persistent background connection, which would drain the battery.
> - In practice the **desktop tray daemon is usually online**, so the phone catches up within seconds of being opened.

## Security
- Transport encryption comes from QUIC/TLS with iroh keys.
- Only NodeIds in `trusted_peers` are accepted.
- Pairing secrets are one-time and short-lived.
- The local database isn't encrypted in v1, since it relies on OS disk encryption (FileVault, LUKS, Android FBE). See [[11 Open Questions]].

## Testing the sync engine
- Property-based tests (`proptest`): random op sequences applied in random orders on N simulated replicas must converge to identical state.
- A network simulation of partitions, duplicated delivery and clock skew.
- A golden test: an exported op-log replays to the same state.
