import 'package:flutter_test/flutter_test.dart';
import 'package:utre/desk/task_sort.dart';
import 'package:utre/src/rust/api/tasks.dart';

TaskRow row(String id) => TaskRow(
  id: id,
  title: id,
  priority: 0,
  allDay: false,
  overdue: false,
  dueToday: false,
  done: false,
  deleted: false,
  repeating: false,
  tags: const [],
  checklistDone: 0,
  checklistTotal: 0,
  subtasks: 0,
);

void main() {
  const ids = ['a', 'b', 'c'];

  test('a drop names the task to go before, or last', () {
    expect(taskDrop(ids, 2, 0), (id: 'c', before: 'a'));
    expect(taskDrop(ids, 0, 1), (id: 'a', before: 'c'));
    expect(taskDrop(ids, 0, 2), (id: 'a', before: null));
    expect(taskDrop(ids, 1, 1), isNull, reason: 'dropped where it was');
    expect(taskDrop(ids, 5, 0), isNull);
  });

  test('the moved order shows at once', () {
    final rows = [for (final i in ids) row(i)];
    List<String> order(List<TaskRow> r) => [for (final x in r) x.id];
    expect(order(applyTaskMove(rows, 'c', 'a')), ['c', 'a', 'b']);
    expect(order(applyTaskMove(rows, 'a', null)), ['b', 'c', 'a']);
    expect(order(applyTaskMove(rows, 'x', 'a')), ids);
  });

  test('menu entries map to and from sort keys', () {
    expect(sortChoiceOf(['due_asc', 'priority_desc'])?.id, 'due');
    expect(
      sortChoiceOf(['due_asc'])?.id,
      'due',
      reason: 'a filter builder sort',
    );
    expect(sortChoiceOf(['manual'])?.label, 'Manual');
    expect(sortChoiceOf(['start_asc']), isNull);
    expect(sortChoiceOf(const []), isNull);
    expect(sortable(ViewKind.trash), isFalse);
  });
}
