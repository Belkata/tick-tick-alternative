// ignore: deprecated_member_use
import 'package:flutter/material.dart';

import '../src/rust/api/tasks.dart';
import 'controller.dart';
import 'filter_builder.dart';
import 'style_dialog.dart';
import 'theme.dart';

/// Inbox, Lists, Filters, the Tags tree, Completed and Trash (plan/08).
class DeskSidebar extends StatelessWidget {
  const DeskSidebar({
    super.key,
    required this.c,
    this.onPicked,
    this.extras = const [],
  });

  final DeskController c;

  /// Called after a view is picked (the narrow layout closes its drawer).
  final VoidCallback? onPicked;

  /// Entries under the views, e.g. the calendar and sync spike screens.
  final List<Widget> extras;

  @override
  Widget build(BuildContext context) {
    final s = context.soft;
    final sb = c.sidebar;

    Widget nav(
      TaskView v,
      String label, {
      Widget? lead,
      int? count,
      int depth = 0,
      Key? key,
      VoidCallback? onEdit,
    }) => NavTile(
      key: key,
      label: label,
      lead: lead,
      count: count,
      depth: depth,
      selected: c.view == v,
      onTap: () {
        c.open(v);
        onPicked?.call();
      },
      onEdit: onEdit,
    );

    Widget icon(IconData i) => Icon(i, size: 16, color: s.mute);
    Widget dot(String? color) => Container(
      width: 8,
      height: 8,
      decoration: BoxDecoration(
        color: hexColor(color, s.mute),
        shape: BoxShape.circle,
      ),
    );
    Widget emojiOr(String? emoji, Widget fallback) => emoji == null
        ? fallback
        : Text(emoji, style: const TextStyle(fontSize: 13));

    Widget section(String label, {Widget? action}) => Padding(
      padding: const EdgeInsets.fromLTRB(16, 14, 10, 4),
      child: Row(
        children: [
          Expanded(
            child: Text(
              label,
              style: manrope(11, FontWeight.w700, color: s.mute),
            ),
          ),
          ?action,
        ],
      ),
    );

    return Material(
      color: s.side,
      child: ListView(
        padding: const EdgeInsets.symmetric(vertical: 12),
        children: [
          Padding(
            padding: const EdgeInsets.fromLTRB(16, 4, 12, 10),
            child: Row(
              children: [
                Expanded(
                  child: Text(
                    'Tick Alt',
                    style: manrope(17, FontWeight.w800, color: s.fg),
                  ),
                ),
                Tooltip(
                  message: c.db.isRemote()
                      ? 'Connected to tt-daemon'
                      : 'Database opened directly (no daemon)',
                  child: Icon(
                    c.db.isRemote() ? Icons.link : Icons.link_off,
                    key: const Key('backend-indicator'),
                    size: 15,
                    color: c.db.isRemote() ? s.accent : s.mute,
                  ),
                ),
              ],
            ),
          ),
          nav(
            const TaskView(kind: ViewKind.inbox),
            'Inbox',
            key: const Key('nav-inbox'),
            lead: icon(Icons.inbox_outlined),
            count: sb?.inbox,
          ),
          section(
            'Lists',
            action: _SmallButton(
              key: const Key('new-list'),
              icon: Icons.add,
              tooltip: 'New list',
              onTap: () => _newList(context),
            ),
          ),
          for (final l in sb?.lists ?? const <NavItem>[])
            nav(
              TaskView(kind: ViewKind.list, id: l.id),
              l.name,
              lead: emojiOr(
                l.icon,
                l.color == null
                    ? icon(Icons.format_list_bulleted)
                    : Icon(
                        Icons.format_list_bulleted,
                        size: 16,
                        color: hexColor(l.color, s.mute),
                      ),
              ),
              count: l.count,
              key: Key('nav-list-${l.name}'),
              onEdit: () => showStyleDialog(context, c, item: l, isTag: false),
            ),
          section(
            'Filters',
            action: _SmallButton(
              key: const Key('new-filter'),
              icon: Icons.add,
              tooltip: 'New filter',
              onTap: () => showFilterBuilder(context, c),
            ),
          ),
          for (final f in sb?.filters ?? const <NavItem>[])
            nav(
              TaskView(kind: ViewKind.filter, id: f.id),
              f.name,
              lead: emojiOr(f.icon, icon(Icons.filter_alt_outlined)),
              count: f.count,
              key: Key('nav-filter-${f.name}'),
              onEdit: () => showFilterBuilder(context, c, filterId: f.id),
            ),
          if ((sb?.tags ?? const []).isNotEmpty) ...[
            section('Tags'),
            for (final t in sb!.tags)
              nav(
                TaskView(kind: ViewKind.tag, id: t.id),
                t.name,
                lead: emojiOr(t.icon, dot(t.color)),
                count: t.count,
                depth: t.depth,
                key: Key('nav-tag-${t.name}'),
                onEdit: () => showStyleDialog(context, c, item: t, isTag: true),
              ),
          ],
          const SizedBox(height: 10),
          nav(
            const TaskView(kind: ViewKind.completed),
            'Completed',
            lead: icon(Icons.check_circle_outline),
          ),
          nav(
            const TaskView(kind: ViewKind.trash),
            'Trash',
            lead: icon(Icons.delete_outline),
          ),
          if (extras.isNotEmpty) ...[section('More'), ...extras],
        ],
      ),
    );
  }

