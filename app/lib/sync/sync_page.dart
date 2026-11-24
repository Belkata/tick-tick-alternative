// ignore: deprecated_member_use
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'dart:async';

import '../src/rust/api/sync.dart';

/// Spike 2: show this device's iroh ticket, pull a peer's open tasks by ticket,
/// and list who pulled from us. The real pairing and sync UI comes in M3/M4.
class SyncPage extends StatefulWidget {
  const SyncPage({super.key, required this.peer});

  /// Completes once the node is bound; fails if it can't start.
  final Future<SyncPeer> peer;

  @override
  State<SyncPage> createState() => _SyncPageState();
}

class _SyncPageState extends State<SyncPage> {
  final _ticketInput = TextEditingController();
  SyncPeer? _peer;
  String _status = 'Starting…';
  String? _nodeId;
  String? _ticket;
  List<String> _addrs = [];
  List<ServedItem> _served = [];
  bool _pulling = false;
  PullResult? _result;
  String? _error;
  Timer? _poll;

  @override
  void initState() {
    super.initState();
    _init();
  }

  @override
  void dispose() {
    _poll?.cancel();
    _ticketInput.dispose();
    super.dispose();
  }

  Future<void> _init() async {
    try {
      final peer = await widget.peer;
      _peer = peer;
      final id = await peer.nodeId();
      if (mounted) setState(() => _nodeId = id);
      final online = await peer.waitOnline(timeoutMs: BigInt.from(10000));
      await _refreshAddress();
      if (mounted) {
        setState(
          () => _status = online
              ? 'Online (relay connected)'
              : 'No relay: direct addresses only',
        );
      }
      _poll = Timer.periodic(
        const Duration(seconds: 2),
        (_) => _refreshServed(),
      );
    } catch (e) {
      if (mounted) setState(() => _status = 'Failed to start: $e');
    }
  }

  Future<void> _refreshAddress() async {
    final peer = _peer;
    if (peer == null) return;
    final ticket = await peer.ticket();
    final addrs = await peer.addrs();
    // Logged so the ticket can be read with `adb logcat` during the spike.
    // ignore: avoid_print
    print('TT_TICKET $ticket');
    if (mounted) {
      setState(() {
        _ticket = ticket;
        _addrs = addrs;
      });
    }
  }

  Future<void> _refreshServed() async {
    final served = await _peer?.served();
    if (served != null && mounted && served.length != _served.length) {
      setState(() => _served = served);
    }
  }

  Future<void> _pull() async {
    final peer = _peer;
    final ticket = _ticketInput.text.trim();
    if (peer == null || ticket.isEmpty) return;
    setState(() {
      _pulling = true;
      _error = null;
      _result = null;
    });
    try {
      final r = await peer.pull(ticket: ticket);
      // ignore: avoid_print
      print(
        'TT_PULL ${r.titles.length} tasks from ${r.peerDevice} '
        '${r.firstPath}->${r.finalPath} rtt=${r.rttMs} ms in ${r.elapsedMs} ms',
      );
      if (mounted) setState(() => _result = r);
    } catch (e) {
      // ignore: avoid_print
      print('TT_PULL_ERROR $e');
      if (mounted) setState(() => _error = '$e');
    } finally {
      if (mounted) setState(() => _pulling = false);
    }
  }

  Future<void> _paste() async {
    final data = await Clipboard.getData(Clipboard.kTextPlain);
    if (data?.text != null) _ticketInput.text = data!.text!.trim();
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final ticket = _ticket;
    return Scaffold(
      appBar: AppBar(
        title: const Text('Sync (spike)'),
        actions: [
          IconButton(
            tooltip: 'Refresh address',
            onPressed: _refreshAddress,
            icon: const Icon(Icons.refresh),
          ),
        ],
      ),
      body: ListView(
        padding: const EdgeInsets.all(16),
        children: [
          Text(
            _status,
            key: const Key('sync-status'),
            style: theme.textTheme.titleMedium,
          ),
          if (_nodeId != null)
            Text(
              'Node ${_nodeId!.substring(0, 10)}…',
              style: theme.textTheme.bodySmall,
            ),
          for (final a in _addrs) Text(a, style: theme.textTheme.bodySmall),
          const SizedBox(height: 12),
          if (ticket != null)
            Card(
              child: ListTile(
                title: const Text('This device’s ticket'),
                subtitle: Text(
                  ticket,
                  maxLines: 3,
                  overflow: TextOverflow.ellipsis,
                ),
                trailing: IconButton(
                  key: const Key('copy-ticket'),
                  tooltip: 'Copy ticket',
                  icon: const Icon(Icons.copy),
                  onPressed: () =>
                      Clipboard.setData(ClipboardData(text: ticket)),
                ),
              ),
            ),
          const SizedBox(height: 16),
          TextField(
            key: const Key('peer-ticket'),
            controller: _ticketInput,
            maxLines: 3,
            decoration: InputDecoration(
              labelText: 'Peer ticket (tt1…)',
              border: const OutlineInputBorder(),
              suffixIcon: IconButton(
                tooltip: 'Paste',
                icon: const Icon(Icons.content_paste),
                onPressed: _paste,
              ),
            ),
          ),
          const SizedBox(height: 8),
          FilledButton.icon(
            key: const Key('pull'),
            onPressed: _pulling || _peer == null ? null : _pull,
            icon: _pulling
                ? const SizedBox.square(
                    dimension: 16,
                    child: CircularProgressIndicator(strokeWidth: 2),
                  )
                : const Icon(Icons.download),
            label: const Text('Pull open tasks'),
          ),
          if (_error != null)
            Padding(
              padding: const EdgeInsets.only(top: 12),
              child: Text(
                _error!,
                key: const Key('pull-error'),
                style: TextStyle(color: theme.colorScheme.error),
              ),
            ),
          if (_result != null) _PullCard(result: _result!),
          const SizedBox(height: 16),
          Text('Served (${_served.length})', style: theme.textTheme.titleSmall),
          for (final s in _served.reversed)
            Text(
              '${s.taskCount} tasks → ${s.peerDevice} (${s.peerId.substring(0, 10)})',
            ),
        ],
      ),
    );
  }
}

class _PullCard extends StatelessWidget {
  const _PullCard({required this.result});

  final PullResult result;

  @override
  Widget build(BuildContext context) {
    final r = result;
    return Card(
      key: const Key('pull-result'),
      margin: const EdgeInsets.only(top: 12),
      child: Padding(
        padding: const EdgeInsets.all(12),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              '${r.titles.length} tasks from ${r.peerDevice} in ${r.elapsedMs} ms',
              style: Theme.of(context).textTheme.titleSmall,
            ),
            Text(
              'Path: ${r.firstPath} → ${r.finalPath}, rtt ${r.rttMs ?? '?'} ms',
            ),
            for (final p in r.paths)
              Text(p, style: Theme.of(context).textTheme.bodySmall),
            const Divider(),
            for (final t in r.titles) Text('• $t'),
          ],
        ),
      ),
    );
  }
}
