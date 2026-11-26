// ignore: deprecated_member_use
import 'package:flutter/material.dart';

import '../src/rust/api/tasks.dart';
import 'controller.dart';
import 'date_picker.dart';
import 'markdown.dart';
import 'sidebar.dart' show askText;
import 'task_list.dart';
import 'theme.dart';

/// The right pane: everything about the selected task, edited in place.
class DetailPane extends StatelessWidget {
  const DetailPane({super.key, required this.c, required this.titleFocus});

  final DeskController c;

  /// `Enter` in the list focuses the title.
  final FocusNode titleFocus;

  @override
  Widget build(BuildContext context) {
    final s = context.soft;
    final d = c.detail;
    if (d == null) {
      return Container(
        color: s.panel,
        alignment: Alignment.center,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(Icons.checklist_rounded, size: 34, color: s.line),
            const SizedBox(height: 10),
            Text(
              'Select a task',
              style: manrope(13.5, FontWeight.w600, color: s.mute),
            ),
          ],
        ),
      );
    }
    final r = d.row;
    final id = r.id;
    final db = c.db;
    final lists = c.sidebar?.lists ?? const <NavItem>[];
    final tagPaths = [
      for (final t in c.sidebar?.tags ?? const <NavItem>[])
        _path(c.sidebar!.tags, t),
    ];
    final checked = d.checklist.where((i) => i.done).length;
    final subsDone = d.subtasks.where((t) => t.done).length;

    // The picker starts from the stored dates. The bridge gives the start date
    // only as a label, so it is read back from that.
    final dueMs = r.dueAt;
    final dueWhen = dueMs == null ? null : whenFromMs(dueMs, r.allDay);
    final startWhen = parseWhenLabel(d.startLabel, DateTime.now());

    final dateRows = <Widget>[
      _PropRow(
        icon: Icons.event_outlined,
        label: 'Due',
        child: _DateField(
          pickKey: const Key('detail-due-pick'),
          title: 'Due date',
          current: dueWhen,
          other: startWhen == null ? null : WhenLimit(startWhen, 'start'),
          isStart: false,
          onPicked: (p) => c.run(() => db.setDue(id: id, text: p.text)),
          field: _EditField(
            key: const Key('detail-due'),
            value: r.dueLabel ?? '',
            hint: 'fri 14:30',
            style: manrope(
              13,
              FontWeight.w700,
              color: r.overdue ? s.late : (r.dueToday ? s.today : s.fg),
            ),
            inline: true,
            onSave: (v) => c.run(() => db.setDue(id: id, text: v)),
          ),
        ),
      ),
      _PropRow(
        icon: Icons.play_circle_outline,
        label: 'Start',
        child: _DateField(
          pickKey: const Key('detail-start-pick'),
          title: 'Start date',
          current: startWhen,
          other: dueWhen == null ? null : WhenLimit(dueWhen, 'due'),
          isStart: true,
          onPicked: (p) => c.run(() => db.setStart(id: id, text: p.text)),
          field: _EditField(
            key: const Key('detail-start'),
            value: d.startLabel ?? '',
            hint: 'optional',
            style: manrope(13, FontWeight.w600, color: s.fg),
            inline: true,
            onSave: (v) => c.run(() => db.setStart(id: id, text: v)),
          ),
        ),
      ),
      _PropRow(
        icon: Icons.repeat_rounded,
        label: 'Repeat',
        child: _RepeatPicker(
          rule: d.repeatRule,
          afterCompletion: d.repeatAfterCompletion,
          onSet: (rule, after) => c.run(
            () => db.setRepeat(id: id, rule: rule, afterCompletion: after),
          ),
        ),
      ),
      // Only a task with a date can be on the calendar, so only then the switch.
      if (r.dueAt != null || d.startLabel != null)
        _PropRow(
          icon: Icons.calendar_month_outlined,
          label: 'Calendar',
          child: Row(
            children: [
              SizedBox(
                width: 42,
                child: FittedBox(
                  alignment: Alignment.centerLeft,
                  child: Switch(
                    key: const Key('detail-in-calendar'),
                    value: d.inCalendar,
                    onChanged: (v) =>
                        c.run(() => db.setInCalendar(id: id, visible: v)),
                  ),
                ),
              ),
              const SizedBox(width: 6),
              Expanded(
                child: Text(
                  d.inCalendar ? 'Shown in Calendar' : 'Kept off the calendar',
                  maxLines: 2,
                  overflow: TextOverflow.ellipsis,
                  style: manrope(12, FontWeight.w500, color: s.mute),
                ),
              ),
            ],
          ),
        ),
    ];

    final placeRows = <Widget>[
      _PropRow(
        icon: Icons.folder_outlined,
        label: 'List',
        child: Align(
          alignment: Alignment.centerLeft,
          child: DropdownButton<String?>(
            key: const Key('detail-list'),
            value: lists.any((l) => l.id == d.listId) ? d.listId : null,
            isDense: true,
            underline: const SizedBox.shrink(),
            style: manrope(13, FontWeight.w600, color: s.fg),
            items: [
              DropdownMenuItem(
                value: null,
                child: Text(
                  'No list',
                  style: manrope(13, FontWeight.w500, color: s.mute),
                ),
              ),
              for (final l in lists)
                DropdownMenuItem(value: l.id, child: Text(l.name)),
            ],
            onChanged: (v) => c.run(() => db.setList(id: id, listId: v)),
          ),
        ),
      ),
      _PropRow(
        icon: Icons.sell_outlined,
        label: 'Tags',
        child: Wrap(
          spacing: 6,
          runSpacing: 6,
          crossAxisAlignment: WrapCrossAlignment.center,
          children: [
            for (final t in r.tags)
              TagPill(
                label: t.path,
                color: t.color,
                icon: t.icon,
                onDeleted: () => c.run(() => db.untag(id: id, tagId: t.id)),
              ),
            _AddTag(
              paths: tagPaths,
              onAdd: (p) => c.run(() => db.tag(id: id, path: p)),
            ),
          ],
        ),
      ),
      _PropRow(
        icon: Icons.account_tree_outlined,
        label: 'Parent',
        child: Row(
          children: [
            Flexible(
              child: InkWell(
                key: const Key('detail-parent'),
                borderRadius: BorderRadius.circular(6),
                onTap: () => _pickParent(context, id, r.parentId),
                child: Padding(
                  padding: const EdgeInsets.symmetric(vertical: 4),
                  child: Text(
                    d.parentTitle ?? 'None',
                    overflow: TextOverflow.ellipsis,
                    style: manrope(
                      13,
                      d.parentTitle == null ? FontWeight.w500 : FontWeight.w600,
                      color: d.parentTitle == null ? s.mute : s.fg,
                    ),
                  ),
                ),
              ),
            ),
            if (r.parentId != null)
              IconButton(
                key: const Key('detail-parent-clear'),
                tooltip: 'Make it a top-level task',
                visualDensity: VisualDensity.compact,
                icon: Icon(Icons.close, size: 14, color: s.mute),
                onPressed: () =>
                    c.run(() => db.setParent(id: id, parentId: null)),
              ),
          ],
        ),
      ),
    ];

    return Container(
      color: s.panel,
      child: ListView(
        key: ValueKey('detail-$id'),
        padding: EdgeInsets.fromLTRB(
          20,
          16,
          20,
          28 + MediaQuery.viewPaddingOf(context).bottom,
        ),
        children: [
          // A subtask: one tap goes up the tree.
          if (r.parentId != null)
            Align(
              alignment: Alignment.centerLeft,
              child: TextButton.icon(
                key: const Key('detail-up'),
                onPressed: () => c.select(r.parentId),
                icon: Icon(Icons.arrow_upward, size: 14, color: s.mute),
                label: Text(
                  d.parentTitle ?? 'Parent task',
                  overflow: TextOverflow.ellipsis,
                  style: manrope(12, FontWeight.w600, color: s.mute),
                ),
                style: TextButton.styleFrom(
                  visualDensity: VisualDensity.compact,
                  padding: const EdgeInsets.symmetric(horizontal: 4),
                ),
              ),
            ),
          Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Padding(
                padding: const EdgeInsets.only(top: 5),
                child: PriorityCheck(
                  priority: r.priority,
                  done: r.done,
                  size: 20,
                  onTap: () => c.toggleDone(id),
                ),
              ),
              const SizedBox(width: 8),
              Expanded(
                child: _EditField(
                  key: const Key('detail-title'),
                  value: r.title,
                  focus: titleFocus,
                  multiline: true,
                  minLines: 1,
                  maxLines: 4,
                  style: manrope(
                    21,
                    FontWeight.w800,
                    color: r.done ? s.mute : s.fg,
                  ).copyWith(height: 1.25),
                  onSave: (v) => v.trim().isEmpty
                      ? null
                      : c.run(() => db.setTitle(id: id, title: v.trim())),
                ),
              ),
            ],
          ),
          const SizedBox(height: 12),
          _PriorityPicker(
            value: r.priority,
            onPick: (p) => c.run(() => db.setPriority(id: id, priority: p)),
          ),
          const SizedBox(height: 14),
          _Card(children: dateRows),
          const SizedBox(height: 10),
          _Card(children: placeRows),
          _Section(
            icon: Icons.notes_rounded,
            title: 'Notes',
            child: _Notes(
              key: const Key('detail-notes'),
              value: d.notes,
              onSave: (v) => c.run(() => db.setNotes(id: id, notes: v)),
            ),
          ),
          _Section(
            icon: Icons.checklist_rounded,
            title: 'Checklist',
            trailing: d.checklist.isEmpty
                ? null
                : '$checked/${d.checklist.length}',
            progress: d.checklist.isEmpty ? null : checked / d.checklist.length,
            child: Column(
              children: [
                for (final item in d.checklist)
                  Row(
                    children: [
                      PriorityCheck(
                        priority: 0,
                        done: item.done,
                        size: 15,
                        onTap: () => c.run(
                          () => db.checklistSet(
                            itemId: item.id,
                            done: !item.done,
                          ),
                        ),
                      ),
                      const SizedBox(width: 6),
                      Expanded(
                        child: Text(
                          item.title,
                          style:
                              manrope(
                                13,
                                FontWeight.w500,
                                color: item.done ? s.mute : s.fg,
                              ).copyWith(
                                decoration: item.done
                                    ? TextDecoration.lineThrough
                                    : null,
                              ),
                        ),
                      ),
                      IconButton(
                        tooltip: 'Remove',
                        visualDensity: VisualDensity.compact,
                        icon: Icon(Icons.close, size: 14, color: s.mute),
                        onPressed: () =>
                            c.run(() => db.checklistRemove(itemId: item.id)),
                      ),
                    ],
                  ),
                _AddLine(
                  key: const Key('add-checklist'),
                  hint: 'Add an item',
                  onAdd: (v) =>
                      c.run(() => db.checklistAdd(taskId: id, title: v)),
                ),
              ],
            ),
          ),
          _Section(
            icon: Icons.subdirectory_arrow_right_rounded,
            title: 'Subtasks',
            trailing: d.subtasks.isEmpty
                ? null
                : '$subsDone/${d.subtasks.length}',
            progress: d.subtasks.isEmpty ? null : subsDone / d.subtasks.length,
            child: Column(
              children: [
                for (final sub in d.subtasks)
                  TaskCard(
                    row: sub,
                    selected: false,
                    showList: false,
                    onTap: () => c.select(sub.id),
                    onCheck: () => c.toggleDone(sub.id),
                  ),
                _AddLine(
                  key: const Key('add-subtask'),
                  hint: 'Add a subtask',
                  onAdd: (v) =>
                      c.run(() => db.addSubtask(parentId: id, title: v)),
                ),
              ],
            ),
          ),
          _Section(
            icon: Icons.notifications_none_rounded,
            title: 'Reminders',
            child: Column(
              children: [
                for (final rem in d.reminders)
                  Row(
                    children: [
                      Icon(Icons.alarm, size: 14, color: s.mute),
                      const SizedBox(width: 8),
                      Expanded(
                        child: Text(
                          rem.label,
                          style: manrope(13, FontWeight.w500, color: s.fg),
                        ),
                      ),
                      IconButton(
                        tooltip: 'Remove',
                        visualDensity: VisualDensity.compact,
                        icon: Icon(Icons.close, size: 14, color: s.mute),
                        onPressed: () =>
                            c.run(() => db.removeReminder(id: rem.id)),
                      ),
                    ],
                  ),
                _AddReminder(
                  onBefore: (m) =>
                      c.run(() => db.addReminderBefore(taskId: id, minutes: m)),
                  onAt: (text) =>
                      c.run(() => db.addReminderAt(taskId: id, text: text)),
                ),
              ],
            ),
          ),
          const SizedBox(height: 22),
          Divider(color: s.line, height: 1),
          const SizedBox(height: 8),
          Align(
            alignment: Alignment.centerLeft,
            child: TextButton.icon(
              key: const Key('detail-trash'),
              onPressed: () => c.trashOrRestore(id),
              icon: Icon(
                r.deleted ? Icons.restore_from_trash : Icons.delete_outline,
                size: 18,
              ),
              label: Text(r.deleted ? 'Restore' : 'Move to Trash'),
              style: TextButton.styleFrom(
                foregroundColor: r.deleted ? s.accent : s.late,
              ),
            ),
          ),
        ],
      ),
    );
  }

  /// Pick the task this one goes under. "No parent" makes it top-level.
  Future<void> _pickParent(
    BuildContext context,
    String id,
    String? current,
  ) async {
    final rows = await c.run(() => c.db.parentCandidates(id: id));
    if (rows == null || !context.mounted) return;
    final picked = await showDialog<({String? id})>(
      context: context,
      builder: (_) => _ParentPicker(rows: rows, current: current),
    );
    if (picked == null || picked.id == current) return;
    await c.run(() => c.db.setParent(id: id, parentId: picked.id));
  }

  static String _path(List<NavItem> tags, NavItem t) {
    final parts = [t.name];
    var p = t.parentId;
    while (p != null) {
      final parent = tags.where((x) => x.id == p).firstOrNull;
      if (parent == null) break;
      parts.insert(0, parent.name);
      p = parent.parentId;
    }
    return parts.join('/');
  }
}

