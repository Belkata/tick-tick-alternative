import 'package:material_ui/material_ui.dart';
import 'package:path_provider/path_provider.dart';

import 'src/rust/api/tasks.dart';
import 'src/rust/frb_generated.dart';

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
      home: InboxPage(db: db),
    );
  }
}

class InboxPage extends StatefulWidget {
  const InboxPage({super.key, required this.db});

  final TaskDb db;

  @override
  State<InboxPage> createState() => _InboxPageState();
}

class _InboxPageState extends State<InboxPage> {
  final _input = TextEditingController();
  List<TaskItem> _tasks = [];

  @override
  void initState() {
    super.initState();
    _reload();
  }

  @override
  void dispose() {
    _input.dispose();
    super.dispose();
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
  }

  Future<void> _complete(TaskItem task) async {
    await widget.db.complete(id: task.id);
    await _reload();
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: Text('Inbox (${_tasks.length})')),
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
