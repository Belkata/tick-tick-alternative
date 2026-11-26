// ignore: deprecated_member_use
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:utre/desk/date_picker.dart';
import 'package:utre/desk/theme.dart';

// Wednesday 2026-11-25, so "next week" is Monday 30 Nov.
final now = DateTime(2026, 11, 25, 10, 30);

/// Opens the picker from a button and keeps what it returns in [result].
class _Host extends StatefulWidget {
  const _Host({this.initial, this.other, this.isStart = false});
  final When? initial;
  final WhenLimit? other;
  final bool isStart;

  @override
  State<_Host> createState() => _HostState();
}

PickedWhen? result;
bool closed = false;

class _HostState extends State<_Host> {
  @override
  Widget build(BuildContext context) => Center(
    child: TextButton(
      key: const Key('open'),
      onPressed: () async {
        result = await showWhenPicker(
          context,
          title: 'Due date',
          isStart: widget.isStart,
          initial: widget.initial,
          other: widget.other,
          now: now,
        );
        closed = true;
      },
      child: const Text('open'),
    ),
  );
}

Future<void> open(
  WidgetTester t, {
  When? initial,
  WhenLimit? other,
  bool isStart = false,
  Brightness brightness = Brightness.light,
  TargetPlatform platform = TargetPlatform.linux,
}) async {
  result = null;
  closed = false;
  await t.pumpWidget(
    MaterialApp(
      theme: softTheme(brightness).copyWith(platform: platform),
      home: Scaffold(
        body: _Host(initial: initial, other: other, isStart: isStart),
      ),
    ),
  );
  await t.tap(find.byKey(const Key('open')));
  await t.pumpAndSettle();
}

Future<void> tapKey(WidgetTester t, String k) async {
  await t.tap(find.byKey(Key(k)));
  await t.pumpAndSettle();
}

