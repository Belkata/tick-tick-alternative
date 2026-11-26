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
}
