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

  group('due groups', () {
    final now = DateTime(2026, 11, 26, 12);
    TaskRow due(
      String id,
      DateTime? at, {
      bool overdue = false,
      bool today = false,
      bool allDay = false,
    }) => TaskRow(
      id: id,
      title: id,
      priority: 0,
      dueAt: at?.millisecondsSinceEpoch,
      allDay: allDay,
      overdue: overdue,
      dueToday: today,
      done: false,
      deleted: false,
      repeating: false,
      tags: const [],
      checklistDone: 0,
      checklistTotal: 0,
      subtasks: 0,
    );

    test('buckets by calendar day, overdue first, no date last', () {
      final rows = [
        due('late', DateTime(2026, 11, 25), overdue: true),
        due('now', DateTime(2026, 11, 26, 18), today: true),
        due('tom', DateTime(2026, 11, 27, 9)),
        due('wk', DateTime(2026, 12, 3)),
        due('far', DateTime(2026, 12, 4)),
        due('none', null),
      ];
      expect(
        [for (final r in rows) dueGroupOf(r, now)],
        [
          DueGroup.overdue,
          DueGroup.today,
          DueGroup.tomorrow,
          DueGroup.week,
          DueGroup.later,
          DueGroup.none,
        ],
      );
    });

    test('groups keep row order and skip empty buckets', () {
      final rows = [
        due('b', DateTime(2026, 11, 27, 11)),
        due('x', null),
        due('a', DateTime(2026, 11, 27, 8)),
      ];
      final g = groupByDue(rows, now);
      expect([for (final e in g) e.$1], [DueGroup.tomorrow, DueGroup.none]);
      expect([for (final r in g.first.$2) r.id], ['b', 'a']);
    });

    test('an all-day date is read as its UTC day', () {
      final r = due('d', DateTime.utc(2026, 11, 27), allDay: true);
      expect(dueGroupOf(r, now), DueGroup.tomorrow);
    });
  });
}
