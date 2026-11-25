import 'dart:io';

// kalender is built on flutter/material; material_ui is a separate copy that it can't see.
// ignore: deprecated_member_use
import 'package:flutter/material.dart';
import 'package:flutter/gestures.dart' show kSecondaryMouseButton;
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:tick_alt/desk/detail.dart';
import 'package:tick_alt/desk/task_list.dart';
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

  testWidgets('a core error shows a message instead of crashing', (
    tester,
  ) async {
    // Regression: an error capturing a Rust backtrace crashed the app, because the
    // unwinder bundled in libflutter_linux_gtk.so clashes with libgcc_s
    // (the Linux runner now sets RUST_BACKTRACE=0).
    await pumpApp(tester);
    await addTask(tester, 'Pick a date');
    await tester.tap(find.text('Pick a date'));
    await settle(tester);
    await tester.enterText(find.byKey(const Key('detail-due')), 'someday-ish');
    await tester.testTextInput.receiveAction(TextInputAction.done);
    await settle(tester);
    expect(find.byType(SnackBar), findsOneWidget);
    expect(find.text('Pick a date'), findsWidgets);
  });

  testWidgets(
    'filter builder: create with a live count, then edit and delete',
    (tester) async {
      final db = await pumpApp(tester);
      await addTask(tester, 'Mow the lawn');
      await addTask(tester, 'Pay rent');
      final mow = (await db.inbox()).firstWhere(
        (t) => t.title == 'Mow the lawn',
      );
      await db.tag(id: mow.id, path: 'home');
      await settle(tester);

      await tester.tap(find.byKey(const Key('new-filter')));
      await settle(tester);
      await tester.enterText(
        find.byKey(const Key('filter-name')),
        'Home stuff',
      );
      await tester.enterText(find.byKey(const Key('cond-value')), 'home');
      await settle(tester);
      expect(find.text('1 open task matches'), findsOneWidget);

      await tester.tap(find.byKey(const Key('filter-save')));
      await settle(tester);
      expect(find.byKey(const Key('nav-filter-Home stuff')), findsOneWidget);
      expect(
        find.text('Mow the lawn'),
        findsWidgets,
      ); // the filter's view is open
      expect(find.text('Pay rent'), findsNothing);

      // Right-click edits; Delete (confirmed) removes it.
      await tester.tap(
        find.byKey(const Key('nav-filter-Home stuff')),
        buttons: kSecondaryMouseButton,
      );
      await settle(tester);
      expect(find.text('Edit filter'), findsOneWidget);
      await tester.tap(find.byKey(const Key('filter-delete')));
      await settle(tester);
      await tester.tap(find.byKey(const Key('confirm')));
      await settle(tester);
      expect(find.byKey(const Key('nav-filter-Home stuff')), findsNothing);
    },
  );

  testWidgets('right-click a tag to give it a colour', (tester) async {
    final db = await pumpApp(tester);
    await addTask(tester, 'Plant bulbs');
    await db.tag(id: (await db.inbox()).single.id, path: 'garden');
    await settle(tester);
    await tester.tap(
      find.byKey(const Key('nav-tag-garden')),
      buttons: kSecondaryMouseButton,
    );
    await settle(tester);
    await tester.tap(find.byKey(const Key('swatch-#3fae5a')));
    await tester.tap(find.byKey(const Key('style-save')));
    await settle(tester);
    final tag = (await db.sidebar()).tags.single;
    expect(tag.color, '#3fae5a');
  });

  testWidgets('arrange tags by dragging them in the sidebar', (tester) async {
    final db = await pumpApp(tester);
    await addTask(tester, 'Plant bulbs');
    final id = (await db.inbox()).single.id;
    await db.tag(id: id, path: 'garden');
    await db.tag(id: id, path: 'work');
    await settle(tester);
    Future<List<String>> names() async => [
      for (final t in (await db.sidebar()).tags) t.name,
    ];
    expect(await names(), ['garden', 'work']);

    await tester.tap(find.byKey(const Key('arrange-tags')));
    await settle(tester);
    final above = tester.getTopLeft(find.byKey(const Key('nav-tag-garden')));
    final g = await tester.startGesture(
      tester.getCenter(find.byKey(const Key('drag-tag-work'))),
    );
    for (var i = 0; i < 10; i++) {
      await g.moveBy(const Offset(0, -6));
      await tester.pump(const Duration(milliseconds: 16));
    }
    await g.moveTo(Offset(above.dx + 40, above.dy - 4));
    await tester.pump(const Duration(milliseconds: 100));
    await g.up();
    await settle(tester);
    expect(await names(), ['work', 'garden']);

    // Done leaves arranging; the order stays.
    await tester.tap(find.byKey(const Key('arrange-tags')));
    await settle(tester);
    expect(find.byKey(const Key('drag-tag-work')), findsNothing);
    expect(
      tester.getTopLeft(find.byKey(const Key('nav-tag-work'))).dy,
      lessThan(tester.getTopLeft(find.byKey(const Key('nav-tag-garden'))).dy),
    );
  });

  testWidgets('one sort for every view, then drag in manual order', (
    tester,
  ) async {
    final db = await pumpApp(tester);
    final list = await db.createList(name: 'Errands');
    await settle(tester);
    await tester.tap(find.text('Errands'));
    await settle(tester);
    for (final t in ['Bank', 'Cobbler', 'Apples']) {
      await addTask(tester, t);
    }
    final view = TaskView(kind: ViewKind.list, id: list);
    Future<List<String>> titles() async => [
      for (final r in await db.tasks(view: view)) r.title,
    ];
    double y(String title) => tester.getTopLeft(find.text(title)).dy;

    await tester.tap(find.byKey(const Key('sort-menu')));
    await settle(tester);
    await tester.tap(find.byKey(const Key('sort-title')));
    await settle(tester);
    expect(await db.viewSort(view: view), ['title_asc']);
    expect(await titles(), ['Apples', 'Bank', 'Cobbler']);
    expect(y('Apples'), lessThan(y('Bank')));
    String label() =>
        tester.widget<Text>(find.byKey(const Key('sort-label'))).data!;
    expect(label(), 'Title');
    final buttonAt = tester.getTopRight(find.byKey(const Key('sort-menu')));

    // The choice holds in every view, and when coming back.
    await tester.tap(find.text('Inbox').first);
    await settle(tester);
    expect(label(), 'Title');
    expect(
      tester.getTopRight(find.byKey(const Key('sort-menu'))),
      buttonAt,
      reason: 'the button stays put whatever the title',
    );
    expect(await db.viewSort(view: const TaskView(kind: ViewKind.inbox)), [
      'title_asc',
    ]);
    await tester.tap(find.byKey(const Key('sort-menu')));
    await settle(tester);
    expect(
      find.descendant(
        of: find.byKey(const Key('sort-title')),
        matching: find.byIcon(Icons.check),
      ),
      findsOneWidget,
    );
    await tester.tapAt(Offset.zero);
    await settle(tester);
    await tester.tap(find.text('Errands'));
    await settle(tester);
    expect(y('Apples'), lessThan(y('Bank')));

    await tester.tap(find.byKey(const Key('sort-menu')));
    await settle(tester);
    await tester.tap(find.byKey(const Key('sort-manual')));
    await settle(tester);
    expect(await titles(), [
      'Apples',
      'Cobbler',
      'Bank',
    ], reason: 'newest first');

    // Drag Bank to the top, in small steps so the list keeps up.
    final target = tester.getTopLeft(find.text('Apples')).dy - 20;
    final g = await tester.startGesture(tester.getCenter(find.text('Bank')));
    var at = tester.getCenter(find.text('Bank')).dy;
    while (at > target) {
      await g.moveBy(const Offset(0, -6));
      at -= 6;
      await tester.pump(const Duration(milliseconds: 16));
    }
    await tester.pump(const Duration(milliseconds: 300));
    await g.up();
    await settle(tester);
    expect(await titles(), ['Bank', 'Apples', 'Cobbler']);
    expect(y('Bank'), lessThan(y('Apples')));

    // A click still opens the task.
    await tester.tap(find.text('Cobbler'));
    await settle(tester);
    expect(find.byType(DetailPane), findsOneWidget);
  });

  testWidgets('drag the line before the detail pane to resize it', (
    tester,
  ) async {
    await pumpApp(tester);
    await addTask(tester, 'Wide notes');
    await tester.tap(find.text('Wide notes'));
    await settle(tester);
    double width() => tester.getSize(find.byType(DetailPane)).width;
    Future<void> reset() async {
      // Double-click: back to the default, which also leaves desk.json at it.
      await tester.tap(find.byKey(const Key('detail-resize')));
      await tester.pump(const Duration(milliseconds: 50));
      await tester.tap(find.byKey(const Key('detail-resize')));
      await settle(tester);
    }

    await reset();
    final before = width();
    expect(before, closeTo(340, 1));
    await tester.drag(
      find.byKey(const Key('detail-resize')),
      const Offset(-120, 0),
    );
    await settle(tester);
    expect(width(), closeTo(before + 120, 1));
    // The task list keeps its minimum: a huge drag stops short.
    await tester.drag(
      find.byKey(const Key('detail-resize')),
      const Offset(-2000, 0),
    );
    await settle(tester);
    expect(
      tester.getSize(find.byType(TaskListPane)).width,
      greaterThanOrEqualTo(299),
    );
    await reset();
    expect(width(), closeTo(340, 1));
  });

  testWidgets('pick a parent task, then go up to it', (tester) async {
    final db = await pumpApp(tester);
    await addTask(tester, 'Plan trip');
    await addTask(tester, 'Book hotel');
    final inbox = await db.inbox();
    final hotel = inbox.firstWhere((t) => t.title == 'Book hotel');
    final trip = inbox.firstWhere((t) => t.title == 'Plan trip');
    await tester.tap(find.text('Book hotel'));
    await settle(tester);
    expect(find.byKey(const Key('detail-up')), findsNothing);

    await tester.tap(find.byKey(const Key('detail-parent')));
    await settle(tester);
    await tester.enterText(find.byKey(const Key('parent-search')), 'plan');
    await settle(tester);
    await tester.tap(find.byKey(const Key('parent-Plan trip')));
    await settle(tester);
    expect((await db.task(id: hotel.id))!.parentTitle, 'Plan trip');

    // Up the tree: the parent opens, listing the subtask.
    await tester.tap(find.byKey(const Key('detail-up')));
    await settle(tester);
    final d = (await db.task(id: trip.id))!;
    expect(d.subtasks.single.title, 'Book hotel');
    expect(find.byKey(const Key('detail-up')), findsNothing);

    // Back down, then make it top-level again.
    await tester.tap(
      find.descendant(
        of: find.byType(DetailPane),
        matching: find.text('Book hotel'),
      ),
    );
    await settle(tester);
    await tester.tap(find.byKey(const Key('detail-parent-clear')));
    await settle(tester);
    expect((await db.task(id: hotel.id))!.row.parentId, isNull);
  });

  testWidgets('Show in Calendar off keeps a task off the calendar', (
    tester,
  ) async {
    final db = await pumpApp(tester);
    await addTask(tester, 'Secret plan');
    final id = (await db.inbox()).single.id;
    await tester.tap(find.text('Secret plan'));
    await settle(tester);
    // No date, so it can't be on the calendar: no switch yet.
    expect(find.byKey(const Key('detail-in-calendar')), findsNothing);

    await tester.enterText(find.byKey(const Key('detail-due')), 'tomorrow');
    await tester.testTextInput.receiveAction(TextInputAction.done);
    await settle(tester);
    final now = DateTime.now();
    Future<int> blocks() async => (await db.calendar(
      fromMs: now.millisecondsSinceEpoch,
      untilMs: now.add(const Duration(days: 3)).millisecondsSinceEpoch,
    )).where((b) => b.taskId == id).length;
    expect(await blocks(), 1);

    await tester.tap(find.byKey(const Key('detail-in-calendar')));
    await settle(tester);
    expect((await db.task(id: id))!.inCalendar, isFalse);
    expect(await blocks(), 0);
    expect(find.text('Kept off the calendar'), findsOneWidget);
  });

  testWidgets('detail: start date, repeat and reminders', (tester) async {
    final db = await pumpApp(tester);
    await addTask(tester, 'Water plants');
    final id = (await db.inbox()).single.id;
    await tester.tap(find.text('Water plants'));
    await settle(tester);

    await tester.enterText(find.byKey(const Key('detail-due')), 'tomorrow');
    await tester.testTextInput.receiveAction(TextInputAction.done);
    await settle(tester);
    await tester.enterText(find.byKey(const Key('detail-start')), 'today');
    await tester.testTextInput.receiveAction(TextInputAction.done);
    await settle(tester);
    var d = (await db.task(id: id))!;
    expect(d.startLabel, 'today');
    expect(d.row.dueLabel, 'tomorrow');

    await tester.tap(find.byKey(const Key('detail-repeat')));
    await settle(tester);
    await tester.tap(find.text('Every day').last);
    await settle(tester);
    await tester.tap(find.byKey(const Key('repeat-after')));
    await settle(tester);
    d = (await db.task(id: id))!;
    expect(d.repeatRule, 'FREQ=DAILY');
    expect(d.repeatAfterCompletion, isTrue);

    await tester.tap(find.byKey(const Key('add-reminder')));
    await settle(tester);
    await tester.tap(find.text('15 minutes before'));
    await settle(tester);
    expect(find.text('15m before'), findsOneWidget);
    expect((await db.task(id: id))!.reminders.length, 1);
    await tester.tap(
      find.descendant(
        of: find
            .ancestor(of: find.text('15m before'), matching: find.byType(Row))
            .first,
        matching: find.byIcon(Icons.close),
      ),
    );
    await settle(tester);
    expect((await db.task(id: id))!.reminders, isEmpty);
  });

  testWidgets('Devices page explains that sync needs utre-daemon', (
    tester,
  ) async {
    tester.view.physicalSize = const Size(1280, 800);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    final dir = await Directory.systemTemp.createTemp('tt');
    final db = await TaskDb.open(path: '${dir.path}/tt.db');
    // Desktop: no spike sync node, so the sidebar offers Devices.
    await tester.pumpWidget(TickAltApp(db: db));
    await settle(tester);
    await tester.tap(find.text('Devices'));
    await settle(tester);
    expect(find.textContaining("isn't running"), findsOneWidget);
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
