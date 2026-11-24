import 'dart:io';

// kalender is built on flutter/material; material_ui is a separate copy that it can't see.
// ignore: deprecated_member_use
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:tick_alt/main.dart';
import 'package:tick_alt/src/rust/api/sync.dart';
import 'package:tick_alt/src/rust/api/tasks.dart';
import 'package:tick_alt/src/rust/frb_generated.dart';

/// Runs on a real device, emulator or desktop: Dart -> Rust core -> SQLite through the M2 shell.
void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  setUpAll(() async => await RustLib.init());

  /// `pumpAndSettle` waits for frames only; the Rust calls behind a reload finish on their
  /// own time, so also give them a moment in real time before settling.
  Future<void> settle(WidgetTester tester) async {
    for (var i = 0; i < 6; i++) {
      await tester.pump(const Duration(milliseconds: 50));
    }
    await tester.pumpAndSettle();
  }

  Future<TaskDb> pumpApp(
    WidgetTester tester, {
    Size size = const Size(1280, 800),
  }) async {
    tester.view.physicalSize = size;
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    final dir = await Directory.systemTemp.createTemp('tt');
    final db = await TaskDb.open(path: '${dir.path}/tt.db');
    final peer = SyncPeer.start(
      dbPath: '${dir.path}/tt.db',
      keyPath: '${dir.path}/tt.key',
      device: 'test',
    );
    await tester.pumpWidget(TickAltApp(db: db, peer: peer));
    await settle(tester);
    return db;
  }

  Future<void> addTask(WidgetTester tester, String title) async {
    await tester.enterText(find.byKey(const Key('add-field')), title);
    await tester.testTextInput.receiveAction(TextInputAction.done);
    await settle(tester);
  }

  Finder checkOf(String title) => find.descendant(
    of: find
        .ancestor(of: find.text(title), matching: find.byType(InkWell))
        .first,
    matching: find.byWidgetPredicate(
      (w) =>
          w.key is ValueKey<String> &&
          (w.key as ValueKey<String>).value.startsWith('check-'),
    ),
  );

  testWidgets('add a task, then complete it', (tester) async {
    final db = await pumpApp(tester);
    expect(find.text('Inbox is empty'), findsOneWidget);

    await addTask(tester, 'Buy milk');
    expect(find.text('Buy milk'), findsOneWidget);
    expect((await db.inbox()).length, 1);

    await tester.tap(checkOf('Buy milk'));
    await settle(tester);
    expect(find.text('Inbox is empty'), findsOneWidget);
    expect(await db.inbox(), isEmpty);

    await tester.tap(find.text('Completed'));
    await settle(tester);
    expect(find.text('Buy milk'), findsOneWidget);
  });

  testWidgets(
    'the detail pane edits priority and tags; a tag moves the task out of Inbox',
    (tester) async {
      final db = await pumpApp(tester);
      await addTask(tester, 'Water the plants');
      await tester.tap(find.text('Water the plants'));
      await settle(tester);
      expect(find.byKey(const Key('detail-title')), findsOneWidget);

      await tester.tap(find.byKey(const Key('priority-3')));
      await settle(tester);
      final id = (await db.inbox()).single.id;
      expect((await db.task(id: id))!.row.priority, 3);

      await tester.tap(find.byKey(const Key('add-tag')));
      await settle(tester);
      await tester.enterText(find.byKey(const Key('tag-field')), 'home/garden');
      await tester.testTextInput.receiveAction(TextInputAction.done);
      await settle(tester);

      // Tagged, so it's triaged: out of Inbox, and the tag tree shows home > garden.
      expect(await db.inbox(), isEmpty);
      expect(find.text('Inbox is empty'), findsOneWidget);
      expect(find.text('garden'), findsWidgets);
      await tester.tap(find.text('garden').first);
      await settle(tester);
      expect(find.text('Water the plants'), findsWidgets);
    },
  );

  testWidgets('trash and restore from the detail pane', (tester) async {
    final db = await pumpApp(tester);
    await addTask(tester, 'Old idea');
    await tester.tap(find.text('Old idea'));
    await settle(tester);
    await tester.tap(find.byKey(const Key('detail-trash')));
    await settle(tester);
    expect(await db.inbox(), isEmpty);

    await tester.tap(find.text('Trash'));
    await settle(tester);
    await tester.tap(find.text('Old idea'));
    await settle(tester);
    expect(find.text('Restore'), findsOneWidget);
    await tester.tap(find.byKey(const Key('detail-trash')));
    await settle(tester);
    expect((await db.inbox()).length, 1);
  });

  testWidgets('keyboard: n adds, arrows select, space completes', (
    tester,
  ) async {
    final db = await pumpApp(tester);
    await addTask(tester, 'First');
    await addTask(tester, 'Second');
    // Esc leaves the quick-add field so the shortcuts apply.
    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await settle(tester);
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowDown);
    await settle(tester);
    await settle(tester);
    expect(find.byKey(const Key('detail-title')), findsOneWidget);
    await tester.sendKeyEvent(LogicalKeyboardKey.space);
    await settle(tester);
    expect((await db.inbox()).length, 1);

    await tester.sendKeyEvent(LogicalKeyboardKey.keyN);
    await settle(tester);
    expect(FocusManager.instance.primaryFocus?.debugLabel, 'quick-add');
  });

  testWidgets('narrow windows: drawer sidebar and a pushed detail', (
    tester,
  ) async {
    await pumpApp(tester, size: const Size(420, 860));
    await addTask(tester, 'Phone task');
    await tester.tap(find.text('Phone task'));
    await settle(tester);
    expect(find.byKey(const Key('detail-title')), findsOneWidget);
    await tester.pageBack();
    await settle(tester);
    await tester.tap(find.byKey(const Key('open-drawer')));
    await settle(tester);
    expect(find.text('Completed'), findsOneWidget);
  });
}
