import 'dart:io';

// ignore: deprecated_member_use
import 'package:flutter/material.dart';

import '../src/rust/api/tasks.dart';
import 'controller.dart';
import 'filter_builder.dart';
import 'style_dialog.dart';
import 'tag_order.dart';
import 'theme.dart';

/// Inbox, Lists, Filters, the Tags tree, Completed and Trash (plan/08).
class DeskSidebar extends StatelessWidget {
  const DeskSidebar({
    super.key,
    required this.c,
    this.onPicked,
    this.top = const [],
    this.extras = const [],
    this.viewSelected = true,
  });

  final DeskController c;

  /// Called after a view is picked (the narrow layout closes its drawer).
  final VoidCallback? onPicked;

  /// Entries right under Inbox, e.g. the calendar.
  final List<Widget> top;

  /// Entries under the views, e.g. the Devices and sync spike screens.
  final List<Widget> extras;

  /// False while a full-pane page (Calendar, Devices) is open: no task view is current then.
  final bool viewSelected;

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
      Widget? trail,
    }) => NavTile(
      key: key,
      label: label,
      lead: lead,
      count: count,
      trail: trail,
      depth: depth,
      selected: viewSelected && c.view == v,
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
                    'Utre',
                    style: manrope(17, FontWeight.w800, color: s.fg),
                  ),
                ),
                // Desktop only: whether the app goes through utre-daemon (phones have none).
                if (!Platform.isAndroid)
                  Tooltip(
                    message: c.db.isRemote()
                        ? 'Connected to utre-daemon'
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
          ...top,
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
          if (sb != null && sb.filters.isEmpty)
            Padding(
              padding: const EdgeInsets.fromLTRB(16, 0, 12, 6),
              child: Text(
                'Save a search with +',
                style: manrope(11.5, FontWeight.w500, color: s.mute),
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
          if ((sb?.tags ?? const []).isNotEmpty)
            _TagSection(
              c: c,
              tags: sb!.tags,
              header: (action) => section('Tags', action: action),
              tile: (t, trail) => nav(
                TaskView(kind: ViewKind.tag, id: t.id),
                t.name,
                lead: emojiOr(t.icon, dot(t.color)),
                count: t.count,
                depth: t.depth,
                trail: trail,
                key: Key('nav-tag-${t.name}'),
                onEdit: () => showStyleDialog(context, c, item: t, isTag: true),
              ),
            ),
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

/// The Tags tree. Its header button switches to arranging: each row gets a drag
/// handle, and a tag moves with the tags under it, among its siblings.
class _TagSection extends StatefulWidget {
  const _TagSection({
    required this.c,
    required this.tags,
    required this.header,
    required this.tile,
  });

  final DeskController c;
  final List<NavItem> tags;
  final Widget Function(Widget action) header;
  final Widget Function(NavItem tag, Widget? trail) tile;

  @override
  State<_TagSection> createState() => _TagSectionState();
}

class _TagSectionState extends State<_TagSection> {
  bool _arranging = false;

  /// The order after a drop, shown until the sidebar reloads with it.
  List<NavItem>? _moved;

  @override
  void didUpdateWidget(_TagSection old) {
    super.didUpdateWidget(old);
    if (!identical(old.tags, widget.tags)) _moved = null;
  }

  void _drop(List<NavItem> tags, int from, int to) {
    final move = tagDrop(tags, from, to);
    if (move == null) return;
    setState(() => _moved = applyTagMove(tags, move.id, move.before));
    widget.c.run(() => widget.c.db.placeTag(id: move.id, before: move.before));
  }

  @override
  Widget build(BuildContext context) {
    final s = context.soft;
    final tags = _moved ?? widget.tags;
    final header = widget.header(
      _SmallButton(
        key: const Key('arrange-tags'),
        icon: _arranging ? Icons.check : Icons.swap_vert,
        tooltip: _arranging ? 'Done arranging' : 'Arrange tags',
        onTap: () => setState(() => _arranging = !_arranging),
      ),
    );
    if (!_arranging) {
      return Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [header, for (final t in tags) widget.tile(t, null)],
      );
    }
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        header,
        ReorderableListView.builder(
          shrinkWrap: true,
          physics: const NeverScrollableScrollPhysics(),
          buildDefaultDragHandles: false,
          itemCount: tags.length,
          // onReorderItem's index counts after the lift; tagDrop wants it before.
          onReorderItem: (from, to) =>
              _drop(tags, from, to < from ? to : to + 1),
          proxyDecorator: (child, _, _) =>
              Material(color: Colors.transparent, child: child),
          itemBuilder: (context, i) => KeyedSubtree(
            key: ValueKey(tags[i].id),
            child: widget.tile(
              tags[i],
              ReorderableDragStartListener(
                index: i,
                child: MouseRegion(
                  cursor: SystemMouseCursors.grab,
                  child: Icon(
                    Icons.drag_indicator,
                    key: Key('drag-tag-${tags[i].name}'),
                    size: 16,
                    color: s.mute,
                  ),
                ),
              ),
            ),
          ),
        ),
      ],
    );
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
    this.trail,
  });

  final String label;
  final bool selected;

  /// Right-click or long-press: edit this list, tag or filter.
  final VoidCallback? onEdit;
  final VoidCallback onTap;
  final Widget? lead;
  final int? count;
  final int depth;

  /// Shown at the end instead of the count, e.g. a drag handle.
  final Widget? trail;

  @override
  Widget build(BuildContext context) {
    final s = context.soft;
    return Padding(
      padding: EdgeInsets.only(left: 8.0 + depth * 16, right: 8, bottom: 2),
      child: FocusRing(
        child: Material(
          animationDuration: motion(context, 160),
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
                  if (trail != null)
                    trail!
                  else if (count != null && count! > 0)
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
