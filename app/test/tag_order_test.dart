import 'package:flutter_test/flutter_test.dart';
import 'package:utre/desk/tag_order.dart';
import 'package:utre/src/rust/api/tasks.dart';

NavItem tag(String id, {String? parent, int depth = 0}) =>
    NavItem(id: id, name: id, count: 0, depth: depth, parentId: parent);

/// work · home (garden, kitchen) · shed
final tags = [
  tag('work'),
  tag('home'),
  tag('garden', parent: 'home', depth: 1),
  tag('kitchen', parent: 'home', depth: 1),
  tag('shed'),
];

List<String> ids(List<NavItem> t) => [for (final x in t) x.id];

void main() {
  test('a top-level tag moves with its children', () {
    // Drag "home" (1) above "work" (0).
    final m = tagDrop(tags, 1, 0)!;
    expect((m.id, m.before), ('home', 'work'));
    expect(ids(applyTagMove(tags, m.id, m.before)), [
      'home',
      'garden',
      'kitchen',
      'work',
      'shed',
    ]);
  });

  test('dropping at the end places it last', () {
    final m = tagDrop(tags, 0, tags.length)!;
    expect((m.id, m.before), ('work', null));
    expect(ids(applyTagMove(tags, m.id, m.before)), [
      'home',
      'garden',
      'kitchen',
      'shed',
      'work',
    ]);
  });

  test('a drop inside another subtree snaps to the next sibling', () {
    // "work" dropped between garden and kitchen: lands before "shed".
    final m = tagDrop(tags, 0, 3)!;
    expect((m.id, m.before), ('work', 'shed'));
  });

  test('a child stays under its parent', () {
    // "kitchen" (3) dropped at the very top: before "garden".
    final m = tagDrop(tags, 3, 0)!;
    expect((m.id, m.before), ('kitchen', 'garden'));
    expect(ids(applyTagMove(tags, m.id, m.before)), [
      'work',
      'home',
      'kitchen',
      'garden',
      'shed',
    ]);
    // "garden" (2) dropped at the bottom: last among home's children.
    final n = tagDrop(tags, 2, tags.length)!;
    expect((n.id, n.before), ('garden', null));
    expect(ids(applyTagMove(tags, n.id, n.before)), [
      'work',
      'home',
      'kitchen',
      'garden',
      'shed',
    ]);
  });

  test('drops that change nothing are ignored', () {
    expect(tagDrop(tags, 1, 1), isNull, reason: 'same place');
    expect(tagDrop(tags, 1, 3), isNull, reason: 'inside its own subtree');
    expect(tagDrop(tags, 1, 4), isNull, reason: 'right after its subtree');
    expect(tagDrop(tags, 3, tags.length), isNull, reason: 'already last child');
  });

  test('tagStep moves one place among siblings', () {
    // home (1) up goes before work (0); down goes after shed, past its subtree.
    expect(tagStep(tags, 1, -1), 0);
    expect(tagStep(tags, 1, 1), 5);
    expect(tagStep(tags, 0, -1), isNull);
    expect(tagStep(tags, 4, 1), isNull);
    expect(tagStep(tags, 3, -1), 2);
    expect(tagStep(tags, 3, 1), isNull);
  });

  group('nesting', () {
    String? parentOf(List<NavItem> t, String id) =>
        t.firstWhere((x) => x.id == id).parentId;
    int depthOf(List<NavItem> t, String id) =>
        t.firstWhere((x) => x.id == id).depth;

    test('dropping onto a tag nests it last, with its subtree', () {
      // "home" (1) onto "shed" (4).
      final n = tagNestDrop(tags, 1, 4)!;
      expect(n, (id: 'home', parent: 'shed', before: null));
      final after = applyTagNest(tags, n);
      expect(ids(after), ['work', 'shed', 'home', 'garden', 'kitchen']);
      expect(depthOf(after, 'home'), 1);
      expect(depthOf(after, 'kitchen'), 2);
      expect(parentOf(after, 'garden'), 'home');
      // "work" onto "garden": two levels down.
      final w = applyTagNest(tags, tagNestDrop(tags, 0, 2)!);
      expect(ids(w), ['home', 'garden', 'work', 'kitchen', 'shed']);
      expect(depthOf(w, 'work'), 2);
    });

    test(
      'not onto itself, its own subtree, or its parent when already last',
      () {
        expect(tagNestDrop(tags, 1, 1), isNull);
        expect(tagNestDrop(tags, 1, 2), isNull);
        expect(
          tagNestDrop(tags, 3, 1),
          isNull,
          reason: 'kitchen is last in home',
        );
        expect(tagNestDrop(tags, 2, 1), isNotNull, reason: 'garden goes last');
      },
    );

    test(
      'a gap offers depths from the row below to one under the row above',
      () {
        // Dragging "shed" (4). Gap 4 sits between kitchen (1) and shed itself.
        expect(tagGapDepths(tags, 4, 4), (min: 0, max: 2));
        // Gap 3, between garden and kitchen: only depth 1 (or under garden).
        expect(tagGapDepths(tags, 4, 3), (min: 1, max: 2));
        // Gap 0: the top.
        expect(tagGapDepths(tags, 4, 0), (min: 0, max: 0));
        // Inside the dragged subtree.
        expect(tagGapDepths(tags, 1, 3), isNull);
      },
    );

    test('dragging left of a child moves it up a level', () {
      // "kitchen" (3) into the gap below itself (gap 4, before shed) at depth 0.
      final n = tagGapDrop(tags, 3, 4, 0)!;
      expect(n, (id: 'kitchen', parent: null, before: 'shed'));
      final after = applyTagNest(tags, n);
      expect(ids(after), ['work', 'home', 'garden', 'kitchen', 'shed']);
      expect(depthOf(after, 'kitchen'), 0);
      // Depth 1 at the same gap is where it is now.
      expect(tagGapDrop(tags, 3, 4, 1), isNull);
    });

    test('a gap between children takes the dragged tag in at that level', () {
      // "shed" (4) between garden and kitchen; depth 0 clamps to 1.
      final n = tagGapDrop(tags, 4, 3, 0)!;
      expect(n, (id: 'shed', parent: 'home', before: 'kitchen'));
      // Depth 2 there: last under garden.
      expect(tagGapDrop(tags, 4, 3, 2), (
        id: 'shed',
        parent: 'garden',
        before: null,
      ));
      // A top-level gap behaves like a plain reorder.
      expect(tagGapDrop(tags, 4, 0, 0), (
        id: 'shed',
        parent: null,
        before: 'work',
      ));
    });

    test('Alt+Right nests under the sibling above, Alt+Left moves out', () {
      expect(tagIndent(tags, 4), (id: 'shed', parent: 'home', before: null));
      expect(tagIndent(tags, 0), isNull, reason: 'no sibling above');
      expect(tagIndent(tags, 2), isNull, reason: 'first child');
      expect(tagIndent(tags, 3), (
        id: 'kitchen',
        parent: 'garden',
        before: null,
      ));
      expect(tagOutdent(tags, 2), (id: 'garden', parent: null, before: 'shed'));
      expect(tagOutdent(tags, 0), isNull);
    });

    test('collapsed tags hide what is under them', () {
      expect(ids(visibleTags(tags, {'home'})), ['work', 'home', 'shed']);
      expect(ids(visibleTags(tags, {'garden'})), ids(tags));
      expect(tagHasChildren(tags, 1), isTrue);
      expect(tagHasChildren(tags, 2), isFalse);
      expect(tagHasChildren(tags, 4), isFalse);
    });
  });
}