/// A text field that shows [value] and saves on Enter or when focus leaves.
/// [inline] drops the box: it reads as text and underlines when focused.
class _EditField extends StatefulWidget {
  const _EditField({
    super.key,
    required this.value,
    required this.onSave,
    required this.style,
    this.focus,
    this.hint = '',
    this.multiline = false,
    this.inline = false,
    this.minLines = 1,
    this.maxLines = 1,
  });

  final String value;
  final Future<void>? Function(String) onSave;
  final TextStyle style;
  final FocusNode? focus;
  final String hint;

  /// A wrapping single-paragraph field (Enter still saves).
  final bool multiline;
  final bool inline;
  final int minLines;
  final int maxLines;

  @override
  State<_EditField> createState() => _EditFieldState();
}

class _EditFieldState extends State<_EditField> {
  late final _text = TextEditingController(text: widget.value);
  late final FocusNode _focus = widget.focus ?? FocusNode();

  @override
  void initState() {
    super.initState();
    _focus.addListener(_onFocus);
  }

  @override
  void didUpdateWidget(_EditField old) {
    super.didUpdateWidget(old);
    // Take the new value unless the user is typing in this field.
    if (!_focus.hasFocus && widget.value != _text.text) {
      _text.text = widget.value;
    }
  }

  @override
  void dispose() {
    _focus.removeListener(_onFocus);
    if (widget.focus == null) _focus.dispose();
    _text.dispose();
    super.dispose();
  }

