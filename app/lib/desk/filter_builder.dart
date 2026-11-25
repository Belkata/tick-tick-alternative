import 'dart:async';

// ignore: deprecated_member_use
import 'package:flutter/material.dart';

import '../src/rust/api/tasks.dart';
import 'controller.dart';
import 'filter_rule.dart';
import 'theme.dart';

/// The visual filter builder (plan/01: AND / OR / NOT with nested groups). Creates a filter,
/// or edits [filterId]; on save it opens the filter's view.
Future<void> showFilterBuilder(
  BuildContext context,
  DeskController c, {
  String? filterId,
}) async {
  var def = FilterDef();
  if (filterId != null) {
    final json = await c.run(() => c.db.filterJson(id: filterId));
    if (json == null) return;
    def = FilterDef.fromJson(json);
  }
  if (!context.mounted) return;
  await showDialog<void>(
    context: context,
    builder: (_) => _FilterBuilder(c: c, def: def, filterId: filterId),
  );
}

/// The sorts the builder's dropdown offers; anything else shows as Default.
const _sortItems = {
  'due_asc',
  'priority_desc',
  'created_desc',
  'created_asc',
  'title_asc',
  'manual',
};

class _FilterBuilder extends StatefulWidget {
  const _FilterBuilder({required this.c, required this.def, this.filterId});

  final DeskController c;
  final FilterDef def;
  final String? filterId;

  @override
  State<_FilterBuilder> createState() => _FilterBuilderState();
}

class _FilterBuilderState extends State<_FilterBuilder> {
  late final _name = TextEditingController(text: widget.def.name);
  late final _icon = TextEditingController(text: widget.def.icon ?? '');
  Timer? _debounce;
  String _count = '';
  bool _countOk = true;
  String? _error;

  FilterDef get def => widget.def;

  @override
  void initState() {
    super.initState();
    _recount();
  }

  @override
  void dispose() {
    _debounce?.cancel();
    _name.dispose();
    _icon.dispose();
    super.dispose();
  }

  /// Something in the tree changed: redraw and refresh the live count shortly.
  void _changed() {
    setState(() {});
    _debounce?.cancel();
    _debounce = Timer(const Duration(milliseconds: 250), _recount);
  }

