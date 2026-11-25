// kalender is built on flutter/material; material_ui is a separate copy that it can't see.
// ignore: deprecated_member_use
import 'package:flutter/material.dart';

import 'dart:io';

import 'package:path_provider/path_provider.dart';

import 'calendar/spike_calendar.dart';
import 'desk/devices.dart';
import 'desk/shell.dart';
import 'desk/theme.dart';
import 'src/rust/api/sync.dart';
import 'src/rust/api/tasks.dart';
import 'src/rust/frb_generated.dart';
import 'sync/sync_page.dart';

/// Tick Alt: the M2 three-pane shell (Soft style with Material tag chips, plan/09)
/// over the Rust core. The calendar is still an M0 spike; desktop sync lives in tt-daemon.
Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  await RustLib.init();
  final dir = await getApplicationSupportDirectory();
  // Desktop shares tt's database; Android keeps it in the app's files dir (the widget reads it there).
  final dbPath = Platform.isLinux || Platform.isMacOS
      ? desktopDbPath()
      : '${dir.path}/tt.db';
  // On desktop, go through tt-daemon when it serves this database (plan/15).
  final db = Platform.isLinux || Platform.isMacOS
      ? await TaskDb.connectOrOpen(path: dbPath)
      : await TaskDb.open(path: dbPath);
  // Desktop: sync runs in tt-daemon, which owns the device key (Devices page, plan/16).
  // Android: the Spike 2 node until M4; bound in the background so the first frame isn't held up.
  final peer = Platform.isLinux || Platform.isMacOS
      ? null
      : SyncPeer.start(
          dbPath: dbPath,
          keyPath: dbPath.replaceFirst(RegExp(r'\.db$'), '.key'),
          device: '${Platform.operatingSystem}-${Platform.localHostname}',
        );
  runApp(TickAltApp(db: db, peer: peer));
}

class TickAltApp extends StatelessWidget {
  const TickAltApp({super.key, required this.db, this.peer});

  final TaskDb db;

  /// The Spike 2 sync node (Android); `null` on desktop, where tt-daemon syncs.
  final Future<SyncPeer>? peer;

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'Tick Alt',
      theme: softTheme(Brightness.light),
      darkTheme: softTheme(Brightness.dark),
      home: DeskShell(
        db: db,
        pages: [
          ShellPage(
            'Calendar',
            Icons.calendar_month_outlined,
            (_) => const SpikeCalendarPage(),
          ),
          if (peer case final p?)
            ShellPage('Sync (spike)', Icons.sync, (_) => SyncPage(peer: p))
          else
            ShellPage('Devices', Icons.devices, (_) => DevicesPage(db: db)),
        ],
      ),
    );
  }
}
