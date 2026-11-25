import 'dart:io';

// kalender is built on flutter/material; material_ui is a separate copy that it can't see.
// ignore: deprecated_member_use
import 'package:flutter/material.dart';
import 'package:flutter/gestures.dart' show PointerDeviceKind;
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:tick_alt/main.dart';
import 'package:tick_alt/src/rust/api/tasks.dart';
import 'package:tick_alt/src/rust/frb_generated.dart';

/// The M5 calendar (plan/18) through the real core: blocks, dragging in from the
/// Unscheduled panel, dropping back, selection and the view switch.
void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  setUpAll(() async => await RustLib.init());

  Future<void> settle(WidgetTester tester) async {
    for (var i = 0; i < 8; i++) {
      await tester.pump(const Duration(milliseconds: 50));
    }
    await tester.pumpAndSettle();
  }

  Future<TaskDb> pumpCalendar(WidgetTester tester) async {
    tester.view.physicalSize = const Size(1400, 900);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    final dir = await Directory.systemTemp.createTemp('tt-cal');
    final db = await TaskDb.open(path: '${dir.path}/tt.db');
    final report = await db.add(title: 'Write report');
    await db.setStart(id: report.id, text: 'today 10:00');
    await db.add(title: 'Plan trip');
    await tester.pumpWidget(TickAltApp(db: db));
    await settle(tester);
    await tester.tap(find.text('Calendar'));
    await settle(tester);
    return db;
  }

  Future<void> mouseDrag(WidgetTester tester, Offset from, Offset to) async {
    final g = await tester.startGesture(from, kind: PointerDeviceKind.mouse);
    await tester.pump(const Duration(milliseconds: 100));
    const steps = 12;
    for (var i = 1; i <= steps; i++) {
      await g.moveTo(Offset.lerp(from, to, i / steps)!);
      await tester.pump(const Duration(milliseconds: 16));
    }
    await g.up();
    await settle(tester);
  }

  testWidgets('shows blocks and the Unscheduled panel; the sidebar folds to a rail', (
    tester,
  ) async {
    await pumpCalendar(tester);
    expect(find.text('Write report'), findsOneWidget);
    expect(find.text('Unscheduled'), findsOneWidget);
    expect(find.text('Plan trip'), findsOneWidget);
    expect(find.byKey(const Key('rail-unfold')), findsOneWidget);
  });

  testWidgets('drag a task in from the panel, then drop it back', (tester) async {
    final db = await pumpCalendar(tester);
    final rows = await db.unscheduled();
    final trip = rows.single;

    // Just below the 10:00 block, in the same day column (well away from the edges,
    // where kalender scrolls or pages).
    final block = tester.getCenter(find.text('Write report'));
    final card = tester.getCenter(find.byKey(Key('unscheduled-${trip.id}')));
    await mouseDrag(tester, card, block + const Offset(0, 50));

    final now = DateTime.now();
    final day = DateTime(now.year, now.month, now.day);
    var items = await db.calendar(
      fromMs: day.millisecondsSinceEpoch,
      untilMs: day.add(const Duration(days: 1)).millisecondsSinceEpoch,
    );
    final placed = items.where((i) => i.taskId == trip.id).single;
    expect(placed.endAt - placed.startAt, 30 * 60 * 1000, reason: 'default length');
    expect(await db.unscheduled(), isEmpty);
    // The dropped task opens in the detail pane.
    expect(find.byKey(const Key('cal-back-to-unscheduled')), findsOneWidget);

    await tester.tap(find.byKey(const Key('cal-back-to-unscheduled')));
    await settle(tester);
    final tile = tester.getCenter(find.text('Plan trip'));
    final panel = tester.getCenter(find.text('Unscheduled'));
    await mouseDrag(tester, tile, panel + const Offset(0, 200));

    expect((await db.unscheduled()).map((r) => r.id), [trip.id]);
    items = await db.calendar(
      fromMs: day.millisecondsSinceEpoch,
      untilMs: day.add(const Duration(days: 1)).millisecondsSinceEpoch,
    );
    expect(items.where((i) => i.taskId == trip.id), isEmpty);
  });

  testWidgets('tapping a block opens its detail; Month still shows it', (
    tester,
  ) async {
    await pumpCalendar(tester);
    await tester.tap(find.text('Write report'));
    await settle(tester);
    expect(find.byKey(const Key('cal-back-to-unscheduled')), findsOneWidget);

    await tester.tap(find.byKey(const Key('view-Month')));
    await settle(tester);
    expect(find.text('Write report'), findsWidgets);
  });
}
