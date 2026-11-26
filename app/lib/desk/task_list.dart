// ignore: deprecated_member_use
import 'package:flutter/material.dart';

import '../src/rust/api/tasks.dart';
import 'controller.dart';
import 'duration_picker.dart' show durationShort;
import 'task_sort.dart';
import 'theme.dart';

/// The middle pane: header, quick add and the task cards.
class TaskListPane extends StatelessWidget {
  const TaskListPane({
    super.key,
    required this.c,
    required this.addFocus,
    required this.onOpen,
    this.leading,
  });

  final DeskController c;
  final FocusNode addFocus;

  /// A row was tapped (the narrow layout pushes the detail).
  final ValueChanged<String> onOpen;

  /// Shown before the title, e.g. the drawer button on narrow windows.
  final Widget? leading;

  bool get _manual => sortable(c.view.kind) && c.sort.firstOrNull == 'manual';

  /// Sorted by due date: split the list under Overdue / Today / Tomorrow… headers.
  /// Only when something is dated (an all-undated list needs no single "No date" header).
  bool get _grouped =>
      sortable(c.view.kind) &&
      sortChoiceOf(c.sort)?.id == 'due' &&
      c.tasks.any((t) => t.dueAt != null);

  Widget _groupedList(List<TaskRow> rows) {
    final groups = groupByDue(rows, DateTime.now());
    return ListView(
      padding: const EdgeInsets.fromLTRB(12, 0, 12, 16),
      children: [
        for (final (g, items) in groups) ...[
          _GroupHeader(group: g, count: items.length),
          for (final r in items) _card(r),
        ],
      ],
    );
  }

  Widget _card(TaskRow row) => TaskCard(
    key: ValueKey(row.id),
    row: row,
    selected: row.id == c.selectedId,
    showList: c.view.kind != ViewKind.list,
    onTap: () => onOpen(row.id),
    onCheck: () => c.toggleDone(row.id),
  );

  @override
  Widget build(BuildContext context) {
    final s = context.soft;
    final rows = c.tasks;
    final empty = switch (c.view.kind) {
      ViewKind.inbox => 'Inbox is empty',
      ViewKind.completed => 'Nothing completed yet',
      ViewKind.trash => 'Trash is empty',
      _ => 'No tasks here',
    };
    return Container(
      color: s.bg,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Padding(
            padding: const EdgeInsets.fromLTRB(18, 16, 18, 10),
            // A fixed height, so views with and without the sort button line up.
            child: SizedBox(
              height: 34,
              child: Row(
                children: [
                  ?leading,
                  // Title and count take the free space, so the sort button stays
                  // at the right edge whatever the title's length.
                  Expanded(
                    child: Row(
                      children: [
                        Flexible(
                          child: Text(
                            c.title,
                            overflow: TextOverflow.ellipsis,
                            style: manrope(19, FontWeight.w800, color: s.fg),
                          ),
                        ),
                        const SizedBox(width: 10),
                        Text(
                          '${rows.length}',
                          style: manrope(12, FontWeight.w600, color: s.mute),
                        ),
                      ],
                    ),
                  ),
                  if (sortable(c.view.kind)) _SortMenu(c: c),
                ],
              ),
            ),
          ),
          if (c.canAdd)
            Padding(
              padding: const EdgeInsets.fromLTRB(16, 0, 16, 10),
              child: _QuickAdd(c: c, focus: addFocus),
            ),
          Expanded(
            child: !c.loaded
                ? const SizedBox.shrink()
                : rows.isEmpty
                ? _EmptyState(kind: c.view.kind, title: empty)
                : _manual
                ? _ReorderableTasks(c: c, onOpen: onOpen)
                : _grouped
                ? _groupedList(rows)
                : ListView.builder(
                    padding: const EdgeInsets.fromLTRB(12, 0, 12, 16),
                    itemCount: rows.length,
                    itemBuilder: (_, i) => _card(rows[i]),
                  ),
          ),
        ],
      ),
    );
  }
}

/// The header's sort button: the sort icon and the current choice's name, opening a
/// menu of [sortChoices] with the current one ticked.
class _SortMenu extends StatelessWidget {
  const _SortMenu({required this.c});

  final DeskController c;