  Future<void> _recount() async {
    try {
      final n = await widget.c.db.ruleCount(ruleJson: def.ruleJson());
      if (!mounted) return;
      setState(() {
        _countOk = true;
        _count = n == 1 ? '1 open task matches' : '$n open tasks match';
      });
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _countOk = false;
        _count = _clean(e);
      });
    }
  }

  Future<void> _save() async {
    def
      ..name = _name.text
      ..icon = _icon.text;
    final db = widget.c.db;
    try {
      final id = widget.filterId;
      if (id == null) {
        final newId = await db.saveFilter(json: def.toJson());
        widget.c.open(TaskView(kind: ViewKind.filter, id: newId));
      } else {
        await db.updateFilter(id: id, json: def.toJson());
      }
      if (mounted) Navigator.of(context).pop();
    } catch (e) {
      setState(() => _error = _clean(e));
    }
  }

  Future<void> _delete() async {
    final id = widget.filterId!;
    final ok = await confirm(
      context,
      title: 'Delete "${def.name}"?',
      body: 'The filter goes away; its tasks stay.',
    );
    if (!ok || !mounted) return;
    try {
      // Leave its view first, so the reload after the delete doesn't ask for it.
      if (widget.c.view.kind == ViewKind.filter && widget.c.view.id == id) {
        widget.c.open(const TaskView(kind: ViewKind.inbox));
      }
      await widget.c.db.deleteFilter(id: id);
      if (mounted) Navigator.of(context).pop();
    } catch (e) {
      setState(() => _error = _clean(e));
    }
  }

  @override
  Widget build(BuildContext context) {
    final s = context.soft;
    final sb = widget.c.sidebar;
    final ctx = _Ctx(
      tagPaths: [
        for (final t in sb?.tags ?? const <NavItem>[]) tagPath(sb!.tags, t),
      ],
      lists: sb?.lists ?? const [],
      changed: _changed,
    );
    return AlertDialog(
      title: Text(
        widget.filterId == null ? 'New filter' : 'Edit filter',
        style: manrope(17, FontWeight.w800),
      ),
      content: SizedBox(
        width: 640,
        child: SingleChildScrollView(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            mainAxisSize: MainAxisSize.min,
            children: [
              Row(
                children: [
                  Expanded(
                    child: TextField(
                      key: const Key('filter-name'),
                      controller: _name,
                      autofocus: widget.filterId == null,
                      decoration: const InputDecoration(
                        hintText: 'Name, e.g. Today + Inbox',
                      ),
                      onChanged: (_) => setState(() {}),
                    ),
                  ),
                  const SizedBox(width: 8),
                  SizedBox(
                    width: 72,
                    child: TextField(
                      controller: _icon,
                      textAlign: TextAlign.center,
                      decoration: const InputDecoration(hintText: 'Icon'),
                    ),
                  ),
                ],
              ),
              const SizedBox(height: 14),
              _GroupEditor(group: def.rule, ctx: ctx, depth: 0),
              const SizedBox(height: 10),
              Text(
                _count,
                key: const Key('filter-count'),
                style: manrope(
                  12.5,
                  FontWeight.w600,
                  color: _countOk ? s.today : s.late,
                ),
              ),
              const SizedBox(height: 10),
              Wrap(
                spacing: 16,
                runSpacing: 4,
                crossAxisAlignment: WrapCrossAlignment.center,
                children: [
                  Row(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      Checkbox(
                        value: def.showCompleted,
                        onChanged: (v) =>
                            setState(() => def.showCompleted = v ?? false),
                      ),
                      Text(
                        'Include completed',
                        style: manrope(13, FontWeight.w500),
                      ),
                    ],
                  ),
                  Row(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      Text(
                        'Sort ',
                        style: manrope(13, FontWeight.w500, color: s.mute),
                      ),
                      DropdownButton<String>(
                        value: _sortItems.contains(def.sort.firstOrNull)
                            ? def.sort.first
                            : '',
                        isDense: true,
                        underline: const SizedBox.shrink(),
                        style: manrope(13, FontWeight.w600, color: s.fg),
                        items: const [
                          DropdownMenuItem(value: '', child: Text('Default')),
                          DropdownMenuItem(
                            value: 'due_asc',
                            child: Text('Due date'),
                          ),
                          DropdownMenuItem(
                            value: 'priority_desc',
                            child: Text('Priority'),
                          ),
                          DropdownMenuItem(
                            value: 'created_desc',
                            child: Text('Newest first'),
                          ),
                          DropdownMenuItem(
                            value: 'created_asc',
                            child: Text('Oldest first'),
                          ),
                          DropdownMenuItem(
                            value: 'title_asc',
                            child: Text('Title'),
                          ),
                          DropdownMenuItem(
                            value: 'manual',
                            child: Text('Manual'),
                          ),
                        ],
                        onChanged: (v) => setState(
                          () => def.sort = (v == null || v.isEmpty) ? [] : [v],
                        ),
                      ),
                    ],
                  ),
                ],
              ),
              if (_error != null)
                Padding(
                  padding: const EdgeInsets.only(top: 8),
                  child: Text(
                    _error!,
                    style: manrope(12.5, FontWeight.w600, color: s.late),
                  ),
                ),
            ],
          ),
        ),
      ),
      actions: [
        if (widget.filterId != null)
          TextButton(
            key: const Key('filter-delete'),
            onPressed: _delete,
            style: TextButton.styleFrom(foregroundColor: s.late),
            child: const Text('Delete'),
          ),
        TextButton(
          onPressed: () => Navigator.of(context).pop(),
          child: const Text('Cancel'),
        ),
        FilledButton(
          key: const Key('filter-save'),
          onPressed: _name.text.trim().isEmpty ? null : _save,
          child: const Text('Save'),
        ),
      ],
    );
  }
}

/// What the editors need from the dialog.
class _Ctx {
  const _Ctx({
    required this.tagPaths,
    required this.lists,
    required this.changed,
  });

  final List<String> tagPaths;
  final List<NavItem> lists;
  final VoidCallback changed;
}

/// A group: "Match all/any of:" its children, which may be groups themselves.
class _GroupEditor extends StatelessWidget {
  const _GroupEditor({
    required this.group,
    required this.ctx,
    required this.depth,
    this.onRemove,
  });

  final RuleGroup group;
  final _Ctx ctx;
  final int depth;
  final VoidCallback? onRemove;