  void _onFocus() {
    if (!_focus.hasFocus) _save();
  }

  void _save() {
    if (_text.text != widget.value) widget.onSave(_text.text);
  }

  @override
  Widget build(BuildContext context) {
    final s = context.soft;
    final plain = InputDecoration(
      hintText: widget.hint,
      hintStyle: manrope(
        widget.style.fontSize ?? 13,
        FontWeight.w500,
        color: s.mute.withValues(alpha: 0.7),
      ),
      filled: false,
      border: InputBorder.none,
      enabledBorder: InputBorder.none,
      focusedBorder: widget.inline
          ? UnderlineInputBorder(borderSide: BorderSide(color: s.accent))
          : InputBorder.none,
      contentPadding: const EdgeInsets.symmetric(vertical: 4),
    );
    return TextField(
      controller: _text,
      focusNode: _focus,
      style: widget.style,
      minLines: widget.minLines,
      maxLines: widget.multiline ? widget.maxLines : 1,
      keyboardType: TextInputType.text,
      textInputAction: TextInputAction.done,
      onSubmitted: (_) => _save(),
      decoration: plain,
    );
  }
}

/// A date field's text box (typing like `fri 14:30` still works) with a button
/// that opens the date and time picker.
class _DateField extends StatelessWidget {
  const _DateField({
    required this.field,
    required this.pickKey,
    required this.title,
    required this.current,
    required this.other,
    required this.isStart,
    required this.onPicked,
  });

