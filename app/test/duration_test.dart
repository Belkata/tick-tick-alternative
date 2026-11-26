// ignore: deprecated_member_use
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:utre/calendar/calendar_page.dart';
import 'package:utre/desk/duration_picker.dart';
import 'package:utre/desk/shell.dart';
import 'package:utre/desk/task_list.dart';
import 'package:utre/desk/theme.dart';

import 'golden/fake_db.dart';
import 'golden/golden_utils.dart' show setSurface;

PickedDuration? result;
bool closed = false;

Future<void> open(
  WidgetTester t, {
  int? initial,
  TargetPlatform platform = TargetPlatform.linux,
}) async {
  result = null;
  closed = false;
  await t.pumpWidget(
    MaterialApp(
      theme: softTheme(Brightness.light).copyWith(platform: platform),
      home: Scaffold(
        body: Center(
          child: TextButton(
            key: const Key('open'),
            onPressed: () async {
              result = await showDurationPicker(
                t.element(find.byKey(const Key('open'))),
                initial: initial,
              );
              closed = true;
            },
            child: const Text('open'),
          ),
        ),
      ),
    ),
  );
  await t.tap(find.byKey(const Key('open')));
  await t.pumpAndSettle();
}

void main() {
  test('typed lengths', () {
    for (final (text, want) in [
      ('90', 90),
      ('45m', 45),
      ('45 min', 45),
      ('1h', 60),
      ('1h30', 90),
      ('1h30m', 90),
      ('1:30', 90),
      ('1.5h', 90),
      ('2H', 120),
    ]) {
      expect(parseDurationText(text), want, reason: text);
    }
    for (final bad in ['', '0', 'abc', '1:75', '-5', '99999', '1h2x']) {
      expect(parseDurationText(bad), isNull, reason: bad);
    }
  });

  test('labels', () {
    expect(durationLabel(45), '45 min');
    expect(durationLabel(120), '2 h');
    expect(durationLabel(90), '1 h 30 min');
    expect(durationShort(45), '45m');
    expect(durationShort(90), '1h30');
    expect(durationShort(65), '1h05');
  });

  testWidgets('a preset picks at once', (t) async {
    await open(t);
    await t.tap(find.byKey(const Key('duration-preset-45')));
    await t.pumpAndSettle();
    expect(result?.minutes, 45);
  });

  testWidgets('typed text, Enter saves; bad text blocks Done', (t) async {
    await open(t, initial: 30);
    await t.enterText(find.byKey(const Key('duration-text')), 'abc');
    await t.pump();
    expect(find.textContaining('Try 90'), findsOneWidget);
    expect(
      t.widget<FilledButton>(find.byKey(const Key('duration-done'))).onPressed,
      isNull,
    );
    await t.enterText(find.byKey(const Key('duration-text')), '1h30');
    await t.testTextInput.receiveAction(TextInputAction.done);
    await t.pumpAndSettle();
    expect(result?.minutes, 90);
  });

  testWidgets('No duration clears; Cancel dismisses', (t) async {
    await open(t, initial: 30);
    await t.tap(find.byKey(const Key('duration-clear')));
    await t.pumpAndSettle();
    expect(result, isNotNull);
    expect(result!.minutes, isNull);
    await open(t);
    await t.tap(find.byKey(const Key('duration-cancel')));
    await t.pumpAndSettle();
    expect(result, isNull);
    expect(closed, isTrue);
  });

  testWidgets('phone: sheet with 48 px targets', (t) async {
    await open(t, platform: TargetPlatform.android);
    expect(t.getSize(find.byKey(const Key('duration-done'))).height, 48);
    expect(t.getSize(find.byKey(const Key('duration-clear'))).height, 48);
  });

  testWidgets('the card shows the length in its facts', (t) async {
    await t.pumpWidget(
      MaterialApp(
        theme: softTheme(Brightness.light),
        home: Scaffold(
          body: Column(
            children: [
              TaskCard(
                row: row('a', 'Gym', duration: 90),
                selected: false,
                onTap: () {},
                onCheck: () {},
              ),
              TaskCard(
                row: row('b', 'Nothing'),
                selected: false,
                onTap: () {},
                onCheck: () {},
              ),
            ],
          ),
        ),
      ),
    );
    expect(find.text('1h30'), findsOneWidget);
    expect(find.byIcon(Icons.timelapse_outlined), findsOneWidget);
  });

  test('a card dragged in from the panel uses the task\'s length', () {
    final e = TaskEvent.draft(
      row('a', 'Gym', duration: 45),
      DateTime(2026, 11, 25, 10),
    );
    expect(e.end.toLocal(), DateTime(2026, 11, 25, 10, 45));
    final d = TaskEvent.draft(row('b', 'x'), DateTime(2026, 11, 25, 10));
    expect(d.end.toLocal(), DateTime(2026, 11, 25, 10, 30));
  });

  testWidgets('the detail row shows, edits and clears the duration', (t) async {
    setSurface(t, const Size(1280, 780));
    final db = FakeDb(tasks: sampleTasks(DateTime.now()));
    await t.pumpWidget(
      MaterialApp(
        theme: softTheme(Brightness.light),
        home: DeskShell(db: db),
      ),
    );
    await t.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 100)),
    );
    await t.pump();
    await t.pump(const Duration(milliseconds: 400));
    await t.tap(find.text('Buy groceries'));
    await t.pump();
    await t.pump(const Duration(milliseconds: 400));
    expect(find.text('Duration'), findsOneWidget);
    expect(find.text('No duration'), findsOneWidget);
    // Pick a preset.
    await t.tap(find.byKey(const Key('detail-duration')));
    await t.pumpAndSettle();
    await t.tap(find.byKey(const Key('duration-preset-60')));
    await t.pumpAndSettle();
    expect(db.durations['t3'], 60);
    expect(find.text('1 h'), findsOneWidget);
    // Clear with the x.
    await t.tap(find.byKey(const Key('detail-duration-clear')));
    await t.pumpAndSettle();
    expect(db.durations['t3'], isNull);
    expect(db.durations.containsKey('t3'), isTrue);
    expect(find.text('No duration'), findsOneWidget);
  });
}
