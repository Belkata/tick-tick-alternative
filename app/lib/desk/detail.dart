// ignore: deprecated_member_use
import 'package:flutter/material.dart';

import '../src/rust/api/tasks.dart';
import 'controller.dart';
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
        child: Text(
          'Select a task',
          style: manrope(13.5, FontWeight.w500, color: s.mute),
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

    Widget label(String text) => Padding(
      padding: const EdgeInsets.only(top: 6),
      child: Text(text, style: manrope(12, FontWeight.w600, color: s.mute)),
    );
    Widget prop(String k, Widget v) => Padding(
      padding: const EdgeInsets.only(bottom: 8),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          SizedBox(width: 62, child: label(k)),
          Expanded(child: v),
        ],
      ),
    );
    Widget heading(String text) => Padding(
      padding: const EdgeInsets.only(top: 14, bottom: 6),
      child: Text(text, style: manrope(12, FontWeight.w700, color: s.mute)),
    );

    return Container(
      color: s.panel,
      child: ListView(
        key: ValueKey('detail-$id'),
        padding: const EdgeInsets.fromLTRB(18, 16, 18, 24),
        children: [
          Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Padding(
                padding: const EdgeInsets.only(top: 4),
                child: PriorityCheck(
                  priority: r.priority,
                  done: r.done,
                  onTap: () => c.toggleDone(id),
                ),
              ),
              const SizedBox(width: 6),
              Expanded(
                child: _EditField(
                  key: const Key('detail-title'),
                  value: r.title,
                  focus: titleFocus,
                  style: manrope(
                    17,
                    FontWeight.w800,
                    color: r.done ? s.mute : s.fg,
                  ),
                  onSave: (v) => v.trim().isEmpty
                      ? null
                      : c.run(() => db.setTitle(id: id, title: v.trim())),
                ),
              ),
            ],
          ),
          const SizedBox(height: 10),
          prop(
            'Priority',
            _PriorityPicker(
              value: r.priority,
              onPick: (p) => c.run(() => db.setPriority(id: id, priority: p)),
            ),
          ),
          prop(
            'Due',
            _EditField(
              key: const Key('detail-due'),
              value: r.dueLabel ?? '',
              hint: 'fri 14:30',
              style: manrope(
                13,
                FontWeight.w600,
                color: r.overdue ? s.late : (r.dueToday ? s.today : s.fg),
              ),
              boxed: true,
              onSave: (v) => c.run(() => db.setDue(id: id, text: v)),
            ),
          ),
          if (d.startLabel != null) prop('Start', label(d.startLabel!)),
          if (d.repeatRule != null) prop('Repeats', label(d.repeatRule!)),
          prop(
            'List',
            Align(
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
          prop(
            'Tags',
            Wrap(
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
          heading('Checklist'),
          for (final item in d.checklist)
            Row(
              children: [
                PriorityCheck(
                  priority: 0,
                  done: item.done,
                  size: 14,
                  onTap: () => c.run(
                    () => db.checklistSet(itemId: item.id, done: !item.done),
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
            onAdd: (v) => c.run(() => db.checklistAdd(taskId: id, title: v)),
          ),
          heading('Subtasks'),
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
            onAdd: (v) => c.run(() => db.addSubtask(parentId: id, title: v)),
          ),
          if (d.reminders.isNotEmpty) ...[
            heading('Reminders'),
            for (final rem in d.reminders)
              Padding(
                padding: const EdgeInsets.only(bottom: 4),
                child: Row(
                  children: [
                    Icon(Icons.notifications_none, size: 14, color: s.mute),
                    const SizedBox(width: 6),
                    Text(
                      rem.label,
                      style: manrope(13, FontWeight.w500, color: s.fg),
                    ),
                  ],
                ),
              ),
          ],
          heading('Notes'),
          _EditField(
            key: const Key('detail-notes'),
            value: d.notes,
            hint: 'Notes',
            multiline: true,
            boxed: true,
            style: manrope(13, FontWeight.w500, color: s.fg),
            onSave: (v) => c.run(() => db.setNotes(id: id, notes: v)),
          ),
          const SizedBox(height: 18),
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
class _EditField extends StatefulWidget {
  const _EditField({
    super.key,
    required this.value,
    required this.onSave,
    required this.style,
    this.focus,
    this.hint = '',
    this.multiline = false,
    this.boxed = false,
  });

  final String value;
  final Future<void>? Function(String) onSave;
  final TextStyle style;
  final FocusNode? focus;
  final String hint;
  final bool multiline;
  final bool boxed;

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
    return TextField(
      controller: _text,
      focusNode: _focus,
      style: widget.style,
      minLines: widget.multiline ? 3 : 1,
      maxLines: widget.multiline ? 12 : 1,
      textInputAction: widget.multiline
          ? TextInputAction.newline
          : TextInputAction.done,
      onSubmitted: widget.multiline ? null : (_) => _save(),
      decoration: widget.boxed
          ? InputDecoration(hintText: widget.hint, fillColor: s.bg)
          : InputDecoration(
              hintText: widget.hint,
              filled: false,
              border: InputBorder.none,
              enabledBorder: InputBorder.none,
              focusedBorder: InputBorder.none,
              contentPadding: const EdgeInsets.symmetric(vertical: 4),
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
    return Wrap(
      spacing: 4,
      runSpacing: 4,
      children: [
        for (var p = 0; p < 4; p++)
          InkWell(
            key: Key('priority-$p'),
            borderRadius: BorderRadius.circular(8),
            onTap: () => onPick(p),
            child: Container(
              padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 3),
              decoration: BoxDecoration(
                borderRadius: BorderRadius.circular(8),
                color: p == value
                    ? s.priority(p).withValues(alpha: 0.14)
                    : null,
                border: Border.all(color: p == value ? s.priority(p) : s.line),
              ),
              child: Text(
                names[p],
                style: manrope(
                  12,
                  p == value ? FontWeight.w700 : FontWeight.w500,
                  color: p == 0 ? s.fg : s.priority(p),
                ),
              ),
            ),
          ),
      ],
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
          padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 1),
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