void main() {
  group('helpers', () {
    test('When makes the text the core parses', () {
      expect(When(DateTime(2026, 12, 1)).text, '2026-12-01');
      expect(
        When(DateTime(2026, 12, 1), const TimeOfDay(hour: 9, minute: 5)).text,
        '2026-12-01 09:05',
      );
    });

    test('next week is the coming Monday', () {
      expect(nextWeek(DateTime(2026, 11, 25)), DateTime(2026, 11, 30));
      expect(nextWeek(DateTime(2026, 11, 30)), DateTime(2026, 12, 7));
      expect(nextWeek(DateTime(2026, 11, 29)), DateTime(2026, 11, 30));
    });

    test('stored dates: all-day is floating, timed is local', () {
      final allDay = whenFromMs(
        DateTime.utc(2026, 12, 1).millisecondsSinceEpoch,
        true,
      );
      expect(allDay, When(DateTime(2026, 12, 1)));
      final timed = whenFromMs(
        DateTime(2026, 12, 1, 14, 30).millisecondsSinceEpoch,
        false,
      );
      expect(
        timed,
        When(DateTime(2026, 12, 1), const TimeOfDay(hour: 14, minute: 30)),
      );
    });

    test('labels read back', () {
      const t = TimeOfDay(hour: 14, minute: 30);
      expect(parseWhenLabel('today', now), When(DateTime(2026, 11, 25)));
      expect(
        parseWhenLabel('tomorrow 14:30', now),
        When(DateTime(2026, 11, 26), t),
      );
      expect(parseWhenLabel('yesterday', now), When(DateTime(2026, 11, 24)));
      expect(parseWhenLabel('Sat', now), When(DateTime(2026, 11, 28)));
      expect(parseWhenLabel('Mon 14:30', now), When(DateTime(2026, 11, 30), t));
      expect(parseWhenLabel('Fri 4 Dec', now), When(DateTime(2026, 12, 4)));
      expect(parseWhenLabel('3 Dec 2027', now), When(DateTime(2027, 12, 3)));
      expect(parseWhenLabel('Sun 22 Nov', now), When(DateTime(2026, 11, 22)));
      expect(parseWhenLabel(null, now), isNull);
      expect(parseWhenLabel('someday', now), isNull);
    });

    test('typed times', () {
      expect(parseTimeText('9'), const TimeOfDay(hour: 9, minute: 0));
      expect(parseTimeText('9:30'), const TimeOfDay(hour: 9, minute: 30));
      expect(parseTimeText('0930'), const TimeOfDay(hour: 9, minute: 30));
      expect(parseTimeText('24:00'), isNull);
      expect(parseTimeText('12:60'), isNull);
      expect(parseTimeText(''), isNull);
    });

    test('start must not be after due, same kind only', () {
      final due = WhenLimit(When(DateTime(2026, 12, 1)), 'due');
      expect(
        orderProblem(
          isStart: true,
          chosen: When(DateTime(2026, 12, 2)),
          other: due,
        ),
        isNotNull,
      );
      expect(
        orderProblem(
          isStart: true,
          chosen: When(DateTime(2026, 12, 1)),
          other: due,
        ),
        isNull,
        reason: 'the same day is fine',
      );
      final start = WhenLimit(When(DateTime(2026, 12, 3)), 'start');
      expect(
        orderProblem(
          isStart: false,
          chosen: When(DateTime(2026, 12, 2)),
          other: start,
        ),
        isNotNull,
      );
      // A timed date next to an all-day one is not compared, but is noted.
      const t = TimeOfDay(hour: 8, minute: 0);
      expect(
        orderProblem(
          isStart: true,
          chosen: When(DateTime(2026, 12, 9), t),
          other: due,
        ),
        isNull,
      );
      expect(
        kindNote(chosen: When(DateTime(2026, 12, 9), t), other: due),
        isNotNull,
      );
      expect(kindNote(chosen: When(DateTime(2026, 12, 9)), other: due), isNull);
    });
  });

  group('picker', () {
    testWidgets('quick choices pick the day, Done returns it', (t) async {
      await open(t);
      await tapKey(t, 'when-tomorrow');
      await tapKey(t, 'when-done');
      expect(result?.text, '2026-11-26');

      await open(t);
      await tapKey(t, 'when-today');
      await tapKey(t, 'when-done');
      expect(result?.text, '2026-11-25');

      await open(t);
      await tapKey(t, 'when-next-week');
      await tapKey(t, 'when-done');
      expect(result?.text, '2026-11-30');
    });

    testWidgets('Done is off until a day is chosen', (t) async {
      await open(t);
      final done = find.byKey(const Key('when-done'));
      expect(t.widget<FilledButton>(done).onPressed, isNull);
      await tapKey(t, 'when-day-2026-11-12');
      expect(t.widget<FilledButton>(done).onPressed, isNotNull);
    });

    testWidgets('No date clears and closes', (t) async {
      await open(t, initial: When(DateTime(2026, 12, 1)));
      await tapKey(t, 'when-clear');
      expect(closed, isTrue);
      expect(result, isNotNull);
      expect(result!.when, isNull);
      expect(result!.text, isNull);
    });

    testWidgets('PageDown moves the chosen day a month, keeping it in view', (
      t,
    ) async {
      await open(t, initial: When(DateTime(2026, 1, 31)));
      await t.sendKeyEvent(LogicalKeyboardKey.pageDown);
      await t.pumpAndSettle();
      expect(find.text('February 2026'), findsOneWidget);
      // The 31st clamps to the end of February, and is the day that gets saved.
      await tapKey(t, 'when-done');
      expect(result!.when, When(DateTime(2026, 2, 28)));
    });

    testWidgets('PageUp with no day chosen only turns the month', (t) async {
      await open(t);
      await t.sendKeyEvent(LogicalKeyboardKey.pageUp);
      await t.pumpAndSettle();
      expect(find.text('October 2026'), findsOneWidget);
      expect(
        t.widget<FilledButton>(find.byKey(const Key('when-done'))).onPressed,
        isNull,
      );
    });

    testWidgets('a day in another month; the month arrows work', (t) async {
      await open(t);
      await tapKey(t, 'when-next');
      expect(find.text('December 2026'), findsOneWidget);
      await tapKey(t, 'when-day-2026-12-24');
      await tapKey(t, 'when-done');
      expect(result?.text, '2026-12-24');
    });

    testWidgets('all-day by default, timed with a typed time', (t) async {
      await open(t);
      await tapKey(t, 'when-today');
      expect(find.byKey(const Key('when-time')), findsNothing);
      await tapKey(t, 'when-timed');
      expect(find.byKey(const Key('when-time')), findsOneWidget);
      await t.enterText(find.byKey(const Key('when-time')), '14:30');
      await t.pump();
      await tapKey(t, 'when-done');
      expect(result?.text, '2026-11-25 14:30');
    });

    testWidgets('an invalid time blocks Done; Enter in the time field saves', (
      t,
    ) async {
      await open(
        t,
        initial: When(
          DateTime(2026, 12, 1),
          const TimeOfDay(hour: 9, minute: 0),
        ),
      );
      final field = find.byKey(const Key('when-time'));
      await t.enterText(field, '25:00');
      await t.pump();
      expect(find.text('Type a time like 14:30.'), findsOneWidget);
      expect(
        t.widget<FilledButton>(find.byKey(const Key('when-done'))).onPressed,
        isNull,
      );
      await t.enterText(field, '0745');
      await t.testTextInput.receiveAction(TextInputAction.done);
      await t.pumpAndSettle();
      expect(result?.text, '2026-12-01 07:45');
    });

    testWidgets('switching the time off makes it all-day again', (t) async {
      await open(
        t,
        initial: When(
          DateTime(2026, 12, 1),
          const TimeOfDay(hour: 9, minute: 0),
        ),
      );
      await tapKey(t, 'when-timed');
      await tapKey(t, 'when-done');
      expect(result?.text, '2026-12-01');
    });

    testWidgets('Esc and Cancel dismiss without a result', (t) async {
      await open(t, initial: When(DateTime(2026, 12, 1)));
      await t.sendKeyEvent(LogicalKeyboardKey.escape);
      await t.pumpAndSettle();
      expect(closed, isTrue);
      expect(result, isNull);

      await open(t);
      await tapKey(t, 'when-cancel');
      expect(closed, isTrue);
      expect(result, isNull);
    });

    testWidgets('arrow keys move the day, Enter saves', (t) async {
      await open(t, initial: When(DateTime(2026, 12, 1)));
      await t.sendKeyEvent(LogicalKeyboardKey.arrowRight);
      await t.sendKeyEvent(LogicalKeyboardKey.arrowDown);
      await t.sendKeyEvent(LogicalKeyboardKey.enter);
      await t.pumpAndSettle();
      expect(result?.text, '2026-12-09');
    });

    testWidgets('a start after the due date is refused', (t) async {
      final due = WhenLimit(When(DateTime(2026, 12, 1)), 'due');
      await open(t, isStart: true, other: due);
      await tapKey(t, 'when-day-2026-11-30');
      expect(find.text('Start can\'t be after the due date.'), findsNothing);
      await tapKey(t, 'when-next');
      await tapKey(t, 'when-day-2026-12-02');
      expect(find.text('Start can\'t be after the due date.'), findsOneWidget);
      expect(
        t.widget<FilledButton>(find.byKey(const Key('when-done'))).onPressed,
        isNull,
      );
      await tapKey(t, 'when-day-2026-12-01');
      await tapKey(t, 'when-done');
      expect(result?.text, '2026-12-01');
    });

    testWidgets('a due before the start is refused', (t) async {
      final start = WhenLimit(When(DateTime(2026, 12, 3)), 'start');
      await open(t, other: start);
      await tapKey(t, 'when-next');
      await tapKey(t, 'when-day-2026-12-02');
      expect(find.text('Due can\'t be before the start date.'), findsOneWidget);
    });

    testWidgets('timed next to an all-day date warns that it will be cleared', (
      t,
    ) async {
      final start = WhenLimit(When(DateTime(2026, 12, 3)), 'start');
      await open(t, other: start, initial: When(DateTime(2026, 12, 5)));
      await tapKey(t, 'when-timed');
      expect(find.textContaining('will be cleared'), findsOneWidget);
      expect(
        t.widget<FilledButton>(find.byKey(const Key('when-done'))).onPressed,
        isNotNull,
      );
    });

    for (final b in Brightness.values) {
      testWidgets('phone sheet with 48 px targets, ${b.name} theme', (t) async {
        t.view.physicalSize = const Size(390, 844);
        t.view.devicePixelRatio = 1;
        addTearDown(t.view.reset);
        await open(t, platform: TargetPlatform.android, brightness: b);
        expect(find.byType(BottomSheet), findsOneWidget);
        expect(
          t.getSize(find.byKey(const Key('when-day-2026-11-25'))).height,
          48,
        );
        expect(
          t.getSize(find.byKey(const Key('when-day-2026-11-25'))).width,
          greaterThanOrEqualTo(48),
        );
        expect(
          t.getSize(find.byKey(const Key('when-done'))).height,
          greaterThanOrEqualTo(48),
        );
        expect(t.takeException(), isNull);
        await tapKey(t, 'when-tomorrow');
        await tapKey(t, 'when-done');
        expect(result?.text, '2026-11-26');
      });
    }

    testWidgets('desktop uses a dialog', (t) async {
      await open(t, brightness: Brightness.dark);
      expect(find.byType(Dialog), findsOneWidget);
      expect(t.takeException(), isNull);
    });
  });
}
