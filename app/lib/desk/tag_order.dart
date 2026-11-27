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

/// The drop index (see [tagDrop]) that moves the item at `i` one place up (`dir` -1) or
/// down (+1) among its siblings, or null at the end. For keyboard and screen readers.
int? tagStep(List<NavItem> tags, int i, int dir) {
  if (i < 0 || i >= tags.length) return null;
  final t = tags[i];
  bool sibling(int j) =>
      tags[j].depth == t.depth && _parent(tags[j]) == _parent(t);
  if (dir < 0) {
    for (var j = i - 1; j >= 0; j--) {
      if (sibling(j)) return j;
    }
    return null;
  }
  for (var j = _subtreeEnd(tags, i); j < tags.length; j++) {
    if (sibling(j)) return _subtreeEnd(tags, j);
  }
  return null;
}

// ----- Nesting: dropping a tag onto another, or into a gap at a chosen depth -----

/// Where a dragged tag lands: under `parent` (null for the top), right before its new
/// sibling `before`, or last with null.
typedef TagNest = ({String id, String? parent, String? before});

/// `tags` without the subtree that starts at `from`: what a drop is measured against.
List<NavItem> _rest(List<NavItem> tags, int from) => [
  ...tags.sublist(0, from),
  ...tags.sublist(_subtreeEnd(tags, from)),
];

/// The sibling that comes after the tag at `i`, or null when it is last.
String? _nextSibling(List<NavItem> tags, int i) {
  final t = tags[i];
  for (var j = _subtreeEnd(tags, i); j < tags.length; j++) {
    if (tags[j].depth < t.depth) return null;
    if (tags[j].depth == t.depth && _parent(tags[j]) == _parent(t)) {
      return tags[j].id;
    }
  }
  return null;
}

/// Null when `n` leaves the tag at `from` where it is.
TagNest? _changed(List<NavItem> tags, int from, TagNest n) {
  final t = tags[from];
  if (n.parent == _parent(t) && n.before == _nextSibling(tags, from)) {
    return null;
  }
  return n;
}

/// The depths a tag dropped into gap `gap` of `tags` (0..length, before the tag at
/// `from` was lifted out) can take: from the next row's depth (it can't leave that row
/// without a parent) to one deeper than the row above (its first or last child).
/// Null when the gap is inside the dragged subtree.
({int min, int max})? tagGapDepths(List<NavItem> tags, int from, int gap) {
  final end = _subtreeEnd(tags, from);
  if (gap > from && gap < end) return null;
  final rest = _rest(tags, from);
  final g = gap >= end ? gap - (end - from) : gap;
  final min = g < rest.length ? rest[g].depth : 0;
  final max = g > 0 ? rest[g - 1].depth + 1 : 0;
  return (min: min, max: max < min ? min : max);
}

/// The tag at `from` dropped into gap `gap` (see [tagGapDepths]) at `depth`, clamped
/// to what fits there. Null when nothing changes.
TagNest? tagGapDrop(List<NavItem> tags, int from, int gap, int depth) {
  final range = tagGapDepths(tags, from, gap);
  if (range == null) return null;
  final end = _subtreeEnd(tags, from);
  final rest = _rest(tags, from);
  final g = gap >= end ? gap - (end - from) : gap;
  final d = depth.clamp(range.min, range.max);
  String? parent;
  if (d > 0) {
    for (var j = g - 1; j >= 0; j--) {
      if (rest[j].depth == d - 1) {
        parent = rest[j].id;
        break;
      }
    }
  }
  final before = g < rest.length && rest[g].depth == d ? rest[g].id : null;
  return _changed(tags, from, (
    id: tags[from].id,
    parent: parent,
    before: before,
  ));
}

/// The tag at `from` dropped onto the tag at `onto`: it goes last under it. Null for a
/// tag inside the dragged subtree, or when nothing changes.
TagNest? tagNestDrop(List<NavItem> tags, int from, int onto) {
  if (onto < 0 || onto >= tags.length) return null;
  if (onto >= from && onto < _subtreeEnd(tags, from)) return null;
  return _changed(tags, from, (
    id: tags[from].id,
    parent: tags[onto].id,
    before: null,
  ));
}

/// Alt+Right: the tag at `i` goes last under the sibling above it.
TagNest? tagIndent(List<NavItem> tags, int i) {
  final t = tags[i];
  for (var j = i - 1; j >= 0; j--) {
    if (tags[j].depth < t.depth) return null;
    if (tags[j].depth == t.depth) return tagNestDrop(tags, i, j);
  }
  return null;
}

/// Alt+Left: the tag at `i` leaves its parent and goes right after it.
TagNest? tagOutdent(List<NavItem> tags, int i) {
  final t = tags[i];
  if (t.depth == 0) return null;
  for (var j = i - 1; j >= 0; j--) {
    if (tags[j].depth == t.depth - 1) {
      return _changed(tags, i, (
        id: t.id,
        parent: _parent(tags[j]),
        before: _nextSibling(tags, j),
      ));
    }
  }
  return null;
}

/// `tags` with the move applied (the subtree re-indented), shown while the database
/// catches up.
List<NavItem> applyTagNest(List<NavItem> tags, TagNest n) {
  final from = tags.indexWhere((t) => t.id == n.id);
  if (from < 0) return tags;
  final end = _subtreeEnd(tags, from);
  final rest = _rest(tags, from);
  final p = n.parent == null ? -1 : rest.indexWhere((t) => t.id == n.parent);
  if (n.parent != null && p < 0) return tags;
  final depth = p < 0 ? 0 : rest[p].depth + 1;
  final shift = depth - tags[from].depth;
  final block = [
    for (var i = from; i < end; i++)
      NavItem(
        id: tags[i].id,
        name: tags[i].name,
        color: tags[i].color,
        icon: tags[i].icon,
        count: tags[i].count,
        depth: tags[i].depth + shift,
        parentId: i == from ? n.parent : tags[i].parentId,
      ),
  ];
  int at;
  if (n.before != null) {
    at = rest.indexWhere((t) => t.id == n.before);
    if (at < 0) return tags;
  } else if (p >= 0) {
    at = _subtreeEnd(rest, p);
  } else {
    at = rest.length;
  }
  return [...rest.sublist(0, at), ...block, ...rest.sublist(at)];
}

/// Whether the tag at `i` is the one at `root` or under it.
bool tagInSubtree(List<NavItem> tags, int root, int i) =>
    i >= root && i < _subtreeEnd(tags, root);

/// Whether the tag at `i` has tags under it.
bool tagHasChildren(List<NavItem> tags, int i) =>
    i + 1 < tags.length && tags[i + 1].depth > tags[i].depth;

/// `tags` without the ones under a collapsed tag.
List<NavItem> visibleTags(List<NavItem> tags, Set<String> collapsed) {
  if (collapsed.isEmpty) return tags;
  final out = <NavItem>[];
  int? hideBelow;
  for (final t in tags) {
    if (hideBelow != null && t.depth > hideBelow) continue;
    hideBelow = null;
    out.add(t);
    if (collapsed.contains(t.id)) hideBelow = t.depth;
  }
  return out;
}
