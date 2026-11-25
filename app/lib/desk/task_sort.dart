import '../src/rust/api/tasks.dart';

/// One entry in a view's sort menu: the core sort keys it saves (first wins).
class SortChoice {
  const SortChoice(this.id, this.label, this.keys);

  final String id;
  final String label;
  final List<String> keys;
}

/// The sort menu, matching `utre sort`'s names. Ties fall back to creation order.
const sortChoices = [
  SortChoice('due', 'Due date', ['due_asc', 'priority_desc']),
  SortChoice('priority', 'Priority', ['priority_desc', 'due_asc']),
  SortChoice('title', 'Title', ['title_asc']),
  SortChoice('newest', 'Newest first', ['created_desc']),
  SortChoice('oldest', 'Oldest first', ['created_asc']),
  SortChoice('manual', 'Manual', ['manual']),
];

/// The menu entry a view's sort shows as; null for a sort set elsewhere
/// (e.g. `start_asc` from the CLI).
SortChoice? sortChoiceOf(List<String> sort) {
  final first = sort.firstOrNull;
  return sortChoices.where((c) => c.keys.first == first).firstOrNull;
}

/// Views with a sort menu.
bool sortable(ViewKind k) => switch (k) {
  ViewKind.inbox || ViewKind.list || ViewKind.tag || ViewKind.filter => true,
  ViewKind.completed || ViewKind.trash => false,
};

/// Where the task at `from` lands when dropped at `to` (counted after it was lifted
/// out): the task to go before, or null for last. Null overall when nothing moves.
({String id, String? before})? taskDrop(List<String> ids, int from, int to) {
  if (from < 0 || from >= ids.length || to == from) return null;
  final rest = [...ids]..removeAt(from);
  if (to < 0 || to > rest.length) return null;
  return (id: ids[from], before: to < rest.length ? rest[to] : null);
}

/// `rows` with `id` moved before `before`, or last. Shown while the database catches up.
List<TaskRow> applyTaskMove(List<TaskRow> rows, String id, String? before) {
  final moved = rows.where((r) => r.id == id).firstOrNull;
  if (moved == null) return rows;
  final rest = [...rows]..remove(moved);
  final at = before == null
      ? rest.length
      : rest.indexWhere((r) => r.id == before);
  if (at < 0) return rows;
  return [...rest.sublist(0, at), moved, ...rest.sublist(at)];
}
