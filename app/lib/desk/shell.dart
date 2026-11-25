// ignore: deprecated_member_use
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../src/rust/api/tasks.dart';
import '../widget_bridge.dart';
import 'controller.dart';
import 'detail.dart';
import 'sidebar.dart';
import 'task_list.dart';
import 'theme.dart';

/// Extra full-pane screens reachable from the sidebar (the calendar, Devices).
class ShellPage {
  const ShellPage(
    this.label,
    this.icon,
    this.builder, {
    this.railSidebar = false,
    this.ownMenuButton = false,
    this.top = false,
  });

  final String label;
  final IconData icon;

  /// Gets the shell's controller, for pages that show or select tasks.
  final Widget Function(BuildContext context, DeskController c) builder;

  /// On wide windows, fold the sidebar to an icon rail while this page is open, so the
  /// page gets the room (the calendar's seven columns, plan/18).
  final bool railSidebar;

  /// On narrow windows and phones the page draws its own menu button (it opens the
  /// drawer with `Scaffold.of(context).openDrawer()`), instead of the shell adding a bar.
  final bool ownMenuButton;

  /// Listed right under Inbox instead of in the sidebar's More section.
  final bool top;
}

/// M2 window: sidebar | task list | detail on wide windows (plan/08), and a single
/// pane with a drawer and a pushed detail on narrow ones and phones.
class DeskShell extends StatefulWidget {
  const DeskShell({super.key, required this.db, this.pages = const []});

  final TaskDb db;
  final List<ShellPage> pages;

  @override
  State<DeskShell> createState() => _DeskShellState();
}

class _DeskShellState extends State<DeskShell> with WidgetsBindingObserver {
  late final DeskController c = DeskController(widget.db);
  final _keys = FocusNode(debugLabel: 'shell-keys');
  final _addFocus = FocusNode(debugLabel: 'quick-add');
  final _titleFocus = FocusNode(debugLabel: 'detail-title');
  final _scaffold = GlobalKey<ScaffoldState>();

  /// Index into [DeskShell.pages] when a page is open instead of the tasks.
  int? _page;

  /// The user unfolded the sidebar while a rail page is open.
  bool _railOpen = false;