  final Widget field;
  final Key pickKey;
  final String title;
  final When? current;
  final WhenLimit? other;
  final bool isStart;
  final Future<void>? Function(PickedWhen) onPicked;

  @override
  Widget build(BuildContext context) {
    final s = context.soft;
    final touch = isTouch(context);
    return Row(
      children: [
        Expanded(child: field),
        IconButton(
          key: pickKey,
          tooltip: 'Pick a date',
          iconSize: 18,
          constraints: BoxConstraints.tightFor(
            width: touch ? 48 : 32,
            height: touch ? 48 : 32,
          ),
          padding: EdgeInsets.zero,
          icon: Icon(Icons.edit_calendar_outlined, color: s.accent),
          onPressed: () async {
            final picked = await showWhenPicker(
              context,
              title: title,
              isStart: isStart,
              initial: current,
              other: other,
            );
            if (picked != null) await onPicked(picked);
          },
        ),
      ],
    );
  }
}

/// A rounded group of property rows with hairlines between them.
class _Card extends StatelessWidget {
  const _Card({required this.children});

  final List<Widget> children;

  @override
  Widget build(BuildContext context) {
    final s = context.soft;
    return Container(
      decoration: BoxDecoration(
        color: s.bg,
        borderRadius: BorderRadius.circular(14),
        border: Border.all(color: s.line),
      ),
      child: Column(
        children: [
          for (var i = 0; i < children.length; i++) ...[
            if (i > 0) Divider(height: 1, indent: 40, color: s.line),
            children[i],
          ],
        ],
      ),
    );
  }
}

/// icon, label, value: one line of a [_Card].
class _PropRow extends StatelessWidget {
  const _PropRow({
    required this.icon,
    required this.label,
    required this.child,
  });

