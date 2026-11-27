import 'dart:io';

// ignore: deprecated_member_use
import 'package:flutter/material.dart';
import 'package:flutter/semantics.dart';
import 'package:flutter/services.dart';

import '../src/rust/api/tasks.dart';
import 'controller.dart';
import 'tag_order.dart';
import 'theme.dart';

/// Builds one tag row: `trail` replaces the count (a drag handle while arranging), and
/// `expander` sits before the icon (the collapse chevron, or a blank of its width).
typedef TagTile = Widget Function(
  NavItem tag, {
  Widget? trail,
  Widget? expander,
});

/// The sidebar's Tags tree (plan/15 "Nesting tags"). Parents fold away with a chevron,
/// remembered per device. While arranging, every tag shows and each row has a handle:
/// drop on a row's top or bottom edge to place a tag there (dragging left or right picks
/// the level), or on its middle to nest it last under that tag. Alt+arrows do the same.
class TagSection extends StatefulWidget {
  const TagSection({
    super.key,
    required this.c,
    required this.tags,
    required this.header,
    required this.tile,
    this.collapsed = const {},
    this.onCollapse,
    this.empty,
  });

  final DeskController c;
  final List<NavItem> tags;

  /// The section header, given the arrange button (null with fewer than two tags).
  final Widget Function(Widget? arrange) header;
  final TagTile tile;

  /// Collapsed tags (ids). Without [onCollapse] there are no chevrons.
  final Set<String> collapsed;
  final void Function(String id, bool collapsed)? onCollapse;

  /// Shown under the header when there are no tags.
  final Widget? empty;

  @override
  State<TagSection> createState() => _TagSectionState();
}

enum _Zone { above, into, below }

class _TagSectionState extends State<TagSection> {
  bool _arranging = false;

  /// The order after a drop, shown until the sidebar reloads with it.
  List<NavItem>? _moved;

  /// The row being dragged, where the pointer went down, and what a drop would do now.
  int? _from;
  double _startX = 0;
  ({int row, _Zone zone, TagNest nest, int depth})? _hover;

  /// One nesting level per this much horizontal drag.
  static const _step = 24.0;

  @override
  void didUpdateWidget(TagSection old) {
    super.didUpdateWidget(old);
    if (!identical(old.tags, widget.tags)) _moved = null;
  }

  void _apply(List<NavItem> tags, TagNest? n) {
    if (n == null) return;
    setState(() => _moved = applyTagNest(tags, n));
    // A tag dropped into a folded parent would vanish once arranging ends.
    if (n.parent != null && widget.collapsed.contains(n.parent)) {
      widget.onCollapse?.call(n.parent!, false);
    }
    widget.c.run(
      () => widget.c.db.nestTag(id: n.id, parentId: n.parent, before: n.before),
    );
  }

  void _place(List<NavItem> tags, int i, int dir) {
    final to = tagStep(tags, i, dir);
    if (to == null) return;
    final move = tagDrop(tags, i, to);
    if (move == null) return;
    setState(() => _moved = applyTagMove(tags, move.id, move.before));
    widget.c.run(() => widget.c.db.placeTag(id: move.id, before: move.before));
  }

  /// What dropping at `pointer` over row `row` (whose box is `box`) would do.
  void _track(List<NavItem> tags, int row, RenderBox box, Offset pointer) {
    final from = _from;
    if (from == null) return;
    final y = box.globalToLocal(pointer).dy / box.size.height;
    final zone = y < 0.28
        ? _Zone.above
        : y > 0.72
        ? _Zone.below
        : _Zone.into;
    final levels = ((pointer.dx - _startX) / _step).round();
    TagNest? nest;
    var depth = 0;
    if (zone == _Zone.into) {
      nest = tagNestDrop(tags, from, row);
      depth = tags[row].depth + 1;
    } else {
      final gap = zone == _Zone.above ? row : row + 1;
      final range = tagGapDepths(tags, from, gap);
      if (range != null) {
        depth = (tags[from].depth + levels).clamp(range.min, range.max);
        nest = tagGapDrop(tags, from, gap, depth);
      }
    }
    final next = nest == null
        ? null
        : (row: row, zone: zone, nest: nest, depth: depth);
    if (next != _hover) setState(() => _hover = next);
  }

