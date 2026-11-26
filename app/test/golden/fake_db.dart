import 'package:utre/src/rust/api/tasks.dart';

/// Wednesday 2026-11-25, 10:30: the clock every golden runs on.
final goldenNow = DateTime(2026, 11, 25, 10, 30);

int ms(DateTime d) => d.millisecondsSinceEpoch;

/// A day at a time of day, [days] from [base] (the golden "today" unless told otherwise).
DateTime at(int days, [int hour = 0, int minute = 0, DateTime? base]) =>
    fixtureAt(days, hour, minute, base);

DateTime fixtureAt(int days, int hour, int minute, DateTime? base) {
  final b = base ?? goldenNow;
  return DateTime(b.year, b.month, b.day + days, hour, minute);
}

const work = TagChip(
  id: 'tag-work',
  name: 'work',
  path: 'work',
  color: '#2f7fd1',
);
const home = TagChip(
  id: 'tag-home',
  name: 'home',
  path: 'home',
  color: '#d98324',
);
const deepFocus = TagChip(
  id: 'tag-focus',
  name: 'focus',
  path: 'work/focus',
  color: '#2f7fd1',
);

TaskRow row(
  String id,
  String title, {
  int priority = 0,
  DateTime? due,
  String? dueLabel,
  bool allDay = false,
  bool overdue = false,
  bool dueToday = false,
  bool done = false,
  bool repeating = false,
  String? list,
  List<TagChip> tags = const [],
  int checkDone = 0,
  int checkTotal = 0,
  int subtasks = 0,
}) => TaskRow(
  id: id,
  title: title,
  priority: priority,
  // The core stores an all-day date as UTC midnight (task_sort.dart reads it back in UTC).
  dueAt: due == null
      ? null
      : ms(allDay ? DateTime.utc(due.year, due.month, due.day) : due),
  allDay: allDay,
  dueLabel: dueLabel,
  overdue: overdue,
  dueToday: dueToday,
  done: done,
  deleted: false,
  repeating: repeating,
  listName: list,
  tags: tags,
  checklistDone: checkDone,
  checklistTotal: checkTotal,
  subtasks: subtasks,
);

/// The tasks the task list and shell goldens show: every priority, every due group,
/// tags, a checklist, subtasks, a repeat, a done task and a long title.
///
/// The list groups rows by comparing their dates with the real clock (a screen with no
/// clock hook), so the shell passes the real today as [base] to keep the groups the same.
List<TaskRow> sampleTasks([DateTime? base]) {
  DateTime at(int days, [int hour = 0, int minute = 0]) =>
      fixtureAt(days, hour, minute, base);
  return [
    row(
      't1',
      'Send the invoice to Marta',
      priority: 3,
      due: at(-2, 9),
      dueLabel: 'Mon 23 Nov',
      overdue: true,
      list: 'Work',
      tags: [work],
    ),
    row(
      't2',
      'Prepare the quarterly review slides with the numbers from finance',
      priority: 2,
      due: at(0, 14),
      dueLabel: 'Today 14:00',
      dueToday: true,
      list: 'Work',
      tags: [work, deepFocus],
      checkDone: 2,
      checkTotal: 5,
    ),
    row(
      't3',
      'Buy groceries',
      priority: 1,
      due: at(0),
      dueLabel: 'Today',
      allDay: true,
      dueToday: true,
      tags: [home],
      checkDone: 1,
      checkTotal: 4,
    ),
    row(
      't4',
      'Water the plants',
      due: at(1),
      dueLabel: 'Tomorrow',
      allDay: true,
      repeating: true,
      list: 'Home',
    ),
    row(
      't5',
      'Plan the trip',
      priority: 2,
      due: at(4, 18),
      dueLabel: 'Sun 29 Nov 18:00',
      list: 'Home',
      subtasks: 3,
    ),
    row(
      't6',
      'Read the Rust async book',
      due: at(20),
      dueLabel: '15 Dec',
      allDay: true,
    ),
    row('t7', 'Call the dentist'),
    row(
      't8',
      'Renew passport',
      done: true,
      due: at(-5),
      dueLabel: 'Fri 20 Nov',
      allDay: true,
    ),
  ];
}

TaskDetail detailOf(TaskRow r) => TaskDetail(
  row: r,
  notes: 'Numbers are in the shared sheet.\n\n- revenue by region\n- churn',
  listId: null,
  startLabel: null,
  repeatRule: r.repeating ? 'FREQ=WEEKLY' : null,
  repeatAfterCompletion: false,
  checklist: const [
    ChecklistRow(id: 'c1', title: 'Collect the figures', done: true),
    ChecklistRow(id: 'c2', title: 'Draft the outline', done: true),
    ChecklistRow(id: 'c3', title: 'Charts', done: false),
  ],
  subtasks: const [],
  reminders: const [ReminderRow(id: 'r1', label: '30 min before')],
  inCalendar: true,
);

