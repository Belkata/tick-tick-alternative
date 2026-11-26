import 'dart:async';

// ignore: deprecated_member_use
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:utre/desk/shell.dart';
import 'package:utre/desk/theme.dart';

import 'golden/fake_db.dart';

/// A core that records title writes and lets a test change the stored title.
class TitleDb extends FakeDb {
  TitleDb() : super(tasks: sampleTasks(DateTime.now()));

  final events = StreamController<int>.broadcast();
  final writes = <String>[];

  @override
  Stream<int> changes() => events.stream;

  @override
  Future<void> setTitle({required String id, required String title}) async {
    writes.add('$id:$title');
  }
}

Future<void> pumpDetail(WidgetTester t, TitleDb db) async {
  t.view.physicalSize = const Size(1280, 780);
  t.view.devicePixelRatio = 1;
  addTearDown(t.view.reset);
  await t.pumpWidget(
    MaterialApp(
      theme: softTheme(Brightness.light),
      home: DeskShell(db: db),
    ),
  );
  await t.runAsync(
    () => Future<void>.delayed(const Duration(milliseconds: 100)),
  );
  await t.pump(const Duration(milliseconds: 400));
  await t.tap(find.text('Buy groceries'));
  await t.pump(const Duration(milliseconds: 400));
}

void main() {
  testWidgets('the title is saved once after typing stops, not per keystroke', (
    t,
  ) async {
    final db = TitleDb();
    await pumpDetail(t, db);
    await t.tap(find.byKey(const Key('detail-title')));
    await t.pump();
    for (final text in [
      'Buy groceries a',
      'Buy groceries ab',
      'Buy groceries abc',
    ]) {
      await t.enterText(find.byKey(const Key('detail-title')), text);
      await t.pump(const Duration(milliseconds: 100));
    }
    expect(db.writes, isEmpty);
    await t.pump(const Duration(milliseconds: 400));
    expect(db.writes, ['t3:Buy groceries abc']);
  });

  testWidgets('Enter saves at once', (t) async {
    final db = TitleDb();
    await pumpDetail(t, db);
    await t.enterText(find.byKey(const Key('detail-title')), 'Shop');
    await t.testTextInput.receiveAction(TextInputAction.done);
    await t.pump();
    expect(db.writes, ['t3:Shop']);
  });

  testWidgets('an unsaved edit is flushed when the field loses focus', (
    t,
  ) async {
    final db = TitleDb();
    await pumpDetail(t, db);
    await t.enterText(find.byKey(const Key('detail-title')), 'Shop soon');
    await t.pump(const Duration(milliseconds: 50));
    FocusManager.instance.primaryFocus?.unfocus();
    await t.pump();
    expect(db.writes, ['t3:Shop soon']);
  });

  testWidgets('a focused field that was not edited writes nothing on blur', (
    t,
  ) async {
    final db = TitleDb();
    await pumpDetail(t, db);
    await t.tap(find.byKey(const Key('detail-title')));
    await t.pump();
    FocusManager.instance.primaryFocus?.unfocus();
    await t.pump(const Duration(milliseconds: 400));
    expect(db.writes, isEmpty);
  });
}