  @override
  Widget build(BuildContext context) {
    final s = context.soft;
    return Container(
      padding: const EdgeInsets.fromLTRB(10, 8, 6, 6),
      decoration: BoxDecoration(
        borderRadius: BorderRadius.circular(12),
        border: Border.all(
          color: depth == 0 ? s.line : s.accent.withValues(alpha: 0.4),
        ),
        color: depth == 0 ? null : s.accent.withValues(alpha: 0.04),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            children: [
              _NotToggle(node: group, ctx: ctx),
              const SizedBox(width: 6),
              Text('Match', style: manrope(13, FontWeight.w600)),
              const SizedBox(width: 8),
              SegmentedButton<bool>(
                key: depth == 0 ? const Key('group-mode') : null,
                showSelectedIcon: false,
                style: const ButtonStyle(visualDensity: VisualDensity.compact),
                segments: const [
                  ButtonSegment(value: true, label: Text('all')),
                  ButtonSegment(value: false, label: Text('any')),
                ],
                selected: {group.all},
                onSelectionChanged: (v) {
                  group.all = v.first;
                  ctx.changed();
                },
              ),
              const SizedBox(width: 8),
              Text('of these', style: manrope(13, FontWeight.w600)),
              const Spacer(),
              if (onRemove != null)
                IconButton(
                  tooltip: 'Remove group',
                  icon: Icon(Icons.close, size: 16, color: s.mute),
                  onPressed: onRemove,
                ),
            ],
          ),
          const SizedBox(height: 6),
          for (final child in group.children)
            Padding(
              key: ObjectKey(child),
              padding: const EdgeInsets.only(bottom: 6),
              child: switch (child) {
                RuleCond() => _CondRow(
                  cond: child,
                  ctx: ctx,
                  onRemove: () {
                    group.children.remove(child);
                    ctx.changed();
                  },
                ),
                RuleGroup() => _GroupEditor(
                  group: child,
                  ctx: ctx,
                  depth: depth + 1,
                  onRemove: () {
                    group.children.remove(child);
                    ctx.changed();
                  },
                ),
              },
            ),
          Row(
            children: [
              TextButton.icon(
                key: depth == 0 ? const Key('add-condition') : null,
                icon: const Icon(Icons.add, size: 16),
                label: const Text('Condition'),
                onPressed: () {
                  group.children.add(RuleCond.of(RuleField.tag));
                  ctx.changed();
                },
              ),
              if (depth < 2)
                TextButton.icon(
                  key: depth == 0 ? const Key('add-group') : null,
                  icon: const Icon(Icons.account_tree_outlined, size: 16),
                  label: const Text('Group'),
                  onPressed: () {
                    group.children.add(
                      RuleGroup(
                        all: !group.all,
                        children: [RuleCond.of(RuleField.tag)],
                      ),
                    );
                    ctx.changed();
                  },
                ),
            ],
          ),
        ],
      ),
    );
  }
}

/// A small "not" chip that negates a condition or group.
class _NotToggle extends StatelessWidget {
  const _NotToggle({required this.node, required this.ctx});

  final RuleNode node;
  final _Ctx ctx;

  @override
  Widget build(BuildContext context) {
    final s = context.soft;
    final on = node.negate;
    return Tooltip(
      message: on ? 'Negated: matches when this is false' : 'Negate (NOT)',
      child: InkWell(
        borderRadius: BorderRadius.circular(8),
        onTap: () {
          node.negate = !on;
          ctx.changed();
        },
        child: Container(
          padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 3),
          decoration: BoxDecoration(
            borderRadius: BorderRadius.circular(8),
            color: on ? s.late.withValues(alpha: 0.14) : null,
            border: Border.all(color: on ? s.late : s.line),
          ),
          child: Text(
            'not',
            style: manrope(11.5, FontWeight.w700, color: on ? s.late : s.mute),
          ),
        ),
      ),
    );
  }
}

/// One condition: [not] [field] [operator] [value] [✕].
class _CondRow extends StatefulWidget {
  const _CondRow({
    required this.cond,
    required this.ctx,
    required this.onRemove,
  });

  final RuleCond cond;
  final _Ctx ctx;
  final VoidCallback onRemove;

  @override
  State<_CondRow> createState() => _CondRowState();
}

class _CondRowState extends State<_CondRow> {
  late final _text = TextEditingController(text: _valueText);

  RuleCond get cond => widget.cond;

  String get _valueText => cond.value is String ? cond.value as String : '';

  @override
  void dispose() {
    _text.dispose();
    super.dispose();
  }

  void _set(VoidCallback f) {
    f();
    if (_text.text != _valueText) _text.text = _valueText;
    widget.ctx.changed();
  }

  @override
  Widget build(BuildContext context) {
    final s = context.soft;
    final style = manrope(13, FontWeight.w600, color: s.fg);
    return Row(
      crossAxisAlignment: CrossAxisAlignment.center,
      children: [
        _NotToggle(node: cond, ctx: widget.ctx),
        const SizedBox(width: 6),
        DropdownButton<RuleField>(
          key: const Key('cond-field'),
          value: cond.field,
          isDense: true,
          style: style,
          underline: const SizedBox.shrink(),
          items: [
            for (final f in RuleField.values)
              DropdownMenuItem(value: f, child: Text(f.label)),
          ],
          onChanged: (f) => f == null ? null : _set(() => cond.setField(f)),
        ),
        const SizedBox(width: 8),
        DropdownButton<String>(
          key: const Key('cond-op'),
          value: cond.op,
          isDense: true,
          style: style,
          underline: const SizedBox.shrink(),
          items: [
            for (final o in cond.field.ops)
              DropdownMenuItem(value: o, child: Text(opLabel(o))),
          ],
          onChanged: (o) => o == null ? null : _set(() => cond.setOp(o)),
        ),
        const SizedBox(width: 8),
        Expanded(
          child: opNeedsValue(cond.op)
              ? _value(context)
              : const SizedBox.shrink(),
        ),
        IconButton(
          tooltip: 'Remove',
          icon: Icon(Icons.close, size: 16, color: s.mute),
          onPressed: widget.onRemove,
        ),
      ],
    );
  }

