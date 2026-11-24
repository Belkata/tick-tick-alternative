import 'package:flutter/gestures.dart';
import 'package:flutter_test/flutter_test.dart';
// kalender is built on flutter/material; material_ui is a separate copy that it can't see.
// ignore: deprecated_member_use
import 'package:flutter/material.dart';
import 'package:tick_alt/calendar/spike_calendar.dart';

/// Spike 5 checks for the kalender-based calendar, at a phone-sized viewport.
void main() {
  Future<void> pumpCalendar(WidgetTester tester) async {
    tester.view.physicalSize = const Size(1080, 2400);
    tester.view.devicePixelRatio = 2.625;
    addTearDown(tester.view.reset);
    await tester.pumpWidget(const MaterialApp(home: SpikeCalendarPage()));
    await tester.pumpAndSettle();
  }

  /// Long-press at [from], wait for the drag to start, then move to [to] in steps and release.
  Future<void> longPressDrag(WidgetTester tester, Offset from, Offset to) async {
    final gesture = await tester.startGesture(from, kind: PointerDeviceKind.touch);
    await tester.pump(kLongPressTimeout + const Duration(milliseconds: 100));
    const steps = 20;
    for (var i = 1; i <= steps; i++) {
      await gesture.moveTo(Offset.lerp(from, to, i / steps)!);
      await tester.pump(const Duration(milliseconds: 16));
    }
    await gesture.up();
    await tester.pumpAndSettle();
  }

  /// Centre of today's column header (the day number), used as a column x position.
  double columnX(WidgetTester tester, DateTime day) =>
      tester.getCenter(find.text('${day.day}').first).dx;

  /// y of a timeline label such as "3:00 PM".
  double rowY(WidgetTester tester, String label) => tester.getCenter(find.text(label)).dy;

  testWidgets('shows sample tasks and read-only events', (tester) async {
    await pumpCalendar(tester);
    expect(find.text('Write spike log'), findsOneWidget);
    expect(find.text('Gym'), findsOneWidget);
    expect(find.text('Renew passport'), findsOneWidget);
  });

  testWidgets('long-press drag moves a task to another time', (tester) async {
    await pumpCalendar(tester);
    final before = tester.getTopLeft(find.text('Write spike log'));

    // Drop it 3 hours later in the same column.
    final target = Offset(before.dx + 20, before.dy + (rowY(tester, '1:00 PM') - rowY(tester, '10:00 AM')));
    await longPressDrag(tester, before + const Offset(20, 10), target);

    final after = tester.getTopLeft(find.text('Write spike log'));
    expect(after.dy, greaterThan(before.dy + 50), reason: 'tile should move down by ~3 hours');
  });

  testWidgets('read-only Google events cannot be dragged', (tester) async {
    await pumpCalendar(tester);
    final before = tester.getTopLeft(find.textContaining('Lunch'));
    await longPressDrag(tester, before + const Offset(10, 10), before + const Offset(10, 300));
    expect(tester.getTopLeft(find.textContaining('Lunch')), before);
    expect(find.text('New task'), findsNothing, reason: 'long-press must not fall through and create a task');
  });

  testWidgets('dragging an unscheduled task onto the grid schedules it', (tester) async {
    await pumpCalendar(tester);
    final chip = find.byKey(const Key('unscheduled-Book flights'));
    expect(chip, findsOneWidget);

    final today = DateTime.now();
    final drop = Offset(columnX(tester, today), rowY(tester, '3:00 PM'));
    await longPressDrag(tester, tester.getCenter(chip), drop);

    expect(chip, findsNothing, reason: 'chip leaves the unscheduled strip');
    expect(find.text('Book flights'), findsOneWidget, reason: 'now a tile on the grid');
    final tile = tester.getTopLeft(find.text('Book flights'));
    final neighbour = [today.subtract(const Duration(days: 1)), today.add(const Duration(days: 1))]
        .firstWhere((d) => find.text('${d.day}').evaluate().isNotEmpty);
    final colWidth = (columnX(tester, today) - columnX(tester, neighbour)).abs();
    expect(tile.dx, inInclusiveRange(drop.dx - colWidth / 2, drop.dx), reason: "lands in today's column");
    final hour = rowY(tester, '4:00 PM') - rowY(tester, '3:00 PM');
    expect((tile.dy - drop.dy).abs(), lessThan(hour / 2), reason: 'starts within half an hour of 3 PM');
  });

  testWidgets('view picker switches between Day, 3 days, Week and Month', (tester) async {
    await pumpCalendar(tester);
    for (final name in ['Day', 'Week', 'Month', '3 days']) {
      await tester.tap(find.byKey(const Key('view-picker')));
      await tester.pumpAndSettle();
      await tester.tap(find.text(name).last);
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull, reason: 'switching to $name');
    }
  });
}