Sidebar sampleSidebar() => const Sidebar(
  inbox: 4,
  lists: [
    NavItem(id: 'l1', name: 'Work', color: '#2f7fd1', count: 3, depth: 0),
    NavItem(id: 'l2', name: 'Home', color: '#d98324', count: 2, depth: 0),
  ],
  filters: [
    NavItem(id: 'f1', name: 'Today', count: 2, depth: 0),
    NavItem(id: 'f2', name: 'Overdue', count: 1, depth: 0),
  ],
  tags: [
    NavItem(id: 'tag-work', name: 'work', color: '#2f7fd1', count: 3, depth: 0),
    NavItem(
      id: 'tag-focus',
      name: 'focus',
      color: '#2f7fd1',
      count: 1,
      depth: 1,
      parentId: 'tag-work',
    ),
    NavItem(id: 'tag-home', name: 'home', color: '#d98324', count: 2, depth: 0),
  ],
);

CalendarItem block(
  String id,
  String title,
  DateTime start,
  int minutes, {
  bool done = false,
  int priority = 0,
  bool allDay = false,
  bool repeating = false,
  String? color,
}) => CalendarItem(
  taskId: id,
  title: title,
  startAt: ms(start),
  endAt: ms(start.add(Duration(minutes: minutes))),
  allDay: allDay,
  done: done,
  movable: true,
  repeating: repeating,
  priority: priority,
  color: color,
);

/// A week of blocks around the golden "today": overlaps, a done block, an all-day task,
/// a repeat, and one block each priority.
List<CalendarItem> sampleBlocks() => [
  block('b1', 'Standup', at(0, 9), 30, priority: 1, repeating: true),
  block(
    'b2',
    'Write the report',
    at(0, 10),
    120,
    priority: 2,
    color: '#2f7fd1',
  ),
  block('b3', 'Review PRs', at(0, 10, 30), 60, priority: 3),
  block('b4', 'Lunch with Ana', at(0, 12, 30), 60, done: true),
  block('b5', 'Dentist', at(0, 15), 45),
  block('b6', 'Plan the trip', at(1, 9, 30), 90, priority: 2),
  block('b7', 'Gym', at(1, 18), 60, color: '#d98324'),
  block('b8', 'Pay rent', at(0), 0, allDay: true, priority: 3),
  block('b9', 'Team offsite', at(2), 0, allDay: true),
  block('b10', 'Call mum', at(-1, 19), 30, done: true),
  block('b11', 'Deep work', at(3, 9), 180, priority: 2),
  block('b12', 'Groceries', at(4, 11), 45),
];

List<GcalEvent> sampleGoogle() => [
  GcalEvent(
    calendarId: 'g1',
    id: 'e1',
    summary: 'Sprint planning',
    startAt: ms(at(0, 11)),
    endAt: ms(at(0, 12)),
    allDay: false,
    color: '#8e24aa',
  ),
  GcalEvent(
    calendarId: 'g1',
    id: 'e2',
    summary: 'Anna\'s birthday',
    startAt: ms(at(2)),
    endAt: ms(at(3)),
    allDay: true,
    color: '#8e24aa',
  ),
];

/// A stand-in for the core that answers from fixed data.
class FakeDb implements TaskDb {
  FakeDb({
    List<TaskRow>? tasks,
    this.blocks = const [],
    this.google = const [],
    this.peerList = const [],
    this.sync = true,
  }) : _tasks = tasks ?? sampleTasks();

  final List<TaskRow> _tasks;
  final List<CalendarItem> blocks;
  final List<GcalEvent> google;
  final List<Peer> peerList;
  final bool sync;

  @override
  Stream<int> changes() => const Stream.empty();

  @override
  bool isRemote() => false;

  @override
  Future<Sidebar> sidebar() async => sampleSidebar();

  @override
  Future<List<TaskRow>> tasks({required TaskView view}) async =>
      switch (view.kind) {
        ViewKind.completed => _tasks.where((t) => t.done).toList(),
        _ => _tasks.where((t) => !t.done).toList(),
      };

  @override
  Future<List<String>> viewSort({required TaskView view}) async => [
    'due_asc',
    'priority_desc',
  ];

  @override
  Future<TaskDetail?> task({required String id}) async =>
      detailOf(_tasks.firstWhere((t) => t.id == id));

  @override
  Future<List<CalendarItem>> calendar({
    required int fromMs,
    required int untilMs,
  }) async => blocks;

  @override
  Future<List<TaskRow>> unscheduled({String? filterId}) async =>
      _tasks.where((t) => t.dueAt == null && !t.done).toList();

  @override
  Future<List<GcalEvent>> gcalEvents({
    required int fromMs,
    required int untilMs,
  }) async => google;

  @override
  Future<List<GcalCalendar>> gcalCalendars() async => const [
    GcalCalendar(
      id: 'g1',
      summary: 'Personal',
      color: '#8e24aa',
      selected: true,
    ),
  ];

  @override
  Future<void> googleSyncIfStale({required int maxAgeSecs}) async {}

  @override
  bool syncAvailable() => sync;

  @override
  Future<SyncStatus> syncStatus() async => SyncStatus(
    nodeId: 'c382b28c2f9d41aa7e0b5d6c13f4a8e29b7c0d5e1f3a4b6c7d8e9f0a1b2c3d4e',
    peers: peerList.length,
    lastSyncMs: ms(DateTime.now().subtract(const Duration(minutes: 2))),
  );

  @override
  Future<List<Peer>> peers() async => peerList;

  @override
  dynamic noSuchMethod(Invocation i) => super.noSuchMethod(i);
}
