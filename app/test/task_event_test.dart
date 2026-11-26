import 'package:flutter_test/flutter_test.dart';
import 'package:utre/calendar/calendar_page.dart';
import 'package:utre/src/rust/api/tasks.dart';

CalendarItem _item(DateTime start, DateTime end, {bool allDay = true}) =>
    CalendarItem(
      taskId: 't',
      title: 'Pay rent',
      startAt: start.millisecondsSinceEpoch,
      endAt: end.millisecondsSinceEpoch,
      allDay: allDay,
      done: false,
      movable: true,
      repeating: false,
      priority: 0,
    );

void main() {
  test('an all-day block with no length still covers its day', () {
    final day = DateTime(2026, 11, 25);
    final e = TaskEvent(_item(day, day));
    expect(e.end.toLocal(), DateTime(2026, 11, 26));
    expect(e.isAllDay, isTrue);
  });

  test('the core\'s whole-day block is kept as it is', () {
    final day = DateTime(2026, 3, 29); // a DST change in many zones
    final next = DateTime(2026, 3, 30);
    expect(TaskEvent(_item(day, next)).end.toLocal(), next);
  });

  test('a timed block keeps its end', () {
    final s = DateTime(2026, 11, 25, 10);
    final end = DateTime(2026, 11, 25, 10, 30);
    expect(TaskEvent(_item(s, end, allDay: false)).end.toLocal(), end);
  });
}