  final IconData icon;
  final String label;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    final s = context.soft;
    return ConstrainedBox(
      constraints: BoxConstraints(minHeight: isTouch(context) ? 48 : 42),
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 4),
        child: Row(
          children: [
            Icon(icon, size: 16, color: s.mute),
            const SizedBox(width: 12),
            SizedBox(
              width: 58,
              child: Text(
                label,
                style: manrope(12, FontWeight.w600, color: s.mute),
              ),
            ),
            Expanded(child: child),
          ],
        ),
      ),
    );
  }
}

/// A titled block: icon, name, an optional count and progress bar, then [child].
class _Section extends StatelessWidget {
  const _Section({
    required this.icon,
    required this.title,
    required this.child,
    this.trailing,
    this.progress,
  });

  final IconData icon;
  final String title;
  final Widget child;
  final String? trailing;
  final double? progress;

  @override
  Widget build(BuildContext context) {
    final s = context.soft;
    return Padding(
      padding: const EdgeInsets.only(top: 22),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Icon(icon, size: 16, color: s.mute),
              const SizedBox(width: 8),
              Text(title, style: manrope(13, FontWeight.w800, color: s.fg)),
              if (trailing != null) ...[
                const SizedBox(width: 8),
                Text(
                  trailing!,
                  style: manrope(11.5, FontWeight.w700, color: s.mute),
                ),
              ],
              if (progress != null) ...[
                const SizedBox(width: 10),
                Expanded(
                  child: ClipRRect(
                    borderRadius: BorderRadius.circular(3),
                    child: LinearProgressIndicator(
                      value: progress,
                      minHeight: 4,
                      backgroundColor: s.line,
                      color: s.accent,
                    ),
                  ),
                ),
              ],
            ],
          ),
          const SizedBox(height: 8),
          child,
        ],
      ),
    );
  }
}

/// Notes in Markdown: rendered until you click it, then a text box with a
/// formatting bar. Saves when it loses focus or on Done.
class _Notes extends StatefulWidget {
  const _Notes({super.key, required this.value, required this.onSave});

  final String value;
  final Future<void>? Function(String) onSave;

  @override
  State<_Notes> createState() => _NotesState();
}

class _NotesState extends State<_Notes> {
  final _text = TextEditingController();
  final _focus = FocusNode();
  bool _editing = false;
  late String _shown = widget.value;

  @override
  void initState() {
    super.initState();
    _focus.addListener(() {
      if (!_focus.hasFocus && _editing) _finish();
    });
  }

  @override
  void didUpdateWidget(_Notes old) {
    super.didUpdateWidget(old);
    if (widget.value != old.value && !_editing) _shown = widget.value;
  }

  @override
  void dispose() {
    _text.dispose();
    _focus.dispose();
    super.dispose();
  }