  @override
  Widget build(BuildContext context) {
    final s = context.soft;
    final current = sortChoiceOf(c.sort);
    return PopupMenuButton<SortChoice>(
      key: const Key('sort-menu'),
      tooltip: 'Sort',
      position: PopupMenuPosition.under,
      borderRadius: BorderRadius.circular(8),
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 6),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(Icons.sort, size: 18, color: s.mute),
            const SizedBox(width: 6),
            Text(
              current?.label ?? 'Custom',
              key: const Key('sort-label'),
              style: manrope(12.5, FontWeight.w600, color: s.mute),
            ),
          ],
        ),
      ),
      onSelected: (choice) => c.setSort(choice.keys),
      itemBuilder: (_) => [
        for (final choice in sortChoices)
          PopupMenuItem(
            key: Key('sort-${choice.id}'),
            value: choice,
            height: 38,
            child: Row(
              children: [
                SizedBox(
                  width: 24,
                  child: choice == current
                      ? Icon(Icons.check, size: 16, color: s.accent)
                      : null,
                ),
                Text(choice.label, style: manrope(13.5, FontWeight.w500)),
              ],
            ),
          ),
      ],
    );
  }
}

/// Manual order: drag a card to move it (long-press on touch screens).
class _ReorderableTasks extends StatelessWidget {
  const _ReorderableTasks({required this.c, required this.onOpen});

  final DeskController c;
  final ValueChanged<String> onOpen;

  @override
  Widget build(BuildContext context) {
    final rows = c.tasks;
    final touch = switch (Theme.of(context).platform) {
      TargetPlatform.android || TargetPlatform.iOS => true,
      _ => false,
    };
    return ReorderableListView.builder(
      padding: const EdgeInsets.fromLTRB(12, 0, 12, 16),
      buildDefaultDragHandles: false,
      itemCount: rows.length,
      onReorderItem: (from, to) {
        final move = taskDrop([for (final r in rows) r.id], from, to);
        if (move != null) c.placeTask(move.id, move.before);
      },
      proxyDecorator: (child, _, _) =>
          Material(color: Colors.transparent, child: child),
      itemBuilder: (_, i) {
        final row = rows[i];
        final card = TaskCard(
          row: row,
          selected: row.id == c.selectedId,
          showList: c.view.kind != ViewKind.list,
          onTap: () => onOpen(row.id),
          onCheck: () => c.toggleDone(row.id),
        );
        return touch
            ? ReorderableDelayedDragStartListener(
                key: ValueKey(row.id),
                index: i,
                child: card,
              )
            : ReorderableDragStartListener(
                key: ValueKey(row.id),
                index: i,
                child: MouseRegion(
                  cursor: SystemMouseCursors.grab,
                  child: card,
                ),
              );
      },
    );
  }
}

class _QuickAdd extends StatefulWidget {
  const _QuickAdd({required this.c, required this.focus});

  final DeskController c;
  final FocusNode focus;

  @override
  State<_QuickAdd> createState() => _QuickAddState();
}

class _QuickAddState extends State<_QuickAdd> {
  final _text = TextEditingController();

  @override
  void initState() {
    super.initState();
    widget.focus.addListener(_onFocus);
    _text.addListener(_onText);
  }

  void _onFocus() => setState(() {});

  bool _hadText = false;

  // Only whether there is text decides what the field shows (the Enter hint), so typing
  // rebuilds the field when that flips, not on every keystroke.
  void _onText() {
    final has = _text.text.trim().isNotEmpty;
    if (has != _hadText) {
      _hadText = has;
      setState(() {});
    }
  }

  @override
  void dispose() {
    widget.focus.removeListener(_onFocus);
    _text.removeListener(_onText);
    _text.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final s = context.soft;
    final where = switch (widget.c.view.kind) {
      ViewKind.list || ViewKind.tag => widget.c.title,
      _ => 'Inbox',
    };
    final touch = switch (Theme.of(context).platform) {
      TargetPlatform.android || TargetPlatform.iOS => true,
      _ => false,
    };
    // Desktop: tell the shortcut while idle, and what Enter does while typing.
    final hint = touch
        ? null
        : widget.focus.hasFocus
        ? (_text.text.trim().isEmpty ? null : 'Enter to add')
        : 'N';
    return TextField(
      key: const Key('add-field'),
      controller: _text,
      focusNode: widget.focus,
      textInputAction: TextInputAction.done,
      decoration: InputDecoration(
        hintText: 'Add a task to $where',
        prefixIcon: Icon(Icons.add, size: 18, color: s.mute),
        prefixIconConstraints: const BoxConstraints(minWidth: 36),
        suffixIcon: hint == null
            ? null
            : Padding(
                padding: const EdgeInsets.only(right: 10),
                child: Center(
                  widthFactor: 1,
                  child: Container(
                    padding: const EdgeInsets.symmetric(
                      horizontal: 6,
                      vertical: 1,
                    ),
                    decoration: BoxDecoration(
                      border: Border.all(color: s.line),
                      borderRadius: BorderRadius.circular(5),
                    ),
                    child: Text(
                      hint,
                      style: manrope(10.5, FontWeight.w700, color: s.mute),
                    ),
                  ),
                ),
              ),
      ),
      onSubmitted: (v) async {
        _text.clear();
        // Keep focus so several tasks can be typed in a row.
        widget.focus.requestFocus();
        await widget.c.add(v);
      },
    );
  }
}

