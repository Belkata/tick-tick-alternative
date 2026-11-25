import 'dart:async';

// ignore: deprecated_member_use
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../src/rust/api/tasks.dart';
import 'filter_builder.dart' show confirm;
import 'sidebar.dart' show askText;
import 'theme.dart';

/// Desktop Devices page (M3, plan/16): pairing, paired devices and sync status. Sync runs in
/// tt-daemon, so everything here goes through it; without a daemon the page says so.
class DevicesPage extends StatefulWidget {
  const DevicesPage({super.key, required this.db});

  final TaskDb db;

  @override
  State<DevicesPage> createState() => _DevicesPageState();
}

class _DevicesPageState extends State<DevicesPage> {
  SyncStatus? _status;
  List<Peer> _peers = const [];
  String? _error;
  bool _busy = false;
  StreamSubscription<int>? _changes;
  Timer? _clock;

  TaskDb get db => widget.db;

  @override
  void initState() {
    super.initState();
    _changes = db.changes().listen((_) => _load());
    // Keep "synced 2 min ago" current.
    _clock = Timer.periodic(const Duration(seconds: 30), (_) => _load());
    _load();
  }

  @override
  void dispose() {
    _changes?.cancel();
    _clock?.cancel();
    super.dispose();
  }

  Future<void> _load() async {
    if (!db.isRemote()) {
      if (mounted) setState(() => _status = null);
      return;
    }
    try {
      final status = await db.syncStatus();
      final peers = await db.peers();
      if (!mounted) return;
      setState(() {
        _status = status;
        _peers = peers;
        _error = null;
      });
    } catch (e) {
      if (mounted) setState(() => _error = _clean(e));
    }
  }

  Future<void> _run(Future<void> Function() action) async {
    setState(() => _busy = true);
    try {
      await action();
    } catch (e) {
      _snack(_clean(e));
    } finally {
      if (mounted) setState(() => _busy = false);
      await _load();
    }
  }

