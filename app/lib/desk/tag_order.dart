import '../src/rust/api/tasks.dart';

/// Arranging tags in the sidebar. `tags` is the sidebar's tree order: each tag, then
/// the tags under it. A tag moves with its subtree and stays under the same parent,
/// so a drop elsewhere snaps to the nearest place among its siblings.

/// The parent a tag is shown under; a tag whose parent is gone shows at the top.
String? _parent(NavItem t) => t.depth == 0 ? null : t.parentId;

/// End (exclusive) of the subtree that starts at `i`.
int _subtreeEnd(List<NavItem> tags, int i) {
  var end = i + 1;
  while (end < tags.length && tags[end].depth > tags[i].depth) {
    end++;
  }
  return end;
}

/// Where the tag at `from` lands when dropped before index `to` (0..length, counted
/// before it was lifted out): the sibling to go before, or null for last. Null
/// overall when the order doesn't change.
({String id, String? before})? tagDrop(List<NavItem> tags, int from, int to) {
  if (from < 0 || from >= tags.length) return null;
  final t = tags[from];
  if (to >= from && to <= _subtreeEnd(tags, from)) return null;
  final siblings = [
    for (var i = 0; i < tags.length; i++)
      if (i != from &&
          tags[i].depth == t.depth &&
          _parent(tags[i]) == _parent(t))
        i,
  ];
  final before = siblings.where((i) => i >= to).firstOrNull;
  final nowBefore = siblings.where((i) => i > from).firstOrNull;
  if (before == nowBefore) return null;
  return (id: t.id, before: before == null ? null : tags[before].id);
}

/// `tags` with `id` (and its subtree) moved before sibling `before`, or after its last
/// sibling. Shown while the database catches up.
List<NavItem> applyTagMove(List<NavItem> tags, String id, String? before) {
  final from = tags.indexWhere((t) => t.id == id);
  if (from < 0) return tags;
  final t = tags[from];
  final block = tags.sublist(from, _subtreeEnd(tags, from));
  final rest = [...tags.sublist(0, from), ...tags.sublist(from + block.length)];
  int at;
  if (before != null) {
    at = rest.indexWhere((x) => x.id == before);
    if (at < 0) return tags;
  } else {
    final last = rest.lastIndexWhere(
      (x) => x.depth == t.depth && _parent(x) == _parent(t),
    );
    if (last < 0) return tags;
    at = _subtreeEnd(rest, last);
  }
  return [...rest.sublist(0, at), ...block, ...rest.sublist(at)];
}