  Future<void> _newList(BuildContext context) async {
    final name = await askText(context, title: 'New list', hint: 'Name');
    if (name == null || name.trim().isEmpty) return;
    final id = await c.run(() => c.db.createList(name: name.trim()));
    if (id != null) c.open(TaskView(kind: ViewKind.list, id: id));
  }
}

/// One sidebar row: icon or dot, label, count. The selected row is a raised card.
class NavTile extends StatelessWidget {
  const NavTile({
    super.key,
    required this.label,
    required this.selected,
    required this.onTap,
    this.lead,
    this.count,
    this.depth = 0,
    this.onEdit,
  });

  final String label;
  final bool selected;

  /// Right-click or long-press: edit this list, tag or filter.
  final VoidCallback? onEdit;
  final VoidCallback onTap;
  final Widget? lead;
  final int? count;
  final int depth;

  @override
  Widget build(BuildContext context) {
    final s = context.soft;
    return Padding(
      padding: EdgeInsets.only(left: 8.0 + depth * 16, right: 8, bottom: 2),
      child: Material(
        color: selected ? s.panel : Colors.transparent,
        elevation: selected && s.cardShadow ? 0.6 : 0,
        shadowColor: const Color(0x22141E3C),
        borderRadius: BorderRadius.circular(10),
        child: InkWell(
          borderRadius: BorderRadius.circular(10),
          onTap: onTap,
          onSecondaryTap: onEdit,
          onLongPress: onEdit,
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 6),
            child: Row(
              children: [
                SizedBox(width: 18, child: Center(child: lead)),
                const SizedBox(width: 8),
                Expanded(
                  child: Text(
                    label,
                    overflow: TextOverflow.ellipsis,
                    style: manrope(
                      13.5,
                      selected ? FontWeight.w700 : FontWeight.w500,
                      color: selected ? s.selFg : s.fg,
                    ),
                  ),
                ),
                if (count != null && count! > 0)
                  Text(
                    '$count',
                    style: manrope(
                      11,
                      FontWeight.w600,
                      color: selected ? s.selFg : s.mute,
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

class _SmallButton extends StatelessWidget {
  const _SmallButton({
    super.key,
    required this.icon,
    required this.tooltip,
    required this.onTap,
  });

  final IconData icon;
  final String tooltip;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) => IconButton(
    tooltip: tooltip,
    onPressed: onTap,
    icon: Icon(icon, size: 16, color: context.soft.mute),
    visualDensity: VisualDensity.compact,
    constraints: const BoxConstraints(minWidth: 24, minHeight: 24),
    padding: EdgeInsets.zero,
  );
}

/// A one-field dialog. Returns null when cancelled.
Future<String?> askText(
  BuildContext context, {
  required String title,
  String hint = '',
  String initial = '',
}) {
  final ctl = TextEditingController(text: initial);
  return showDialog<String>(
    context: context,
    builder: (ctx) => AlertDialog(
      title: Text(title, style: manrope(16, FontWeight.w800)),
      content: TextField(
        key: const Key('ask-field'),
        controller: ctl,
        autofocus: true,
        decoration: InputDecoration(hintText: hint),
        onSubmitted: (v) => Navigator.pop(ctx, v),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(ctx),
          child: const Text('Cancel'),
        ),
        FilledButton(
          onPressed: () => Navigator.pop(ctx, ctl.text),
          child: const Text('OK'),
        ),
      ],
    ),
  );
}