  /// At or above this width the three panes sit side by side.
  static const wide = 760.0;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    c.addListener(_onChange);
    c.reload();
    // Android share sheet: shared text or links become Inbox tasks.
    WidgetBridge.onShared(_addShared);
    WidgetBridge.takeShared().then((t) {
      if (t != null) _addShared(t);
    });
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    c.removeListener(_onChange);
    c.dispose();
    for (final f in [_keys, _addFocus, _titleFocus]) {
      f.dispose();
    }
    super.dispose();
  }

  /// The Android widget writes from Kotlin, outside the change stream: reload on resume.
  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state == AppLifecycleState.resumed) c.reload();
  }

  Future<void> _addShared(String text) async {
    final title = text.length > 300 ? '${text.substring(0, 300)}…' : text;
    final id = await c.run(
      () => c.db.addIn(
        view: const TaskView(kind: ViewKind.inbox),
        title: title,
      ),
    );
    if (id != null && mounted) {
      ScaffoldMessenger.of(context)
          .showSnackBar(SnackBar(content: Text('Added to Inbox: $title')));
    }
  }

  void _onChange() {
    final err = c.error;
    if (err != null && mounted) {
      c.error = null;
      ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text(err)));
    }
    WidgetBridge.changed();
  }

  bool get _typing {
    final ctx = FocusManager.instance.primaryFocus?.context;
    return ctx != null &&
        (ctx.widget is EditableText ||
            ctx.findAncestorWidgetOfExactType<EditableText>() != null);
  }

  /// `n` new · `Enter` edit · `Space` complete · `Del` trash · arrows move · `Esc` back to the list.
  KeyEventResult _onKey(FocusNode node, KeyEvent e) {
    if (e is! KeyDownEvent && e is! KeyRepeatEvent) {
      return KeyEventResult.ignored;
    }
    final k = e.logicalKey;
    if (_typing) {
      if (k == LogicalKeyboardKey.escape) {
        _keys.requestFocus();
        return KeyEventResult.handled;
      }
      return KeyEventResult.ignored;
    }
    if (HardwareKeyboard.instance.isControlPressed ||
        HardwareKeyboard.instance.isAltPressed ||
        HardwareKeyboard.instance.isMetaPressed) {
      return KeyEventResult.ignored;
    }
    final sel = c.selectedId;
    if (k == LogicalKeyboardKey.keyN && c.canAdd) {
      _showTasks();
      _addFocus.requestFocus();
    } else if (k == LogicalKeyboardKey.arrowDown) {
      c.moveSelection(1);
    } else if (k == LogicalKeyboardKey.arrowUp) {
      c.moveSelection(-1);
    } else if (sel != null &&
        (k == LogicalKeyboardKey.enter ||
            k == LogicalKeyboardKey.numpadEnter)) {
      if (MediaQuery.sizeOf(context).width < wide) {
        _pushDetail();
      } else {
        _titleFocus.requestFocus();
      }
    } else if (sel != null && k == LogicalKeyboardKey.space) {
      c.toggleDone(sel);
    } else if (sel != null &&
        (k == LogicalKeyboardKey.delete || k == LogicalKeyboardKey.backspace)) {
      c.trashOrRestore(sel);
    } else {
      return KeyEventResult.ignored;
    }
    return KeyEventResult.handled;
  }

  void _showTasks() {
    if (_page != null) setState(() => _page = null);
  }

  /// The folded sidebar: Inbox, the extra pages, and » to unfold it.
  Widget _rail() {
    final s = context.soft;
    Widget item(
      String tip,
      IconData icon,
      bool on,
      VoidCallback onTap, {
      Key? key,
      int? count,
    }) => Tooltip(
      message: tip,
      child: InkWell(
        key: key,
        borderRadius: BorderRadius.circular(11),
        onTap: onTap,
        child: Container(
          width: 40,
          height: 36,
          decoration: on
              ? BoxDecoration(
                  color: s.panel,
                  borderRadius: BorderRadius.circular(11),
                  boxShadow: s.cardShadow
                      ? const [
                          BoxShadow(
                            color: Color(0x14141E3C),
                            blurRadius: 3,
                            offset: Offset(0, 1),
                          ),
                        ]
                      : null,
                )
              : null,
          child: Stack(
            alignment: Alignment.center,
            children: [
              Icon(icon, size: 18, color: on ? s.selFg : s.mute),
              if (count != null && count > 0)
                Positioned(
                  right: 2,
                  top: 2,
                  child: Container(
                    padding: const EdgeInsets.symmetric(horizontal: 4),
                    decoration: BoxDecoration(
                      color: s.accent,
                      borderRadius: BorderRadius.circular(6),
                    ),
                    child: Text(
                      '$count',
                      style: manrope(
                        9,
                        FontWeight.w700,
                        color: Theme.of(context).colorScheme.onPrimary,
                      ),
                    ),
                  ),
                ),
            ],
          ),
        ),
      ),
    );
    return Container(
      width: 60,
      color: s.side,
      padding: const EdgeInsets.symmetric(vertical: 14),
      child: Column(
        children: [
          item('Inbox', Icons.inbox_outlined, false, () {
            c.open(const TaskView(kind: ViewKind.inbox));
            _showTasks();
          }, count: c.sidebar?.inbox),
          const SizedBox(height: 6),
          for (var i = 0; i < widget.pages.length; i++) ...[
            item(
              widget.pages[i].label,
              widget.pages[i].icon,
              _page == i,
              () => setState(() => _page = i),
            ),
            const SizedBox(height: 6),
          ],
          const Spacer(),
          item(
            'Show the sidebar',
            Icons.keyboard_double_arrow_right,
            false,
            () => setState(() => _railOpen = true),
            key: const Key('rail-unfold'),
          ),
        ],
      ),
    );
  }

  void _openRow(String id) {
    c.select(id);
    _keys.requestFocus();
    if (MediaQuery.sizeOf(context).width < wide) _pushDetail();
  }

  void _pushDetail() {
    Navigator.of(context).push(
      MaterialPageRoute<void>(
        builder: (_) => Scaffold(
          appBar: AppBar(backgroundColor: context.soft.panel),
          body: ListenableBuilder(
            listenable: c,
            builder: (_, _) => DetailPane(c: c, titleFocus: _titleFocus),
          ),
        ),
      ),
    );
  }

  Widget _sidebar({required bool inDrawer}) {
    final s = context.soft;
    Widget pageTile(int i) => NavTile(
      label: widget.pages[i].label,
      lead: Icon(widget.pages[i].icon, size: 16, color: s.mute),
      selected: _page == i,
      onTap: () {
        setState(() {
          _page = i;
          _railOpen = false;
        });
        if (inDrawer) Navigator.of(context).pop();
      },
    );
    return DeskSidebar(
      c: c,
      onPicked: () {
        _showTasks();
        if (inDrawer) Navigator.of(context).pop();
      },
      top: [
        for (var i = 0; i < widget.pages.length; i++)
          if (widget.pages[i].top) pageTile(i),
      ],
      extras: [
        for (var i = 0; i < widget.pages.length; i++)
          if (!widget.pages[i].top) pageTile(i),
        if (WidgetBridge.supported)
          NavTile(
            key: const Key('pin-widget'),
            label: 'Add home-screen widget',
            lead: Icon(Icons.widgets_outlined, size: 16, color: s.mute),
            selected: false,
            onTap: _pinWidget,
          ),
      ],
    );
  }

  Future<void> _pinWidget() async {
    final ok = await WidgetBridge.pin();
    if (!ok && mounted) {
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(
          content: Text(
            'This launcher can’t add widgets from apps. Add it from the home screen.',
          ),
        ),
      );
    }
  }

  @override
  Widget build(BuildContext context) {
    final s = context.soft;
    return Focus(
      focusNode: _keys,
      autofocus: true,
      onKeyEvent: _onKey,
      child: ListenableBuilder(
        listenable: c,
        builder: (context, _) => LayoutBuilder(
          builder: (context, box) {
            final isWide = box.maxWidth >= wide;
            final page = _page == null
                ? null
                : widget.pages[_page!].builder(context, c);
            final rail =
                _page != null && widget.pages[_page!].railSidebar && !_railOpen;
            if (isWide) {
              final sideW = box.maxWidth >= 1100 ? 230.0 : 200.0;
              final detailW = box.maxWidth >= 1100 ? 340.0 : 290.0;
              return Scaffold(
                body: Row(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    if (rail)
                      _rail()
                    else
                      SizedBox(width: sideW, child: _sidebar(inDrawer: false)),
                    VerticalDivider(width: 1, color: s.line),
                    if (page != null)
                      Expanded(child: page)
                    else ...[
                      Expanded(
                        child: TaskListPane(
                          c: c,
                          addFocus: _addFocus,
                          onOpen: _openRow,
                        ),
                      ),
                      VerticalDivider(width: 1, color: s.line),
                      SizedBox(
                        width: detailW,
                        child: DetailPane(c: c, titleFocus: _titleFocus),
                      ),
                    ],
                  ],
                ),
              );
            }
            return Scaffold(
              key: _scaffold,
              drawer: Drawer(
                backgroundColor: s.side,
                child: SafeArea(child: _sidebar(inDrawer: true)),
              ),
              body: SafeArea(
                child: page != null && widget.pages[_page!].ownMenuButton
                    ? page
                    : page != null
                    // A bar for the menu button, so it never covers the page's own title.
                    ? Column(
                        crossAxisAlignment: CrossAxisAlignment.stretch,
                        children: [
                          Align(
                            alignment: Alignment.centerLeft,
                            child: IconButton(
                              key: const Key('open-drawer'),
                              icon: const Icon(Icons.menu),
                              onPressed: () =>
                                  _scaffold.currentState?.openDrawer(),
                            ),
                          ),
                          Expanded(child: page),
                        ],
                      )
                    : TaskListPane(
                        c: c,
                        addFocus: _addFocus,
                        onOpen: _openRow,
                        leading: IconButton(
                          key: const Key('open-drawer'),
                          icon: const Icon(Icons.menu),
                          onPressed: () => _scaffold.currentState?.openDrawer(),
                        ),
                      ),
              ),
            );
          },
        ),
      ),
    );
  }
}
