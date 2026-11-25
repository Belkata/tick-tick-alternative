import 'dart:async';

// ignore: deprecated_member_use
import 'package:flutter/foundation.dart';

import '../src/rust/api/tasks.dart';
import 'task_sort.dart';

/// State for the three-pane shell: the open view, the selected task and what they show.
/// Every write goes through the bridge; its change stream triggers a reload, so the
/// panes stay live (and will for writes from other processes once the socket lands).
class DeskController extends ChangeNotifier {
  DeskController(this.db) {
    _sub = db.changes().listen((_) => reload());
  }

  final TaskDb db;
  late final StreamSubscription<int> _sub;

  TaskView view = const TaskView(kind: ViewKind.inbox);
  String? selectedId;
  Sidebar? sidebar;
  List<TaskRow> tasks = const [];

  /// How the open view is sorted (core sort keys); empty where there's no sort menu.
  List<String> sort = const [];
  TaskDetail? detail;
  bool loaded = false;

  /// The last failed command's message, for a snackbar. Cleared once shown.
  String? error;

  int _generation = 0;

  @override
  void dispose() {
    _sub.cancel();
    super.dispose();
  }

  /// Re-query everything the panes show. Results of an older reload are dropped.
  /// If the open list, tag or filter is gone (deleted here, by `utre` or elsewhere),
  /// fall back to Inbox; other failures become [error] instead of exceptions.
  Future<void> reload() async {
    final gen = ++_generation;
    final v = view;
    final sel = selectedId;
    final Sidebar sb;
    final List<TaskRow> rows;
    final List<String> order;
    final TaskDetail? d;
    try {
      sb = await db.sidebar();
      if (v.kind != ViewKind.inbox && !_exists(sb, v)) {
        if (gen == _generation) open(const TaskView(kind: ViewKind.inbox));
        return;
      }
      rows = await db.tasks(view: v);
      order = sortable(v.kind) ? await db.viewSort(view: v) : const <String>[];
      d = sel == null ? null : await db.task(id: sel);
    } catch (e) {
      if (gen != _generation) return;
      error = _clean(e);
      notifyListeners();
      return;
    }
    if (gen != _generation) return;
    sidebar = sb;
    tasks = rows;
    sort = order;
    detail = d;
    if (d == null) selectedId = null;
    loaded = true;
    notifyListeners();
  }

  static bool _exists(Sidebar sb, TaskView v) => switch (v.kind) {
    ViewKind.list => sb.lists.any((n) => n.id == v.id),
    ViewKind.tag => sb.tags.any((n) => n.id == v.id),
    ViewKind.filter => sb.filters.any((n) => n.id == v.id),
    _ => true,
  };

  static String _clean(Object e) => '$e'
      .replaceFirst(RegExp(r'^(AnyhowException|Exception)\(?'), '')
      .replaceFirst(RegExp(r'\)$'), '');

  void open(TaskView v) {
    view = v;
    selectedId = null;
    detail = null;
    reload();
  }

  void select(String? id) {
    if (id == selectedId) return;
    selectedId = id;
    reload();
  }

  /// Arrow keys: move the selection up or down the list.
  void moveSelection(int delta) {
    if (tasks.isEmpty) return;
    final i = tasks.indexWhere((t) => t.id == selectedId);
    final next = i < 0
        ? (delta > 0 ? 0 : tasks.length - 1)
        : (i + delta).clamp(0, tasks.length - 1);
    select(tasks[next].id);
  }

  /// Run a command; a failure becomes [error] instead of an exception.
  Future<T?> run<T>(Future<T> Function() command) async {
    try {
      return await command();
    } catch (e) {
      error = _clean(e);
      notifyListeners();
      return null;
    }
  }

  /// The header for the open view.
  String get title {
    final sb = sidebar;
    String find(List<NavItem>? items) =>
        items?.where((n) => n.id == view.id).map((n) => n.name).firstOrNull ??
        '';
    return switch (view.kind) {
      ViewKind.inbox => 'Inbox',
      ViewKind.list => find(sb?.lists),
      ViewKind.tag => find(sb?.tags),
      ViewKind.filter => find(sb?.filters),
      ViewKind.completed => 'Completed',
      ViewKind.trash => 'Trash',
    };
  }

  /// Quick add makes sense where a new task has an obvious home.
  bool get canAdd => switch (view.kind) {
    ViewKind.inbox || ViewKind.list || ViewKind.tag || ViewKind.filter => true,
    ViewKind.completed || ViewKind.trash => false,
  };

  Future<void> add(String title) async {
    final t = title.trim();
    if (t.isEmpty) return;
    await run(() => db.addIn(view: view, title: t));
  }

  /// Choose how the open view is sorted.
  Future<void> setSort(List<String> keys) async {
    sort = keys;
    notifyListeners();
    await run(() => db.setViewSort(view: view, sort: keys));
  }

  /// Manual order: move a task before `before` (or last), showing it there at once.
  Future<void> placeTask(String id, String? before) async {
    tasks = applyTaskMove(tasks, id, before);
    notifyListeners();
    await run(() => db.placeTask(view: view, id: id, before: before));
  }

  Future<void> toggleDone(String id) => run(() => db.toggleDone(id: id));

  Future<void> trashOrRestore(String id) async {
    final row = tasks.where((t) => t.id == id).firstOrNull ?? detail?.row;
    if (row?.deleted ?? false) {
      await run(() => db.restore(id: id));
    } else {
      // Keep the keyboard flow going: select the next task before this one leaves.
      final i = tasks.indexWhere((t) => t.id == id);
      if (i >= 0 && tasks.length > 1) {
        selectedId = tasks[i + 1 < tasks.length ? i + 1 : i - 1].id;
      }
      await run(() => db.trash(id: id));
    }
  }
}