/// A section header in a date-sorted list: Overdue, Today, Tomorrow…
class _GroupHeader extends StatelessWidget {
  const _GroupHeader({required this.group, required this.count});

  final DueGroup group;
  final int count;

  @override
  Widget build(BuildContext context) {
    final s = context.soft;
    final color = switch (group) {
      DueGroup.overdue => s.late,
      DueGroup.today => s.today,
      _ => s.mute,
    };
    return Semantics(
      header: true,
      child: Padding(
        padding: const EdgeInsets.fromLTRB(8, 14, 8, 6),
        child: Row(
          children: [
            Text(
              group.label,
              key: Key('group-${group.name}'),
              style: manrope(12, FontWeight.w800, color: color),
            ),
            const SizedBox(width: 8),
            Text(
              '$count',
              style: manrope(11.5, FontWeight.w600, color: s.mute),
            ),
          ],
        ),
      ),
    );
  }
}

/// What a view shows when it has no tasks: a quiet icon, the headline and a hint.
class _EmptyState extends StatelessWidget {
  const _EmptyState({required this.kind, required this.title});

  final ViewKind kind;
  final String title;

  @override
  Widget build(BuildContext context) {
    final s = context.soft;
    final (icon, hint) = switch (kind) {
      ViewKind.inbox => (
        Icons.inbox_outlined,
        'Capture anything above. Press n to jump to it.',
      ),
      ViewKind.completed => (
        Icons.check_circle_outline,
        'Tasks you finish show up here.',
      ),
      ViewKind.trash => (Icons.delete_outline, 'Deleted tasks wait here.'),
      ViewKind.filter => (
        Icons.filter_alt_outlined,
        'Nothing matches this filter right now.',
      ),
      _ => (Icons.task_alt, 'Add one above.'),
    };
    return Center(
      child: Padding(
        padding: const EdgeInsets.all(24),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Container(
              width: 56,
              height: 56,
              decoration: BoxDecoration(
                color: s.accent.withValues(alpha: 0.12),
                shape: BoxShape.circle,
              ),
              child: Icon(icon, size: 26, color: s.selFg),
            ),
            const SizedBox(height: 14),
            Text(title, style: manrope(14.5, FontWeight.w700, color: s.fg)),
            const SizedBox(height: 4),
            Text(
              hint,
              textAlign: TextAlign.center,
              style: manrope(12.5, FontWeight.w500, color: s.mute),
            ),
          ],
        ),
      ),
    );
  }
}

/// One rounded task card (Soft): priority checkbox and title, with a quiet line of
/// facts under the title (due date first, then checklist, subtasks, list and tags).
class TaskCard extends StatefulWidget {
  const TaskCard({
    super.key,
    required this.row,
    required this.selected,
    required this.onTap,
    required this.onCheck,
    this.showList = true,
  });

  final TaskRow row;
  final bool selected;
  final bool showList;
  final VoidCallback onTap;
  final VoidCallback onCheck;

  @override
  State<TaskCard> createState() => _TaskCardState();
}

class _TaskCardState extends State<TaskCard> {
  bool _hover = false;