  void _endDrag() => setState(() {
    _from = null;
    _hover = null;
  });

  @override
  Widget build(BuildContext context) {
    final all = _moved ?? widget.tags;
    final canFold = widget.onCollapse != null;
    final anyParent = [
      for (var i = 0; i < all.length; i++) tagHasChildren(all, i),
    ].any((x) => x);
    final header = widget.header(
      all.length < 2
          ? null
          : _ArrangeButton(
              arranging: _arranging,
              onTap: () => setState(() {
                _arranging = !_arranging;
                _endDrag();
              }),
            ),
    );
    // Touch devices get bigger targets.
    final touch = Platform.isAndroid;
    final foldWidth = touch ? 26.0 : 18.0;
    Widget? blank() => canFold && anyParent ? SizedBox(width: foldWidth) : null;

    if (!_arranging) {
      final shown = visibleTags(all, widget.collapsed);
      return Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          header,
          if (all.isEmpty) ?widget.empty,
          for (final t in shown)
            widget.tile(
              t,
              expander: canFold && tagHasChildren(all, all.indexOf(t))
                  ? _Fold(
                      key: Key('fold-tag-${t.name}'),
                      width: foldWidth,
                      collapsed: widget.collapsed.contains(t.id),
                      onTap: () => widget.onCollapse!(
                        t.id,
                        !widget.collapsed.contains(t.id),
                      ),
                    )
                  : blank(),
            ),
        ],
      );
    }

    final s = context.soft;
    final size = touch ? 48.0 : 28.0;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        header,
        for (var i = 0; i < all.length; i++)
          _row(context, s, all, i, size, blank()),
      ],
    );
  }

  Widget _row(
    BuildContext context,
    SoftColors s,
    List<NavItem> all,
    int i,
    double size,
    Widget? expander,
  ) {
    final t = all[i];
    final from = _from;
    final lifted = from != null && tagInSubtree(all, from, i);
    final handle = Listener(
      onPointerDown: (e) => _startX = e.position.dx,
      child: Draggable<int>(
        data: i,
        dragAnchorStrategy: pointerDragAnchorStrategy,
        onDragStarted: () => setState(() => _from = i),
        onDragEnd: (_) => _endDrag(),
        onDraggableCanceled: (_, _) => _endDrag(),
        feedback: Transform.translate(
          offset: const Offset(-24, -16),
          child: Material(
            elevation: 4,
            color: s.panel,
            borderRadius: BorderRadius.circular(10),
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 7),
              child: Text(t.name, style: manrope(13.5, FontWeight.w600)),
            ),
          ),
        ),
        child: MouseRegion(
          cursor: SystemMouseCursors.grab,
          child: SizedBox(
            width: size,
            height: size,
            child: Icon(
              Icons.drag_indicator,
              key: Key('drag-tags-${t.name}'),
              size: 16,
              color: s.mute,
            ),
          ),
        ),
      ),
    );
    final hover = _hover?.row == i ? _hover : null;
    final indent = 8.0 + t.depth * 16;
    final tile = widget.tile(t, trail: handle, expander: expander);
    final marked = Stack(
      clipBehavior: Clip.none,
      children: [
        Opacity(opacity: lifted ? 0.4 : 1, child: tile),
        if (hover?.zone == _Zone.into)
          Positioned.fill(
            left: indent,
            right: 8,
            bottom: 2,
            child: IgnorePointer(
              child: DecoratedBox(
                key: const Key('tag-drop-into'),
                decoration: BoxDecoration(
                  color: s.accent.withValues(alpha: 0.10),
                  border: Border.all(color: s.accent, width: 1.5),
                  borderRadius: BorderRadius.circular(10),
                ),
              ),
            ),
          )
        else if (hover != null)
          Positioned(
            left: 8.0 + hover.depth * 16 + 8,
            right: 8,
            top: hover.zone == _Zone.above ? -2 : null,
            bottom: hover.zone == _Zone.below ? 0 : null,
            height: 2,
            child: IgnorePointer(
              child: ColoredBox(
                key: const Key('tag-drop-line'),
                color: s.accent,
              ),
            ),
          ),
      ],
    );
    final target = DragTarget<int>(
      onWillAcceptWithDetails: (_) => !lifted,
      onMove: (d) {
        // This row's own box, found through its key below.
        final mine = _boxes[t.id]?.currentContext?.findRenderObject();
        if (mine is RenderBox) _track(all, i, mine, d.offset);
      },
      onLeave: (_) {
        if (_hover?.row == i) setState(() => _hover = null);
      },
      onAcceptWithDetails: (_) {
        final n = _hover?.row == i ? _hover!.nest : null;
        _endDrag();
        _apply(all, n);
      },
      builder: (_, _, _) => KeyedSubtree(
        key: _boxes.putIfAbsent(t.id, GlobalKey.new),
        child: marked,
      ),
    );
    return KeyedSubtree(
      key: ValueKey(t.id),
      child: CallbackShortcuts(
        bindings: {
          const SingleActivator(LogicalKeyboardKey.arrowUp, alt: true): () =>
              _place(all, i, -1),
          const SingleActivator(LogicalKeyboardKey.arrowDown, alt: true): () =>
              _place(all, i, 1),
          const SingleActivator(LogicalKeyboardKey.arrowRight, alt: true): () =>
              _apply(all, tagIndent(all, i)),
          const SingleActivator(LogicalKeyboardKey.arrowLeft, alt: true): () =>
              _apply(all, tagOutdent(all, i)),
        },
        child: Semantics(
          customSemanticsActions: {
            const CustomSemanticsAction(label: 'Move up'): () =>
                _place(all, i, -1),
            const CustomSemanticsAction(label: 'Move down'): () =>
                _place(all, i, 1),
            const CustomSemanticsAction(
              label: 'Nest under the tag above',
            ): () =>
                _apply(all, tagIndent(all, i)),
            const CustomSemanticsAction(label: 'Move out of its parent'): () =>
                _apply(all, tagOutdent(all, i)),
          },
          child: target,
        ),
      ),
    );
  }

  /// Each row's box, to measure where over it the pointer is.
  final _boxes = <String, GlobalKey>{};
}

