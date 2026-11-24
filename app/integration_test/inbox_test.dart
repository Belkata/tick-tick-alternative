import 'dart:io';

// kalender is built on flutter/material; material_ui is a separate copy that it can't see.
// ignore: deprecated_member_use
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:tick_alt/main.dart';
import 'package:tick_alt/src/rust/api/sync.dart';
import 'package:tick_alt/src/rust/api/tasks.dart';
import 'package:tick_alt/src/rust/frb_generated.dart';

/// Runs on a real device or emulator: proves Dart -> Rust core -> SQLite works.
void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  setUpAll(() async => await RustLib.init());

  testWidgets('add a task, then complete it', (tester) async {
    final dir = await Directory.systemTemp.createTemp('tt');
    final db = await TaskDb.open(path: '${dir.path}/tt.db');

    final peer = SyncPeer.start(
      dbPath: '${dir.path}/tt.db',
      keyPath: '${dir.path}/tt.key',
      device: 'test',
    );

    await tester.pumpWidget(TickAltApp(db: db, peer: peer));
    await tester.pumpAndSettle();
    expect(find.text('Inbox is empty'), findsOneWidget);

    await tester.enterText(find.byKey(const Key('add-field')), 'Buy milk');
    await tester.testTextInput.receiveAction(TextInputAction.done);
    await tester.pumpAndSettle();
    expect(find.text('Buy milk'), findsOneWidget);
    expect((await db.inbox()).length, 1);

    await tester.tap(find.byType(Checkbox));
    await tester.pumpAndSettle();
    expect(find.text('Inbox is empty'), findsOneWidget);
    expect(await db.inbox(), isEmpty);
  });
}
