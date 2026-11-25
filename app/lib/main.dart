import 'dart:async';

// kalender is built on flutter/material; material_ui is a separate copy that it can't see.
// ignore: deprecated_member_use
import 'package:flutter/material.dart';

import 'dart:io';

import 'package:path_provider/path_provider.dart';

import 'calendar/calendar_page.dart';
import 'desk/devices.dart';
import 'desk/google.dart';
import 'desk/shell.dart';
import 'desk/theme.dart';
import 'src/rust/api/sync.dart';
import 'src/rust/api/tasks.dart';
import 'src/rust/frb_generated.dart';
import 'sync/sync_page.dart';
import 'widget_bridge.dart';

/// Tick Alt: the M2 three-pane shell (Soft style with Material tag chips, plan/09)
/// over the Rust core, with the M5 calendar (plan/18). Desktop sync lives in utre-daemon.
Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  await RustLib.init();
  final desktop = Platform.isLinux || Platform.isMacOS;
  if (!desktop) {
    // Android has no /etc/localtime: tell the core the zone before anything reads it.
    final tz = await WidgetBridge.timeZone();
    if (tz != null) setTimeZone(name: tz);
  }
  final dir = await getApplicationSupportDirectory();
  // Desktop shares tt's database; Android keeps it in the app's files dir (the widget reads it there).
  final dbPath = desktop ? desktopDbPath() : '${dir.path}/tt.db';
  // On desktop, go through utre-daemon when it serves this database (plan/15).
  final db = desktop
      ? await TaskDb.connectOrOpen(path: dbPath)
      : await TaskDb.open(path: dbPath);
  runApp(TickAltApp(db: db));
  if (!desktop) {
    // Android syncs in-process (no daemon), after the first frame so startup isn't held up.
    final key = await WidgetBridge.keyPath();
    final name = await WidgetBridge.deviceName();
    if (key != null) {
      unawaited(
        db
            .startPhoneSync(keyPath: key, device: name ?? 'phone')
            .catchError((Object e) => debugPrint('sync: $e')),
      );
    }
    unawaited(WidgetBridge.askNotifications());
    unawaited(WidgetBridge.changed());
  }
}

class TickAltApp extends StatelessWidget {
  const TickAltApp({super.key, required this.db, this.peer});

  final TaskDb db;

  /// The Spike 2 sync node, for its diagnostics page (tests only; real sync is on Devices).
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
            (_, c) => CalendarPage(c: c),
            railSidebar: true,
            ownMenuButton: true,
            top: true,
          ),
          if (peer case final p?)
            ShellPage('Sync (spike)', Icons.sync, (_, _) => SyncPage(peer: p)),
          ShellPage('Devices', Icons.devices, (_, _) => DevicesPage(db: db)),
          // Google runs in utre-daemon, so the desktop only (the phone has no daemon yet).
          if (!Platform.isAndroid)
            ShellPage(
              'Google Calendar',
              Icons.event_repeat_outlined,
              (_, _) => GooglePage(db: db),
            ),
        ],
      ),
    );
  }
}
