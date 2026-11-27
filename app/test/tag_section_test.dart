// ignore: deprecated_member_use
import 'package:flutter/material.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:utre/desk/controller.dart';
import 'package:utre/desk/sidebar.dart';
import 'package:utre/desk/theme.dart';
import 'package:utre/src/rust/api/tasks.dart';

import 'golden/fake_db.dart';

/// Sidebar data with work · home (garden) · shed, and a record of tag writes.
class _TagDb extends FakeDb {
  final calls = <String>[];

  @override
  Future<Sidebar> sidebar() async {
    final s = sampleSidebar();
    return Sidebar(
      inbox: s.inbox,
      lists: s.lists,
      filters: s.filters,
      tags: const [
        NavItem(id: 'work', name: 'work', count: 1, depth: 0),
        NavItem(id: 'home', name: 'home', count: 1, depth: 0),
        NavItem(
          id: 'garden',
          name: 'garden',
          count: 1,
          depth: 1,
          parentId: 'home',
        ),
        NavItem(id: 'shed', name: 'shed', count: 1, depth: 0),
      ],
    );
  }

  @override
  Future<void> nestTag({
    required String id,
    String? parentId,
    String? before,
  }) async => calls.add('nest $id under $parentId before $before');

  @override
  Future<String> createTag({required String name, String? parentId}) async {
    calls.add('create $name under $parentId');
    return 'new';
  }

  @override
  Future<String> ensureTagPath({required String path}) async {
    calls.add('path $path');
    return 'new';
  }
}

void main() {
  late _TagDb db;
  late DeskController c;
  late Set<String> folded;

  Future<void> pump(WidgetTester t) async {
    await t.binding.setSurfaceSize(const Size(320, 900));
    db = _TagDb();
    c = DeskController(db);
    folded = {};
    await c.reload();
    await t.pumpWidget(
      MaterialApp(
        theme: softTheme(Brightness.light),
        home: StatefulBuilder(
          builder: (context, setState) => Scaffold(
            body: ListenableBuilder(
              listenable: c,
              builder: (_, _) => DeskSidebar(
                c: c,
                collapsedTags: folded,
                onCollapseTag: (id, f) =>
                    setState(() => f ? folded.add(id) : folded.remove(id)),
              ),
            ),
          ),
        ),
      ),
    );
    await t.pumpAndSettle();
  }

  /// A test with the sidebar pumped; the controller (and its midnight timer) goes after.
  void testTags(String name, Future<void> Function(WidgetTester t) body) =>
      testWidgets(name, (t) async {
        await pump(t);
        await body(t);
        await t.pumpWidget(const SizedBox());
        c.dispose();
      });

  Finder tag(String n) => find.byKey(Key('nav-tag-$n'));

  /// Drags `name`'s handle to `to` in small steps, then drops it.
  Future<void> drag(WidgetTester t, String name, Offset to) async {
    final g = await t.startGesture(
      t.getCenter(find.byKey(Key('drag-tags-$name'))),
    );
    final from = t.getCenter(find.byKey(Key('drag-tags-$name')));
    for (var i = 1; i <= 8; i++) {
      await g.moveTo(Offset.lerp(from, to, i / 8)!);
      await t.pump(const Duration(milliseconds: 16));
    }
    await g.up();
    await t.pumpAndSettle();
  }

  testTags('a parent folds its children away', (t) async {
    expect(tag('garden'), findsOneWidget);
    expect(find.byKey(const Key('fold-tag-work')), findsNothing);
    await t.tap(find.byKey(const Key('fold-tag-home')));
    await t.pumpAndSettle();
    expect(folded, {'home'});
    expect(tag('garden'), findsNothing);
    await t.tap(find.byKey(const Key('fold-tag-home')));
    await t.pumpAndSettle();
    expect(tag('garden'), findsOneWidget);
  });

  testTags('arranging shows folded tags; dropping on a tag nests it', (
    t,
  ) async {
    await t.tap(find.byKey(const Key('fold-tag-home')));
    await t.pumpAndSettle();
    await t.tap(find.byKey(const Key('arrange-tags')));
    await t.pumpAndSettle();
    expect(tag('garden'), findsOneWidget);

    // "shed" onto the middle of "work", at the handle's x (no level change).
    final x = t.getCenter(find.byKey(const Key('drag-tags-shed'))).dx;
    await drag(t, 'shed', Offset(x, t.getCenter(tag('work')).dy));
    expect(db.calls, ['nest shed under work before null']);
    // Shown nested at once; a folded parent it went into opens up.
    expect(
      t.getTopLeft(tag('shed')).dy,
      greaterThan(t.getTopLeft(tag('work')).dy),
    );
  });

  testTags('dragging left onto an edge moves a child up a level', (t) async {
    await t.tap(find.byKey(const Key('arrange-tags')));
    await t.pumpAndSettle();
    // "garden" to the top edge of "shed", one level left.
    final h = t.getCenter(find.byKey(const Key('drag-tags-garden')));
    final shed = t.getRect(tag('shed'));
    await drag(t, 'garden', Offset(h.dx - 30, shed.top + 3));
    expect(db.calls, ['nest garden under null before shed']);
  });

  testTags('a plain reorder on an edge stays a reorder', (t) async {
    await t.tap(find.byKey(const Key('arrange-tags')));
    await t.pumpAndSettle();
    final h = t.getCenter(find.byKey(const Key('drag-tags-shed')));
    await drag(t, 'shed', Offset(h.dx, t.getRect(tag('work')).top + 3));
    expect(db.calls, ['nest shed under null before work']);
  });

  testTags('Alt+Right nests under the tag above, Alt+Left moves out', (
    t,
  ) async {
    await t.tap(find.byKey(const Key('arrange-tags')));
    await t.pumpAndSettle();
    // The row's own focus (the InkWell's), found from its label.
    Focus.of(t.element(find.text('shed'))).requestFocus();
    await t.pump();
    await t.sendKeyDownEvent(LogicalKeyboardKey.altLeft);
    await t.sendKeyEvent(LogicalKeyboardKey.arrowRight);
    await t.sendKeyUpEvent(LogicalKeyboardKey.altLeft);
    await t.pumpAndSettle();
    expect(db.calls, ['nest shed under home before null']);
    // Shown under home at once; Alt+Left takes it back out, after home.
    await t.sendKeyDownEvent(LogicalKeyboardKey.altLeft);
    await t.sendKeyEvent(LogicalKeyboardKey.arrowLeft);
    await t.sendKeyUpEvent(LogicalKeyboardKey.altLeft);
    await t.pumpAndSettle();
    expect(db.calls.last, 'nest shed under null before null');
  });

  testTags('new tags: + takes a path, the dialog makes a sub-tag', (t) async {
    await t.tap(find.byKey(const Key('new-tag')));
    await t.pumpAndSettle();
    await t.enterText(find.byKey(const Key('ask-field')), ' trips / japan ');
    await t.tap(find.text('OK'));
    await t.pumpAndSettle();
    expect(db.calls, ['path trips/japan']);

    await t.tap(tag('home'), buttons: kSecondaryMouseButton);
    await t.pumpAndSettle();
    await t.tap(find.byKey(const Key('style-subtag')));
    await t.pumpAndSettle();
    expect(find.text('New tag under home'), findsOneWidget);
    await t.enterText(find.byKey(const Key('ask-field')), 'shed');
    await t.tap(find.text('OK'));
    await t.pumpAndSettle();
    expect(db.calls.last, 'create shed under home');
  });
}
