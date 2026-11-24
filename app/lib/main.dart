// kalender is built on flutter/material; material_ui is a separate copy that it can't see.
// ignore: deprecated_member_use
import 'package:flutter/material.dart';
import 'package:path_provider/path_provider.dart';

import 'calendar/spike_calendar.dart';
import 'src/rust/api/tasks.dart';
import 'src/rust/frb_generated.dart';
import 'widget_bridge.dart';

/// M0 spike: an Inbox screen backed by the Rust core.
/// The real UI (Soft style with Material tag chips) comes in M2/M4.
Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  await RustLib.init();
  final dir = await getApplicationSupportDirectory();
  final db = await TaskDb.open(path: '${dir.path}/tt.db');
  runApp(TickAltApp(db: db));
}

class TickAltApp extends StatelessWidget {
  const TickAltApp({super.key, required this.db});

  final TaskDb db;

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'Tick Alt',
      theme: ThemeData(
        colorSchemeSeed: const Color(0xFF14B8A6), // Soft teal (placeholder accent)
        useMaterial3: true,
      ),
      home: HomeShell(db: db),
    );
  }
}

/// Bottom navigation between the spike screens.
class HomeShell extends StatefulWidget {
  const HomeShell({super.key, required this.db});

  final TaskDb db;

  @override
  State<HomeShell> createState() => _HomeShellState();
}

class _HomeShellState extends State<HomeShell> {
  int _tab = 0;

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      body: IndexedStack(
        index: _tab,
        children: [InboxPage(db: widget.db), const SpikeCalendarPage()],
      ),
      bottomNavigationBar: NavigationBar(
        selectedIndex: _tab,
        onDestinationSelected: (i) => setState(() => _tab = i),
        destinations: const [
          NavigationDestination(icon: Icon(Icons.inbox_outlined), label: 'Inbox'),
          NavigationDestination(icon: Icon(Icons.calendar_month_outlined), label: 'Calendar'),
        ],
      ),
    );
  }
}

class InboxPage extends StatefulWidget {
  const InboxPage({super.key, required this.db});

  final TaskDb db;

  @override
  State<InboxPage> createState() => _InboxPageState();
}

class _InboxPageState extends State<InboxPage> with WidgetsBindingObserver {
  final _input = TextEditingController();
  List<TaskItem> _tasks = [];

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    _reload();
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    _input.dispose();
    super.dispose();
  }

  /// The home-screen widget may have completed tasks while the app was in the background.
  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state == AppLifecycleState.resumed) _reload();
  }

  Future<void> _pinWidget() async {
    final ok = await WidgetBridge.pin();
    if (!ok && mounted) {
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(content: Text('This launcher can’t add widgets from apps. Add it from the home screen.')),
      );
    }
  }

  Future<void> _reload() async {
    final tasks = await widget.db.inbox();
    if (mounted) setState(() => _tasks = tasks);
  }

  Future<void> _add() async {
    final title = _input.text.trim();
    if (title.isEmpty) return;
    await widget.db.add(title: title);
    _input.clear();
    await _reload();
    await WidgetBridge.refresh();
  }

  Future<void> _complete(TaskItem task) async {
    await widget.db.complete(id: task.id);
    await _reload();
    await WidgetBridge.refresh();
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: Text('Inbox (${_tasks.length})'),
        actions: [
          if (WidgetBridge.supported)
            IconButton(
              key: const Key('pin-widget'),
              tooltip: 'Add widget to home screen',
              onPressed: _pinWidget,
              icon: const Icon(Icons.widgets_outlined),
            ),
        ],
      ),
      body: Column(
        children: [
          Padding(
            padding: const EdgeInsets.all(16),
            child: TextField(
              key: const Key('add-field'),
              controller: _input,
              decoration: const InputDecoration(
                hintText: 'Add a task',
                border: OutlineInputBorder(),
              ),
              // Keep the keyboard up so several tasks can be added in a row.
              onSubmitted: (_) => _add(),
              onEditingComplete: () {},
            ),
          ),
          Expanded(
            child: _tasks.isEmpty
                ? const Center(child: Text('Inbox is empty'))
                : ListView.builder(
                    itemCount: _tasks.length,
                    itemBuilder: (context, i) {
                      final t = _tasks[i];
                      return CheckboxListTile(
                        key: Key('task-${t.id}'),
                        controlAffinity: ListTileControlAffinity.leading,
                        value: false,
                        title: Text(t.title),
                        onChanged: (_) => _complete(t),
                      );
                    },
                  ),
          ),
        ],
      ),
    );
  }
}