class _ArrangeButton extends StatelessWidget {
  const _ArrangeButton({required this.arranging, required this.onTap});

  final bool arranging;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) => IconButton(
    key: const Key('arrange-tags'),
    tooltip: arranging ? 'Done arranging' : 'Arrange tags',
    onPressed: onTap,
    icon: Icon(
      arranging ? Icons.check : Icons.swap_vert,
      size: 16,
      color: context.soft.mute,
    ),
    visualDensity: VisualDensity.compact,
    constraints: const BoxConstraints(minWidth: 24, minHeight: 24),
    padding: EdgeInsets.zero,
  );
}

/// The chevron that folds a parent tag's children away.
class _Fold extends StatelessWidget {
  const _Fold({
    super.key,
    required this.width,
    required this.collapsed,
    required this.onTap,
  });

  final double width;
  final bool collapsed;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) => Semantics(
    button: true,
    label: collapsed ? 'Expand' : 'Collapse',
    child: InkWell(
      borderRadius: BorderRadius.circular(6),
      onTap: onTap,
      child: SizedBox(
        width: width,
        height: 24,
        child: AnimatedRotation(
          turns: collapsed ? 0 : 0.25,
          duration: motion(context, 140),
          child: Icon(Icons.chevron_right, size: 16, color: context.soft.mute),
        ),
      ),
    ),
  );
}
