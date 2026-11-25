import 'dart:io';

// kalender is built on flutter/material; material_ui is a separate copy that it can't see.
// ignore: deprecated_member_use
import 'package:flutter/material.dart';
import 'package:flutter/gestures.dart' show PointerDeviceKind;
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:path_provider/path_provider.dart';
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

  Future<TaskDb> pumpCalendar(
    WidgetTester tester, {
    Size size = const Size(1400, 900),
    // Runs once the database file exists, before the app starts.
    void Function(String dbPath)? beforeLaunch,
  }) async {
    // On a phone the phone layout comes from the real screen (a forced 1:1 size would
    // leave no room once the on-screen keyboard's physical-pixel inset is applied).
    if (!(Platform.isAndroid && size.width < 760)) {
      tester.view.physicalSize = size;
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);
    }
    // Start from no saved view or Unscheduled source (calendar.json).
    final support = await getApplicationSupportDirectory();
    final prefs = File('${support.path}/calendar.json');
    if (prefs.existsSync()) prefs.deleteSync();
    final dir = await Directory.systemTemp.createTemp('tt-cal');
    final db = await TaskDb.open(path: '${dir.path}/tt.db');
    beforeLaunch?.call('${dir.path}/tt.db');
    final report = await db.add(title: 'Write report');
    await db.setStart(id: report.id, text: 'today 10:00');
    await db.add(title: 'Plan trip');
    await tester.pumpWidget(TickAltApp(db: db));
    await settle(tester);
    if (size.width < 760) {
      await tester.tap(find.byKey(const Key('open-drawer')));
      await settle(tester);
    }
    await tester.tap(find.text('Calendar'));
    await settle(tester);
    return db;
  }

  /// A drag the way the platform does it: a mouse drag on desktop, a long-press then
  /// move on Android (cards and blocks there use long-press dragging).
  Future<void> mouseDrag(WidgetTester tester, Offset from, Offset to) async {
    final touch = Platform.isAndroid;
    final g = await tester.startGesture(
      from,
      kind: touch ? PointerDeviceKind.touch : PointerDeviceKind.mouse,
    );
    if (touch) {
      for (var i = 0; i < 8; i++) {
        await tester.pump(const Duration(milliseconds: 100));
      }
    }
    await tester.pump(const Duration(milliseconds: 100));
    const steps = 12;
    for (var i = 1; i <= steps; i++) {
      await g.moveTo(Offset.lerp(from, to, i / steps)!);
      await tester.pump(const Duration(milliseconds: 16));
    }
    await g.up();
    await settle(tester);
  }

  testWidgets(
    'shows blocks and the Unscheduled panel; the sidebar folds to a rail',
    (tester) async {
      await pumpCalendar(tester);
      expect(find.text('Write report'), findsOneWidget);
      expect(find.text('Unscheduled'), findsOneWidget);
      expect(find.text('Plan trip'), findsOneWidget);
      expect(find.byKey(const Key('rail-unfold')), findsOneWidget);
    },
  );

  testWidgets('drag a task in from the panel, then drop it back', (
    tester,
  ) async {
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
    expect(
      placed.endAt - placed.startAt,
      30 * 60 * 1000,
      reason: 'default length',
    );
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

  testWidgets(
    'phone: Day view, week strip, drawer drag, two taps for detail, quick add',
    (tester) async {
      final db = await pumpCalendar(tester, size: const Size(412, 880));
      final now = DateTime.now();
      final day = DateTime(now.year, now.month, now.day);
      expect(find.byKey(Key('strip-${day.day}')), findsOneWidget);
      expect(find.byKey(const Key('cal-drawer')), findsOneWidget);
      expect(find.text('Write report'), findsOneWidget);

      // Open the drawer, then drag its card onto the day.
      await tester.tap(find.byKey(const Key('cal-drawer-handle')));
      await settle(tester);
      final trip = (await db.unscheduled()).single;
      final card = tester.getCenter(find.byKey(Key('unscheduled-${trip.id}')));
      final block = tester.getCenter(find.text('Write report'));
      await mouseDrag(tester, card, block + const Offset(0, 50));
      final items = await db.calendar(
        fromMs: day.millisecondsSinceEpoch,
        untilMs: day.add(const Duration(days: 1)).millisecondsSinceEpoch,
      );
      expect(items.where((i) => i.taskId == trip.id), hasLength(1));

      // The dropped block is selected; tapping it opens the detail page.
      await tester.tap(find.text('Plan trip'));
      await settle(tester);
      expect(find.byType(BackButton), findsOneWidget);
      await tester.tap(find.byType(BackButton));
      await settle(tester);

      // Quick add lands in Unscheduled.
      await tester.tap(find.byKey(const Key('cal-fab')));
      await settle(tester);
      await tester.enterText(
        find.byKey(const Key('cal-quick-add')),
        'Call mum',
      );
      // Submit from the keyboard; the on-screen keyboard can move the dialog's buttons.
      await tester.testTextInput.receiveAction(TextInputAction.done);
      await settle(tester);
      expect((await db.unscheduled()).map((r) => r.title), ['Call mum']);
    },
  );
  // M5b (plan/19): Google's events come from the daemon's cache. Here the cache is filled
  // with the sqlite3 CLI, as the daemon's fetch would, so no Google account is needed.
  testWidgets('Google events show read-only; hidden calendars stay hidden', (
    tester,
  ) async {
    // Google is desktop only, and the cache is filled with the sqlite3 CLI.
    if (Platform.isAndroid) {
      return;
    }
    final probe = await Process.run('sqlite3', [
      '-version',
    ]).catchError((_) => ProcessResult(0, 1, '', ''));
    if (probe.exitCode != 0) {
      return;
    }
    const local = "strftime('%s','now','localtime','start of day','utc')";
    final sql =
        '''
      INSERT INTO gcal_calendar (id, summary, color, selected) VALUES
        ('work@x', 'Work Blocks', '#16a765', 1), ('off@x', 'Hidden calendar', '#4986e7', 0);
      INSERT INTO gcal_event (calendar_id, id, summary, start_at, end_at, all_day, html_link) VALUES
        ('work@x', 'e1', 'Dentist', ($local + 13*3600) * 1000, ($local + 14*3600) * 1000, 0,
         'https://www.google.com/calendar/event?eid=e1'),
        ('work@x', 'e2', 'Mums birthday', strftime('%s', date('now','localtime')) * 1000,
         strftime('%s', date('now','localtime','+1 day')) * 1000, 1, NULL),
        ('off@x', 'h1', 'Secret meeting', ($local + 15*3600) * 1000, ($local + 16*3600) * 1000, 0, NULL);
    ''';
    final db = await pumpCalendar(
      tester,
      beforeLaunch: (path) {
        final r = Process.runSync('sqlite3', [path, sql]);
        expect(r.exitCode, 0, reason: '${r.stderr}');
      },
    );
    // A data change (the daemon's commit, here a write of ours) makes the page reload.
    await db.add(title: 'Nudge');
    await settle(tester);

    expect(find.text('Dentist'), findsOneWidget);
    expect(find.text('Mums birthday'), findsOneWidget);
    expect(find.text('Secret meeting'), findsNothing);
    expect(
      find.text('Write report'),
      findsOneWidget,
      reason: 'tasks still show',
    );

    // A tap opens the read-only details, not the task detail.
    await tester.tap(find.text('Dentist'));
    await settle(tester);
    expect(find.byKey(const Key('google-event-dialog')), findsOneWidget);
    expect(find.text('Work Blocks'), findsOneWidget);
    expect(find.byKey(const Key('google-event-copy-link')), findsOneWidget);
    await tester.tap(find.text('Close'));
    await settle(tester);
    expect(find.byKey(const Key('google-event-dialog')), findsNothing);

    // Dragging it does nothing: it stays where Google has it.
    final before = (await db.gcalEvents(
      fromMs: 0,
      untilMs: 1 << 50,
    )).map((e) => e.startAt).toList();
    await mouseDrag(
      tester,
      tester.getCenter(find.text('Dentist')),
      tester.getCenter(find.text('Dentist')) + const Offset(0, 120),
    );
    expect(
      (await db.gcalEvents(
        fromMs: 0,
        untilMs: 1 << 50,
      )).map((e) => e.startAt).toList(),
      before,
    );
  });
}
