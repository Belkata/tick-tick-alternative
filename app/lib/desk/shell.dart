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

/// Extra full-pane screens reachable from the sidebar (the calendar and sync spikes).
class ShellPage {
  const ShellPage(this.label, this.icon, this.builder);

  final String label;
  final IconData icon;
  final WidgetBuilder builder;
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

  /// Index into [DeskShell.pages] when a spike screen is open instead of the tasks.
  int? _page;

  /// At or above this width the three panes sit side by side.
  static const wide = 760.0;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    c.addListener(_onChange);
    c.reload();
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
    return DeskSidebar(
      c: c,
      onPicked: () {
        _showTasks();
        if (inDrawer) Navigator.of(context).pop();
      },
      extras: [
        for (var i = 0; i < widget.pages.length; i++)
          NavTile(
            label: widget.pages[i].label,
            lead: Icon(widget.pages[i].icon, size: 16, color: s.mute),
            selected: _page == i,
            onTap: () {
              setState(() => _page = i);
              if (inDrawer) Navigator.of(context).pop();
            },
          ),
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
                : widget.pages[_page!].builder(context);
            if (isWide) {
              final sideW = box.maxWidth >= 1100 ? 230.0 : 200.0;
              final detailW = box.maxWidth >= 1100 ? 340.0 : 290.0;
              return Scaffold(
                body: Row(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
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
                child: page != null
                    ? Stack(
                        children: [
                          Positioned.fill(child: page),
                          Positioned(
                            left: 4,
                            top: 4,
                            child: IconButton(
                              icon: const Icon(Icons.menu),
                              onPressed: () =>
                                  _scaffold.currentState?.openDrawer(),
                            ),
                          ),
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
