import 'dart:async';

import 'package:flutter_test/flutter_test.dart';
import 'package:utre/desk/controller.dart';
import 'package:utre/src/rust/api/tasks.dart';

/// A core whose sidebar answers when the test says so.
class _SlowDb implements TaskDb {
  final sidebarGate = Completer<Sidebar>();

  @override
  Stream<int> changes() => const Stream.empty();

  @override
  Future<Sidebar> sidebar() => sidebarGate.future;

  @override
  dynamic noSuchMethod(Invocation i) => super.noSuchMethod(i);
}

void main() {
  test('a reload that finishes after dispose does not notify', () async {
    final db = _SlowDb();
    final c = DeskController(db);
    var notified = 0;
    c.addListener(() => notified++);
    final reload = c.reload();
    c.dispose();
    db.sidebarGate.completeError('daemon went away');
    await reload;
    expect(notified, 0);
  });

  test('run() after dispose swallows the error without notifying', () async {
    final c = DeskController(_SlowDb());
    c.dispose();
    final r = await c.run<int>(() async => throw 'boom');
    expect(r, isNull);
  });
}