  void _edit() {
    _text.text = _shown;
    setState(() => _editing = true);
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted) return;
      _focus.requestFocus();
      _text.selection = TextSelection.collapsed(offset: _text.text.length);
    });
  }

  void _finish() {
    final v = _text.text;
    setState(() {
      _editing = false;
      _shown = v;
    });
    if (v != widget.value) widget.onSave(v);
  }

  /// Wrap the selection in [left]/[right], or insert them with the cursor between.
  void _wrap(String left, [String? right]) {
    right ??= left;
    final sel = _text.selection.isValid
        ? _text.selection
        : TextSelection.collapsed(offset: _text.text.length);
    final t = _text.text;
    final inner = t.substring(sel.start, sel.end);
    _text.value = TextEditingValue(
      text: t.replaceRange(sel.start, sel.end, '$left$inner$right'),
      selection: inner.isEmpty
          ? TextSelection.collapsed(offset: sel.start + left.length)
          : TextSelection(
              baseOffset: sel.start + left.length,
              extentOffset: sel.start + left.length + inner.length,
            ),
    );
    _focus.requestFocus();
  }

  /// Put [prefix] at the start of the cursor's line.
  void _prefix(String prefix) {
    final sel = _text.selection.isValid
        ? _text.selection
        : TextSelection.collapsed(offset: _text.text.length);
    final t = _text.text;
    final at = t.lastIndexOf('\n', sel.start == 0 ? 0 : sel.start - 1) + 1;
    final start = sel.start < at ? 0 : at;
    _text.value = TextEditingValue(
      text: t.replaceRange(start, start, prefix),
      selection: TextSelection.collapsed(offset: sel.end + prefix.length),
    );
    _focus.requestFocus();
  }

  @override
  Widget build(BuildContext context) {
    final s = context.soft;
    if (!_editing) {
      final empty = _shown.trim().isEmpty;
      return InkWell(
        borderRadius: BorderRadius.circular(12),
        onTap: _edit,
        child: Container(
          width: double.infinity,
          constraints: const BoxConstraints(minHeight: 64),
          padding: const EdgeInsets.all(12),
          decoration: BoxDecoration(
            color: s.bg,
            borderRadius: BorderRadius.circular(12),
            border: Border.all(color: s.line),
          ),
          child: empty
              ? Text(
                  'Add notes. Markdown works: **bold**, - lists, - [ ] tasks',
                  style: manrope(13, FontWeight.w500, color: s.mute),
                )
              : MarkdownView(
                  source: _shown,
                  onChanged: (v) {
                    setState(() => _shown = v);
                    widget.onSave(v);
                  },
                ),
        ),
      );
    }
    Widget tool(IconData i, String tip, VoidCallback f) => IconButton(
      tooltip: tip,
      visualDensity: VisualDensity.compact,
      iconSize: 17,
      icon: Icon(i, color: s.mute),
      onPressed: f,
    );
    return Container(
      decoration: BoxDecoration(
        color: s.bg,
        borderRadius: BorderRadius.circular(12),
        border: Border.all(color: s.accent, width: 1.5),
      ),
      child: Column(
        children: [
          TextFieldTapRegion(
            child: Focus(
              canRequestFocus: false,
              descendantsAreFocusable: false,
              child: Padding(
                padding: const EdgeInsets.fromLTRB(4, 2, 8, 0),
                child: Row(
                  children: [
                    tool(Icons.format_bold, 'Bold', () => _wrap('**')),
                    tool(Icons.format_italic, 'Italic', () => _wrap('*')),
                    tool(Icons.code, 'Code', () => _wrap('`')),
                    tool(Icons.title, 'Heading', () => _prefix('## ')),
                    tool(
                      Icons.format_list_bulleted,
                      'List',
                      () => _prefix('- '),
                    ),
                    tool(
                      Icons.check_box_outlined,
                      'Task',
                      () => _prefix('- [ ] '),
                    ),
                    tool(Icons.link, 'Link', () => _wrap('[', '](https://)')),
                    const Spacer(),
                    TextButton(
                      key: const Key('notes-done'),
                      onPressed: _finish,
                      style: TextButton.styleFrom(
                        visualDensity: VisualDensity.compact,
                      ),
                      child: const Text('Done'),
                    ),
                  ],
                ),
              ),
            ),
          ),
          Divider(height: 1, color: s.line),
          TextField(
            key: const Key('notes-field'),
            controller: _text,
            focusNode: _focus,
            minLines: 5,
            maxLines: 16,
            style: manrope(
              13,
              FontWeight.w500,
              color: s.fg,
            ).copyWith(height: 1.5),
            decoration: InputDecoration(
              hintText: 'Write in Markdown',
              filled: false,
              border: InputBorder.none,
              enabledBorder: InputBorder.none,
              focusedBorder: InputBorder.none,
              contentPadding: const EdgeInsets.all(12),
            ),
          ),
        ],
      ),
    );
  }
}

class _PriorityPicker extends StatelessWidget {
  const _PriorityPicker({required this.value, required this.onPick});

  final int value;
  final ValueChanged<int> onPick;

  @override
  Widget build(BuildContext context) {
    final s = context.soft;
    const names = ['None', 'Low', 'Med', 'High'];
    // One segmented control: the picked priority fills with its colour.
    return Container(
      padding: const EdgeInsets.all(3),
      decoration: BoxDecoration(
        color: s.side,
        borderRadius: BorderRadius.circular(11),
      ),
      child: Row(
        children: [
          for (var p = 0; p < 4; p++)
            Expanded(
              child: Semantics(
                selected: p == value,
                button: true,
                label: 'Priority ${names[p]}',
                child: InkWell(
                  key: Key('priority-$p'),
                  borderRadius: BorderRadius.circular(8),
                  onTap: () => onPick(p),
                  child: AnimatedContainer(
                    duration: motion(context, 120),
                    height: 32,
                    alignment: Alignment.center,
                    decoration: BoxDecoration(
                      borderRadius: BorderRadius.circular(8),
                      color: p == value
                          ? (p == 0
                                ? s.panel
                                : s.priority(p).withValues(alpha: 0.16))
                          : null,
                      border: p == value
                          ? Border.all(color: p == 0 ? s.line : s.priority(p))
                          : null,
                    ),
                    child: Row(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        if (p > 0) ...[
                          Icon(Icons.flag, size: 12, color: s.priority(p)),
                          const SizedBox(width: 3),
                        ],
                        Text(
                          names[p],
                          style: manrope(
                            12.5,
                            p == value ? FontWeight.w700 : FontWeight.w500,
                            color: p == 0 || p != value
                                ? (p == value ? s.fg : s.mute)
                                : s.priority(p),
                          ),
                        ),
                      ],
                    ),
                  ),
                ),
              ),
            ),
        ],
      ),
    );
  }
}

/// `+ tag` that turns into a field with tag-path suggestions.
class _AddTag extends StatefulWidget {
  const _AddTag({required this.paths, required this.onAdd});

  final List<String> paths;
  final ValueChanged<String> onAdd;

  @override
  State<_AddTag> createState() => _AddTagState();
}