  void _snack(String text) {
    if (mounted) {
      ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text(text)));
    }
  }

  Future<void> _pair() => _run(() async {
    final ticket = await db.syncPair();
    if (!mounted) return;
    await showDialog<void>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: Text('Pair a device', style: manrope(17, FontWeight.w800)),
        content: SizedBox(
          width: 460,
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              const Text(
                'On the other device, open Devices → Join and paste this ticket, '
                'or run "tt sync join <ticket>". It works once, for 5 minutes.',
              ),
              const SizedBox(height: 12),
              Container(
                padding: const EdgeInsets.all(10),
                decoration: BoxDecoration(
                  color: ctx.soft.bg,
                  borderRadius: BorderRadius.circular(10),
                  border: Border.all(color: ctx.soft.line),
                ),
                constraints: const BoxConstraints(maxHeight: 140),
                child: SingleChildScrollView(
                  child: SelectableText(
                    ticket,
                    key: const Key('pair-ticket'),
                    style: const TextStyle(
                      fontFamily: 'monospace',
                      fontSize: 11,
                    ),
                  ),
                ),
              ),
            ],
          ),
        ),
        actions: [
          TextButton.icon(
            icon: const Icon(Icons.copy, size: 16),
            label: const Text('Copy'),
            onPressed: () {
              Clipboard.setData(ClipboardData(text: ticket));
              ScaffoldMessenger.of(context)
                  .showSnackBar(const SnackBar(content: Text('Ticket copied')));
            },
          ),
          FilledButton(
            onPressed: () => Navigator.pop(ctx),
            child: const Text('Done'),
          ),
        ],
      ),
    );
  });

  Future<void> _join() async {
    final ticket = await askText(
      context,
      title: 'Join a device',
      hint: 'Paste the tt2… ticket',
    );
    if (ticket == null || ticket.trim().isEmpty) return;
    await _run(() async {
      final o = await db.syncJoin(ticket: ticket);
      _snack(
        'Paired with ${o.peerName}: received ${o.received}, sent ${o.sent}',
      );
    });
  }

  Future<void> _syncNow() => _run(() async {
    final all = await db.syncNow();
    if (all.isEmpty) return _snack('No paired devices yet');
    final ok = all.where((o) => o.error == null).length;
    _snack(
      ok == all.length
          ? 'Synced with ${all.map((o) => o.peerName).join(', ')}'
          : 'Synced with $ok of ${all.length}; '
                '${all.where((o) => o.error != null).map((o) => o.peerName).join(', ')} offline',
    );
  });

  Future<void> _forget(Peer p) async {
    final ok = await confirm(
      context,
      title: 'Forget ${p.name}?',
      body: 'It stops syncing with this device until you pair again. Nothing is deleted.',
      action: 'Forget',
    );
    if (ok) await _run(() async => await db.forgetPeer(nodeId: p.nodeId));
  }

  @override
  Widget build(BuildContext context) {
    final s = context.soft;
    final status = _status;
    final body = <Widget>[
      Text('Devices', style: manrope(19, FontWeight.w800, color: s.fg)),
      const SizedBox(height: 14),
    ];
    if (status == null) {
      body.add(
        _Card(
          child: Text(
            _error ??
                'Sync runs in tt-daemon, which isn\'t running for this database.\n'
                    'Start it with: systemctl --user start tick-alt',
            style: manrope(13.5, FontWeight.w500, color: s.mute),
          ),
        ),
      );
    } else {
      final line = status.lastSyncMs != null
          ? 'Last synced ${_ago(status.lastSyncMs!)}'
          : (status.peers == 0 ? 'No paired devices yet' : 'Not synced yet');
      body.addAll([
        _Card(
          child: Row(
            children: [
              Icon(Icons.computer, color: s.accent),
              const SizedBox(width: 12),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text('This device', style: manrope(14, FontWeight.w700)),
                    Text(
                      '$line · id ${status.nodeId.substring(0, 10)}',
                      key: const Key('sync-line'),
                      style: manrope(12, FontWeight.w500, color: s.mute),
                    ),
                    if (status.lastError != null)
                      Text(
                        status.lastError!,
                        style: manrope(12, FontWeight.w500, color: s.late),
                      ),
                  ],
                ),
              ),
            ],
          ),
        ),
        const SizedBox(height: 12),
        Wrap(
          spacing: 8,
          runSpacing: 8,
          children: [
            FilledButton.icon(
              key: const Key('pair-device'),
              onPressed: _busy ? null : _pair,
              icon: const Icon(Icons.qr_code_2, size: 18),
              label: const Text('Pair a device'),
            ),
            OutlinedButton.icon(
              key: const Key('join-device'),
              onPressed: _busy ? null : _join,
              icon: const Icon(Icons.link, size: 18),
              label: const Text('Join a device'),
            ),
            OutlinedButton.icon(
              key: const Key('sync-now'),
              onPressed: _busy || _peers.isEmpty ? null : _syncNow,
              icon: const Icon(Icons.sync, size: 18),
              label: const Text('Sync now'),
            ),
            if (_busy)
              const Padding(
                padding: EdgeInsets.all(8),
                child: SizedBox(
                  width: 18,
                  height: 18,
                  child: CircularProgressIndicator(strokeWidth: 2),
                ),
              ),
          ],
        ),
        const SizedBox(height: 18),
        Text(
          'Paired devices',
          style: manrope(12, FontWeight.w700, color: s.mute),
        ),
        const SizedBox(height: 8),
        if (_peers.isEmpty)
          Text(
            'Pair your other computer or phone to sync tasks between them, peer to peer.',
            style: manrope(13, FontWeight.w500, color: s.mute),
          ),
        for (final p in _peers)
          _Card(
            child: Row(
              children: [
                Icon(Icons.devices_other, color: s.mute),
                const SizedBox(width: 12),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(p.name, style: manrope(14, FontWeight.w700)),
                      Text(
                        '${p.lastSeen == null ? 'never synced' : 'synced ${_ago(p.lastSeen!)}'}'
                        ' · id ${p.nodeId.substring(0, 10)}',
                        style: manrope(12, FontWeight.w500, color: s.mute),
                      ),
                    ],
                  ),
                ),
                TextButton(
                  onPressed: () => _forget(p),
                  child: const Text('Forget'),
                ),
              ],
            ),
          ),
      ]);
    }
    return Container(
      color: s.bg,
      child: ListView(
        padding: const EdgeInsets.fromLTRB(24, 18, 24, 24),
        children: body,
      ),
    );
  }
}

class _Card extends StatelessWidget {
  const _Card({required this.child});

  final Widget child;

  @override
  Widget build(BuildContext context) {
    final s = context.soft;
    return Padding(
      padding: const EdgeInsets.only(bottom: 8),
      child: Material(
        color: s.panel,
        elevation: s.cardShadow ? 0.6 : 0,
        shadowColor: const Color(0x22141E3C),
        borderRadius: BorderRadius.circular(12),
        child: Padding(padding: const EdgeInsets.all(14), child: child),
      ),
    );
  }
}

String _ago(int ms) {
  final s = (DateTime.now().millisecondsSinceEpoch - ms) ~/ 1000;
  if (s < 60) return 'just now';
  if (s < 3600) return '${s ~/ 60} min ago';
  if (s < 86400) return '${s ~/ 3600} h ago';
  return '${s ~/ 86400} days ago';
}

String _clean(Object e) => '$e'
    .replaceFirst(RegExp(r'^(AnyhowException|Exception)\(?'), '')
    .replaceFirst(RegExp(r'\)$'), '');
