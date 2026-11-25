// ignore: deprecated_member_use
import 'package:flutter/material.dart';

import '../src/rust/api/tasks.dart';
import 'controller.dart';
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
            child: Row(
              children: [
                ?leading,
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
                const Spacer(),
                if (sortable(c.view.kind)) _SortMenu(c: c),
              ],
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
                ? Center(
                    child: Text(
                      empty,
                      style: manrope(13.5, FontWeight.w500, color: s.mute),
                    ),
                  )
                : _manual
                ? _ReorderableTasks(c: c, onOpen: onOpen)
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

/// The header's sort button: a menu of [sortChoices], the current one ticked.
class _SortMenu extends StatelessWidget {
  const _SortMenu({required this.c});

  final DeskController c;

  @override
  Widget build(BuildContext context) {
    final s = context.soft;
    final current = sortChoiceOf(c.sort);
    return PopupMenuButton<SortChoice>(
      key: const Key('sort-menu'),
      tooltip: 'Sort: ${current?.label ?? 'custom'}',
      icon: Icon(Icons.sort, size: 20, color: s.mute),
      position: PopupMenuPosition.under,
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
  void dispose() {
    _text.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final where = switch (widget.c.view.kind) {
      ViewKind.list || ViewKind.tag => widget.c.title,
      _ => 'Inbox',
    };
    return TextField(
      key: const Key('add-field'),
      controller: _text,
      focusNode: widget.focus,
      textInputAction: TextInputAction.done,
      decoration: InputDecoration(
        hintText: 'Add a task to $where',
        prefixIcon: Icon(Icons.add, size: 18, color: context.soft.mute),
        prefixIconConstraints: const BoxConstraints(minWidth: 36),
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

/// One rounded task card (Soft): priority checkbox, title, then tags and date on the right.
class TaskCard extends StatelessWidget {
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
  Widget build(BuildContext context) {
    final s = context.soft;
    final dueColor = row.done
        ? s.mute
        : row.overdue
        ? s.late
        : row.dueToday
        ? s.today
        : s.mute;
    final meta = <Widget>[
      if (row.checklistTotal > 0)
        _Meta(
          icon: Icons.checklist,
          text: '${row.checklistDone}/${row.checklistTotal}',
        ),
      if (row.subtasks > 0)
        _Meta(icon: Icons.subdirectory_arrow_right, text: '${row.subtasks}'),
      if (showList && row.listName != null)
        _Meta(icon: Icons.format_list_bulleted, text: row.listName!),
      for (final t in row.tags)
        TagPill(label: t.name, color: t.color, icon: t.icon),
      if (row.dueLabel != null)
        Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            if (row.repeating) Icon(Icons.repeat, size: 12, color: dueColor),
            if (row.repeating) const SizedBox(width: 2),
            Text(
              row.dueLabel!,
              style: manrope(11.5, FontWeight.w600, color: dueColor),
            ),
          ],
        ),
      if (row.whenLabel != null)
        Text(
          row.whenLabel!,
          style: manrope(11.5, FontWeight.w500, color: s.mute),
        ),
    ];
    return Padding(
      padding: const EdgeInsets.only(bottom: 6),
      child: Material(
        color: s.panel,
        elevation: s.cardShadow ? 0.6 : 0,
        shadowColor: const Color(0x22141E3C),
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(12),
          side: selected
              ? BorderSide(color: s.accent, width: 2)
              : BorderSide.none,
        ),
        child: InkWell(
          borderRadius: BorderRadius.circular(12),
          onTap: onTap,
          child: ConstrainedBox(
            constraints: const BoxConstraints(minHeight: 44),
            child: Padding(
              padding: const EdgeInsets.fromLTRB(8, 4, 12, 4),
              child: Row(
                children: [
                  PriorityCheck(
                    key: Key('check-${row.id}'),
                    priority: row.priority,
                    done: row.done,
                    onTap: onCheck,
                  ),
                  const SizedBox(width: 8),
                  Expanded(
                    child: Text(
                      row.title,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style:
                          manrope(
                            13.5,
                            FontWeight.w500,
                            color: row.done ? s.mute : s.fg,
                          ).copyWith(
                            decoration: row.done
                                ? TextDecoration.lineThrough
                                : null,
                          ),
                    ),
                  ),
                  if (meta.isNotEmpty) ...[
                    const SizedBox(width: 8),
                    Flexible(
                      child: Wrap(
                        alignment: WrapAlignment.end,
                        crossAxisAlignment: WrapCrossAlignment.center,
                        spacing: 6,
                        runSpacing: 4,
                        children: meta,
                      ),
                    ),
                  ],
                ],
              ),
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