class _AddTagState extends State<_AddTag> {
  bool _editing = false;

  @override
  Widget build(BuildContext context) {
    final s = context.soft;
    if (!_editing) {
      return InkWell(
        key: const Key('add-tag'),
        borderRadius: BorderRadius.circular(8),
        onTap: () => setState(() => _editing = true),
        child: Container(
          padding: EdgeInsets.symmetric(
            horizontal: 8,
            vertical: isTouch(context) ? 9 : 1,
          ),
          decoration: BoxDecoration(
            borderRadius: BorderRadius.circular(8),
            border: Border.all(color: s.line),
          ),
          child: Text(
            '+ tag',
            style: manrope(11, FontWeight.w600, color: s.mute),
          ),
        ),
      );
    }
    return SizedBox(
      width: 170,
      child: Autocomplete<String>(
        optionsBuilder: (v) {
          final q = v.text.toLowerCase();
          return q.isEmpty
              ? const []
              : widget.paths.where((p) => p.toLowerCase().contains(q)).take(8);
        },
        onSelected: _submit,
        fieldViewBuilder: (context, ctl, focus, onSubmit) => TextField(
          key: const Key('tag-field'),
          controller: ctl,
          focusNode: focus,
          autofocus: true,
          style: manrope(12, FontWeight.w500),
          decoration: const InputDecoration(hintText: 'tag or parent/child'),
          onSubmitted: (v) {
            onSubmit();
            _submit(v);
          },
          onTapOutside: (_) => setState(() => _editing = false),
        ),
      ),
    );
  }

  void _submit(String v) {
    final p = v.trim();
    if (p.isNotEmpty) widget.onAdd(p);
    setState(() => _editing = false);
  }
}

/// A borderless "+ Add …" line for checklist items and subtasks.
class _AddLine extends StatefulWidget {
  const _AddLine({super.key, required this.hint, required this.onAdd});

  final String hint;
  final ValueChanged<String> onAdd;

  @override
  State<_AddLine> createState() => _AddLineState();
}

class _AddLineState extends State<_AddLine> {
  final _text = TextEditingController();
  final _focus = FocusNode();

  @override
  void dispose() {
    _text.dispose();
    _focus.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final s = context.soft;
    return TextField(
      controller: _text,
      focusNode: _focus,
      style: manrope(13, FontWeight.w500),
      decoration: InputDecoration(
        hintText: '+ ${widget.hint}',
        filled: false,
        border: InputBorder.none,
        enabledBorder: InputBorder.none,
        focusedBorder: UnderlineInputBorder(
          borderSide: BorderSide(color: s.accent),
        ),
        contentPadding: const EdgeInsets.symmetric(vertical: 6, horizontal: 4),
      ),
      onSubmitted: (v) {
        if (v.trim().isNotEmpty) widget.onAdd(v.trim());
        _text.clear();
        _focus.requestFocus();
      },
    );
  }
}

/// Repeat presets, plus whatever RRULE the task already has, and "from completion".
class _RepeatPicker extends StatelessWidget {
  const _RepeatPicker({
    required this.rule,
    required this.afterCompletion,
    required this.onSet,
  });

  final String? rule;
  final bool afterCompletion;
  final void Function(String? rule, bool afterCompletion) onSet;

  static const presets = {
    'FREQ=DAILY': 'Every day',
    'FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR': 'Every weekday',
    'FREQ=WEEKLY': 'Every week',
    'FREQ=WEEKLY;INTERVAL=2': 'Every 2 weeks',
    'FREQ=MONTHLY': 'Every month',
    'FREQ=YEARLY': 'Every year',
  };

  @override
  Widget build(BuildContext context) {
    final s = context.soft;
    final current = rule;
    final items = <String, String>{
      '': 'Never',
      ...presets,
      if (current != null && !presets.containsKey(current)) current: current,
      '…': 'Custom rule…',
    };
    // The dropdown is as wide as its longest rule, so cap it to the room the pane has.
    return LayoutBuilder(
      builder: (context, box) => Wrap(
        crossAxisAlignment: WrapCrossAlignment.center,
        spacing: 8,
        children: [
          SizedBox(
            width: box.maxWidth.clamp(0, 170).toDouble(),
            child: DropdownButton<String>(
              key: const Key('detail-repeat'),
              value: current ?? '',
              isDense: true,
              isExpanded: true,
              underline: const SizedBox.shrink(),
              style: manrope(13, FontWeight.w600, color: s.fg),
              items: [
                for (final e in items.entries)
                  DropdownMenuItem(
                    value: e.key,
                    child: Text(e.value, overflow: TextOverflow.ellipsis),
                  ),
              ],
              onChanged: (v) async {
                if (v == null) return;
                if (v == '…') {
                  final custom = await askText(
                    context,
                    title: 'Repeat rule (RRULE)',
                    hint: 'FREQ=MONTHLY;BYMONTHDAY=1',
                    initial: current ?? '',
                  );
                  if (custom != null && custom.trim().isNotEmpty) {
                    onSet(custom.trim(), afterCompletion);
                  }
                  return;
                }
                onSet(v.isEmpty ? null : v, afterCompletion);
              },
            ),
          ),
          if (current != null)
            Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                SizedBox(
                  width: 28,
                  child: Checkbox(
                    key: const Key('repeat-after'),
                    value: afterCompletion,
                    visualDensity: VisualDensity.compact,
                    onChanged: (v) => onSet(current, v ?? false),
                  ),
                ),
                Text(
                  'from completion',
                  style: manrope(12, FontWeight.w500, color: s.mute),
                ),
              ],
            ),
        ],
      ),
    );
  }
}

