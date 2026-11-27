// ignore: deprecated_member_use
import 'package:flutter/material.dart';

import '../src/rust/api/tasks.dart';
import 'controller.dart';
import 'filter_builder.dart' show confirm;
import 'theme.dart';

/// Tag and list colours: readable in light and dark, close to the Soft palette.
const tagPalette = [
  '#e2525c', // red
  '#eb9b2c', // orange
  '#d9b23a', // yellow
  '#3fae5a', // green
  '#12a594', // teal
  '#4c8df6', // blue
  '#6b6bdf', // indigo
  '#9b6bdf', // purple
  '#c45bd6', // magenta
  '#e06aa4', // pink
  '#8a6f5c', // brown
  '#7a8197', // grey
];

/// Rename, recolour, set an icon on, or delete a list or tag (right-click in the sidebar).
Future<void> showStyleDialog(
  BuildContext context,
  DeskController c, {
  required NavItem item,
  required bool isTag,
  VoidCallback? onNewSubTag,
}) => showDialog<void>(
  context: context,
  builder: (_) =>
      _StyleDialog(c: c, item: item, isTag: isTag, onNewSubTag: onNewSubTag),
);

class _StyleDialog extends StatefulWidget {
  const _StyleDialog({
    required this.c,
    required this.item,
    required this.isTag,
    this.onNewSubTag,
  });

  final DeskController c;
  final NavItem item;
  final bool isTag;

  /// Tags: closes the dialog and asks for a tag to make under this one.
  final VoidCallback? onNewSubTag;

  @override
  State<_StyleDialog> createState() => _StyleDialogState();
}

class _StyleDialogState extends State<_StyleDialog> {
  late final _name = TextEditingController(text: widget.item.name);
  late final _icon = TextEditingController(text: widget.item.icon ?? '');
  late String? _color = widget.item.color;
  String? _error;

  @override
  void dispose() {
    _name.dispose();
    _icon.dispose();
    super.dispose();
  }

  Future<void> _save() async {
    final db = widget.c.db;
    final id = widget.item.id;
    final name = _name.text.trim();
    final icon = _icon.text.trim().isEmpty ? null : _icon.text.trim();
    try {
      if (name.isNotEmpty && name != widget.item.name) {
        widget.isTag
            ? await db.renameTag(id: id, name: name)
            : await db.renameList(id: id, name: name);
      }
      if (_color != widget.item.color || icon != widget.item.icon) {
        widget.isTag
            ? await db.setTagStyle(id: id, color: _color, icon: icon)
            : await db.setListStyle(id: id, color: _color, icon: icon);
      }
      if (mounted) Navigator.of(context).pop();
    } catch (e) {
      setState(
        () => _error = '$e'.replaceFirst(RegExp(r'^AnyhowException\(?'), ''),
      );
    }
  }

  Future<void> _delete() async {
    final what = widget.isTag ? 'tag' : 'list';
    final ok = await confirm(
      context,
      title: 'Delete the $what "${widget.item.name}"?',
      body: widget.isTag
          ? 'Its tasks lose this tag and the tags nested under it. The tasks stay.'
          : 'Its tasks stay, with no list (untagged, undated ones return to Inbox).',
    );
    if (!ok || !mounted) return;
    final c = widget.c;
    final id = widget.item.id;
    try {
      // Leave its view first, so the reload after the delete doesn't ask for it.
      if (c.view.id == id) c.open(const TaskView(kind: ViewKind.inbox));
      widget.isTag
          ? await c.db.deleteTag(id: id)
          : await c.db.deleteList(id: id);
      if (mounted) Navigator.of(context).pop();
    } catch (e) {
      setState(() => _error = '$e');
    }
  }

  @override
  Widget build(BuildContext context) {
    final s = context.soft;
    Widget swatch(String? hex) {
      final on = _color == hex;
      return FocusRing(
        circle: true,
        child: InkWell(
          key: Key('swatch-${hex ?? 'none'}'),
          customBorder: const CircleBorder(),
          onTap: () => setState(() => _color = hex),
          child: Container(
            width: 28,
            height: 28,
            decoration: BoxDecoration(
              shape: BoxShape.circle,
              color: hex == null ? null : hexColor(hex, s.mute),
              border: Border.all(
                color: on ? s.fg : s.line,
                width: on ? 2.5 : 1,
              ),
            ),
            child: hex == null
                ? Icon(Icons.block, size: 14, color: s.mute)
                : null,
          ),
        ),
      );
    }

    return AlertDialog(
      title: Text(
        widget.isTag ? 'Edit tag' : 'Edit list',
        style: manrope(17, FontWeight.w800),
      ),
      content: SizedBox(
        width: 360,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Row(
              children: [
                Expanded(
                  child: TextField(
                    key: const Key('style-name'),
                    controller: _name,
                    decoration: InputDecoration(
                      hintText: widget.isTag
                          ? 'Tag name (no "/")'
                          : 'List name',
                    ),
                  ),
                ),
                const SizedBox(width: 8),
                SizedBox(
                  width: 72,
                  child: TextField(
                    key: const Key('style-icon'),
                    controller: _icon,
                    textAlign: TextAlign.center,
                    decoration: const InputDecoration(hintText: 'Icon'),
                  ),
                ),
              ],
            ),
            const SizedBox(height: 14),
            Text('Colour', style: manrope(12, FontWeight.w700, color: s.mute)),
            const SizedBox(height: 8),
            Wrap(
              spacing: 8,
              runSpacing: 8,
              children: [
                swatch(null),
                for (final hex in tagPalette) swatch(hex),
              ],
            ),
            if (_error != null)
              Padding(
                padding: const EdgeInsets.only(top: 10),
                child: Text(
                  _error!,
                  style: manrope(12.5, FontWeight.w600, color: s.late),
                ),
              ),
          ],
        ),
      ),
      actions: [
        TextButton(
          key: const Key('style-delete'),
          onPressed: _delete,
          style: TextButton.styleFrom(foregroundColor: s.late),
          child: const Text('Delete'),
        ),
        if (widget.onNewSubTag != null)
          TextButton(
            key: const Key('style-subtag'),
            onPressed: () {
              Navigator.of(context).pop();
              widget.onNewSubTag!();
            },
            child: const Text('New sub-tag'),
          ),
        TextButton(
          onPressed: () => Navigator.of(context).pop(),
          child: const Text('Cancel'),
        ),
        FilledButton(
          key: const Key('style-save'),
          onPressed: _save,
          child: const Text('Save'),
        ),
      ],
    );
  }
}