  Widget _value(BuildContext context) {
    final s = context.soft;
    final style = manrope(13, FontWeight.w600, color: s.fg);
    Widget pick<T>(T? value, Map<T, String> options, {Key? key}) => Align(
      alignment: Alignment.centerLeft,
      child: DropdownButton<T>(
        key: key,
        value: options.containsKey(value) ? value : null,
        hint: Text(
          'Choose…',
          style: manrope(13, FontWeight.w500, color: s.mute),
        ),
        isDense: true,
        style: style,
        underline: const SizedBox.shrink(),
        items: [
          for (final e in options.entries)
            DropdownMenuItem(value: e.key, child: Text(e.value)),
        ],
        onChanged: (v) => _set(() => cond.value = v),
      ),
    );
    Widget text(
      String hint, {
      List<String> suggestions = const [],
      bool append = false,
    }) => Row(
      children: [
        Expanded(
          child: TextField(
            key: const Key('cond-value'),
            controller: _text,
            style: manrope(13, FontWeight.w500),
            decoration: InputDecoration(hintText: hint),
            onChanged: (v) {
              cond.value = v;
              widget.ctx.changed();
            },
          ),
        ),
        if (suggestions.isNotEmpty)
          PopupMenuButton<String>(
            tooltip: 'Pick',
            icon: Icon(Icons.arrow_drop_down, color: s.mute),
            itemBuilder: (_) => [
              for (final o in suggestions)
                PopupMenuItem(value: o, child: Text(o)),
            ],
            onSelected: (o) => _set(() {
              final cur = _valueText.trim();
              cond.value = append && cur.isNotEmpty ? '$cur, $o' : o;
            }),
          ),
      ],
    );

    const dates = [
      'today',
      'tomorrow',
      'yesterday',
      '+7d',
      'this_week',
      'next_week',
      'this_month',
    ];
    return switch (cond.field) {
      RuleField.tag => text(
        cond.op == 'has' ? 'tag, e.g. home/garden' : 'tags, comma-separated',
        suggestions: widget.ctx.tagPaths,
        append: cond.op != 'has',
      ),
      RuleField.list => pick<String>(cond.value as String?, {
        for (final l in widget.ctx.lists) l.id: l.name,
      }),
      RuleField.inbox || RuleField.hasTime || RuleField.repeating => pick<bool>(
        cond.value as bool?,
        {true: 'yes', false: 'no'},
      ),
      RuleField.priority => pick<int>(cond.value as int?, {
        0: 'None',
        1: 'Low',
        2: 'Medium',
        3: 'High',
      }),
      RuleField.status => pick<String>(cond.value as String?, {
        'open': 'open',
        'done': 'done',
        'deleted': 'in Trash',
      }),
      RuleField.due || RuleField.start => text(
        cond.op == 'between'
            ? 'from, to — e.g. today, +7d'
            : 'today, +7d, 2026-12-01',
        suggestions: dates,
        append: cond.op == 'between',
      ),
      RuleField.text => text('words'),
    };
  }
}

/// A tag's full path (`home/garden`) from the sidebar's flat list.
String tagPath(List<NavItem> tags, NavItem t) {
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

/// A core error message without the bridge's wrapping.
String _clean(Object e) => '$e'
    .replaceFirst(RegExp(r'^(AnyhowException|Exception)\(?'), '')
    .replaceFirst(RegExp(r'\)$'), '');

/// A yes/no dialog. True when confirmed.
Future<bool> confirm(
  BuildContext context, {
  required String title,
  required String body,
  String action = 'Delete',
}) async {
  final ok = await showDialog<bool>(
    context: context,
    builder: (ctx) => AlertDialog(
      title: Text(title, style: manrope(16, FontWeight.w800)),
      content: Text(body),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(ctx, false),
          child: const Text('Cancel'),
        ),
        FilledButton(
          key: const Key('confirm'),
          onPressed: () => Navigator.pop(ctx, true),
          child: Text(action),
        ),
      ],
    ),
  );
  return ok ?? false;
}