/// "+ Add a reminder": common offsets, or a typed date and time.
class _AddReminder extends StatelessWidget {
  const _AddReminder({required this.onBefore, required this.onAt});

  final ValueChanged<int> onBefore;
  final ValueChanged<String> onAt;

  static const offsets = {
    0: 'At the time',
    5: '5 minutes before',
    15: '15 minutes before',
    30: '30 minutes before',
    60: '1 hour before',
    1440: '1 day before',
  };

  @override
  Widget build(BuildContext context) {
    final s = context.soft;
    return Align(
      alignment: Alignment.centerLeft,
      child: PopupMenuButton<int>(
        key: const Key('add-reminder'),
        tooltip: 'Add a reminder',
        itemBuilder: (_) => [
          for (final e in offsets.entries)
            PopupMenuItem(value: e.key, child: Text(e.value)),
          const PopupMenuDivider(),
          const PopupMenuItem(value: -1, child: Text('At a date and time…')),
        ],
        onSelected: (m) async {
          if (m >= 0) return onBefore(m);
          final text = await askText(
            context,
            title: 'Remind me at',
            hint: 'tomorrow 09:00',
          );
          if (text != null && text.trim().isNotEmpty) onAt(text.trim());
        },
        child: Padding(
          padding: const EdgeInsets.symmetric(vertical: 6, horizontal: 4),
          child: Text(
            '+ Add a reminder',
            style: manrope(13, FontWeight.w500, color: s.mute),
          ),
        ),
      ),
    );
  }
}

/// The Parent picker: open tasks this one may go under, with a search field.
/// Pops `(id: null)` for "No parent".
class _ParentPicker extends StatefulWidget {
  const _ParentPicker({required this.rows, required this.current});

  final List<TaskRow> rows;
  final String? current;

  @override
  State<_ParentPicker> createState() => _ParentPickerState();
}

class _ParentPickerState extends State<_ParentPicker> {
  String _query = '';

  @override
  Widget build(BuildContext context) {
    final s = context.soft;
    final q = _query.trim().toLowerCase();
    final rows = [
      for (final r in widget.rows)
        if (q.isEmpty || r.title.toLowerCase().contains(q)) r,
    ];
    Widget option(String? id, String title, {String? sub, Key? key}) =>
        ListTile(
          key: key,
          dense: true,
          selected: id == widget.current,
          title: Text(
            title,
            overflow: TextOverflow.ellipsis,
            style: manrope(13, FontWeight.w600, color: s.fg),
          ),
          subtitle: sub == null
              ? null
              : Text(sub, style: manrope(11.5, FontWeight.w500, color: s.mute)),
          onTap: () => Navigator.pop(context, (id: id)),
        );
    return AlertDialog(
      title: Text('Parent task', style: manrope(16, FontWeight.w800)),
      contentPadding: const EdgeInsets.fromLTRB(16, 12, 16, 0),
      content: SizedBox(
        width: 380,
        height: 420,
        child: Column(
          children: [
            TextField(
              key: const Key('parent-search'),
              autofocus: true,
              decoration: const InputDecoration(
                hintText: 'Search tasks',
                prefixIcon: Icon(Icons.search, size: 18),
                isDense: true,
              ),
              onChanged: (v) => setState(() => _query = v),
            ),
            const SizedBox(height: 8),
            Expanded(
              child: ListView(
                children: [
                  if (q.isEmpty)
                    option(
                      null,
                      'No parent',
                      sub: 'A top-level task',
                      key: const Key('parent-none'),
                    ),
                  for (final r in rows)
                    option(
                      r.id,
                      r.title,
                      sub: r.listName,
                      key: Key('parent-${r.title}'),
                    ),
                  if (rows.isEmpty && q.isNotEmpty)
                    Padding(
                      padding: const EdgeInsets.all(16),
                      child: Text(
                        'No task matches',
                        style: manrope(13, FontWeight.w500, color: s.mute),
                      ),
                    ),
                ],
              ),
            ),
          ],
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context),
          child: const Text('Cancel'),
        ),
      ],
    );
  }
}