  @override
  Widget build(BuildContext context) {
    final s = context.soft;
    final row = widget.row;
    final touch = switch (Theme.of(context).platform) {
      TargetPlatform.android || TargetPlatform.iOS => true,
      _ => false,
    };
    final dueColor = row.done
        ? s.mute
        : row.overdue
        ? s.late
        : row.dueToday
        ? s.today
        : s.mute;
    final meta = <Widget>[
      if (row.dueLabel != null)
        Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(
              row.repeating ? Icons.repeat : Icons.calendar_today_outlined,
              size: 12,
              color: dueColor,
            ),
            const SizedBox(width: 3),
            Text(
              row.dueLabel!,
              style: manrope(11.5, FontWeight.w600, color: dueColor),
            ),
          ],
        ),
      if (row.durationMin != null)
        _Meta(
          icon: Icons.timelapse_outlined,
          text: durationShort(row.durationMin!),
        ),
      if (row.checklistTotal > 0)
        _Meta(
          icon: Icons.checklist,
          text: '${row.checklistDone}/${row.checklistTotal}',
        ),
      if (row.subtasks > 0)
        _Meta(icon: Icons.subdirectory_arrow_right, text: '${row.subtasks}'),
      if (widget.showList && row.listName != null)
        _Meta(icon: Icons.format_list_bulleted, text: row.listName!),
      for (final t in row.tags)
        TagPill(label: t.name, color: t.color, icon: t.icon),
      if (row.whenLabel != null)
        Text(
          row.whenLabel!,
          style: manrope(11.5, FontWeight.w500, color: s.mute),
        ),
    ];
    // Selected: a teal wash and a bar on the left edge. Hover: a faint wash.
    final fill = widget.selected
        ? Color.alphaBlend(s.accent.withValues(alpha: 0.11), s.panel)
        : _hover
        ? Color.alphaBlend(s.fg.withValues(alpha: 0.04), s.panel)
        : s.panel;
    final titleStyle = manrope(
      13.5,
      FontWeight.w500,
      color: row.done ? s.mute : s.fg,
    ).copyWith(decoration: row.done ? TextDecoration.lineThrough : null);
    // The checkbox is centred on the title's first line, not on the whole card.
    final hit = touch ? 15.5 : 3.0;
    final align = FirstLineAlign(
      box: 17 + 2 * hit,
      lineHeight: FirstLineAlign.lineHeightOf(context, titleStyle),
      textTop: meta.isEmpty ? 10 : 8,
    );
    return Padding(
      padding: const EdgeInsets.only(bottom: 6),
      child: FocusRing(
        radius: 12,
        child: Material(
          animationDuration: motion(context, 160),
          color: fill,
          elevation: s.cardShadow ? 0.6 : 0,
          shadowColor: const Color(0x22141E3C),
          borderRadius: BorderRadius.circular(12),
          clipBehavior: Clip.antiAlias,
          child: InkWell(
            onTap: widget.onTap,
            onHover: (h) => setState(() => _hover = h),
            hoverColor: Colors.transparent,
            child: Stack(
              children: [
                Positioned(
                  left: 0,
                  top: 10,
                  bottom: 10,
                  child: AnimatedContainer(
                    duration: motion(context),
                    curve: Curves.easeOut,
                    width: widget.selected ? 3 : 0,
                    decoration: BoxDecoration(
                      color: s.accent,
                      borderRadius: const BorderRadius.horizontal(
                        right: Radius.circular(3),
                      ),
                    ),
                  ),
                ),
                ConstrainedBox(
                  constraints: BoxConstraints(minHeight: touch ? 48 : 44),
                  child: Padding(
                    padding: EdgeInsets.fromLTRB(touch ? 0 : 8, 4, 12, 4),
                    child: Row(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Padding(
                          padding: EdgeInsets.only(top: align.lead),
                          child: PriorityCheck(
                            key: Key('check-${row.id}'),
                            priority: row.priority,
                            done: row.done,
                            hit: hit,
                            onTap: widget.onCheck,
                          ),
                        ),
                        const SizedBox(width: 8),
                        Expanded(
                          child: Padding(
                            padding: EdgeInsets.only(
                              top: align.text,
                              bottom: meta.isEmpty ? 10 : 8,
                            ),
                            child: Column(
                              crossAxisAlignment: CrossAxisAlignment.start,
                              children: [
                                Text(
                                  row.title,
                                  key: Key('title-${row.id}'),
                                  maxLines: 2,
                                  overflow: TextOverflow.ellipsis,
                                  style: titleStyle,
                                ),
                                if (meta.isNotEmpty) ...[
                                  const SizedBox(height: 4),
                                  Wrap(
                                    spacing: 8,
                                    runSpacing: 4,
                                    crossAxisAlignment:
                                        WrapCrossAlignment.center,
                                    children: meta,
                                  ),
                                ],
                              ],
                            ),
                          ),
                        ),
                      ],
                    ),
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

class _Meta extends StatelessWidget {
  const _Meta({required this.icon, required this.text});

  final IconData icon;
  final String text;

  @override
  Widget build(BuildContext context) {
    final s = context.soft;
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        Icon(icon, size: 12, color: s.mute),
        const SizedBox(width: 2),
        Text(text, style: manrope(11, FontWeight.w500, color: s.mute)),
      ],
    );
  }
}
