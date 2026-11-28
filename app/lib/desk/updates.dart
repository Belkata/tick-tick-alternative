// The phone's "Updates" page: Utre updating itself from GitHub releases (Updater.kt, plan/17).

import 'dart:async';

// ignore: deprecated_member_use
import 'package:flutter/material.dart';

import '../widget_bridge.dart';
import 'theme.dart';

/// What the page needs from the Android side, so a widget test can stand in for it.
abstract class UpdatesSource {
  Future<Map<String, dynamic>?> state();
  Future<void> check();
  Future<void> setAuto(bool on);
  Future<void> installSettings();
}

class BridgeUpdates implements UpdatesSource {
  @override
  Future<Map<String, dynamic>?> state() => WidgetBridge.updateState();
  @override
  Future<void> check() => WidgetBridge.updateCheck();
  @override
  Future<void> setAuto(bool on) => WidgetBridge.updateSetAuto(on);
  @override
  Future<void> installSettings() => WidgetBridge.updateInstallSettings();
}

class UpdatesPage extends StatefulWidget {
  const UpdatesPage({super.key, required this.source});

  final UpdatesSource source;

  @override
  State<UpdatesPage> createState() => _UpdatesPageState();
}

class _UpdatesPageState extends State<UpdatesPage> with WidgetsBindingObserver {
  Map<String, dynamic>? _s;
  Timer? _clock;

  UpdatesSource get u => widget.source;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    _clock = Timer.periodic(const Duration(seconds: 1), (_) => _load());
    _load();
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    _clock?.cancel();
    super.dispose();
  }

  /// Coming back from the "install unknown apps" screen: show the new permission.
  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state == AppLifecycleState.resumed) _load();
  }

  Future<void> _load() async {
    final s = await u.state();
    if (mounted) setState(() => _s = s);
  }

  String _statusText(Map<String, dynamic> s) {
    final latest = s['latest'];
    switch (s['status']) {
      case 'checking':
        return 'Checking…';
      case 'downloading':
        return 'Downloading $latest…';
      case 'installing':
        return 'Installing $latest…';
      case 'ready':
        return 'Ready to install $latest';
      case 'needs_permission':
        return 'Utre $latest is available. Allow installs to update.';
      case 'wrong_key':
        return (s['message'] as String?) ??
            "This update is signed with Utre's new release key. Sync your tasks, then "
                "uninstall Utre and install $latest from GitHub once; updates work again after that.";
      case 'error':
        return (s['message'] as String?) ?? 'Something went wrong';
      case 'uptodate':
        return 'Up to date';
      default:
        return 'Not checked yet';
    }
  }

  @override
  Widget build(BuildContext context) {
    final soft = context.soft;
    final s = _s;
    final busy = s != null &&
        const {'checking', 'downloading', 'installing'}.contains(s['status']);
    final children = <Widget>[
      Text('Updates', style: manrope(19, FontWeight.w800, color: soft.fg)),
      const SizedBox(height: 14),
    ];
    if (s == null) {
      children.add(_card(context, Text('Loading…', style: manrope(13.5, FontWeight.w500, color: soft.mute))));
    } else {
      children.addAll([
        _card(
          context,
          Row(
            children: [
              Icon(
                const {'error', 'wrong_key'}.contains(s['status']) ? Icons.error_outline : Icons.system_update_alt,
                color: const {'error', 'wrong_key'}.contains(s['status']) ? soft.late : soft.accent,
              ),
              const SizedBox(width: 12),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text('Utre ${s['current'] ?? ''}', style: manrope(14, FontWeight.w700)),
                    Text(
                      _statusText(s),
                      key: const Key('update-status'),
                      style: manrope(12, FontWeight.w500, color: soft.mute),
                    ),
                  ],
                ),
              ),
            ],
          ),
        ),
        if (s['canInstall'] == false)
          _card(
            context,
            Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  'Utre needs permission to install updates. Turn on "Allow from this '
                  'source" for Utre, then come back.',
                  key: const Key('update-permission'),
                  style: manrope(13, FontWeight.w500, color: soft.mute),
                ),
                const SizedBox(height: 8),
                OutlinedButton(
                  key: const Key('update-permission-button'),
                  onPressed: u.installSettings,
                  child: const Text('Open settings'),
                ),
              ],
            ),
          ),
        Align(
          alignment: Alignment.centerLeft,
          child: FilledButton.icon(
            key: const Key('update-check'),
            onPressed: busy
                ? null
                : () async {
                    await u.check();
                    await _load();
                  },
            icon: const Icon(Icons.refresh, size: 18),
            label: const Text('Check for updates'),
          ),
        ),
        const SizedBox(height: 10),
        _card(
          context,
          SwitchListTile(
            key: const Key('update-auto'),
            contentPadding: EdgeInsets.zero,
            title: Text('Install updates automatically', style: manrope(14, FontWeight.w700)),
            subtitle: Text(
              'Checks once a day on Wi-Fi and installs a newer release from GitHub.',
              style: manrope(12, FontWeight.w500, color: soft.mute),
            ),
            value: s['auto'] != false,
            onChanged: (on) async {
              await u.setAuto(on);
              await _load();
            },
          ),
        ),
      ]);
    }
    return ListView(
      padding: const EdgeInsets.fromLTRB(16, 16, 16, 24),
      children: children,
    );
  }

  Widget _card(BuildContext context, Widget child) {
    final soft = context.soft;
    return Padding(
      padding: const EdgeInsets.only(bottom: 8),
      child: Material(
        color: soft.panel,
        elevation: soft.cardShadow ? 0.6 : 0,
        shadowColor: const Color(0x22141E3C),
        borderRadius: BorderRadius.circular(12),
        child: Padding(padding: const EdgeInsets.all(14), child: child),
      ),
    );
  }
}
