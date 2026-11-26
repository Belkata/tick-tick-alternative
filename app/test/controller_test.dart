import 'dart:async';

import 'package:flutter_test/flutter_test.dart';
import 'package:utre/desk/controller.dart';
import 'package:utre/src/rust/api/tasks.dart';

import 'golden/fake_db.dart';

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
  midnightTests();
  changeTests();
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

class _CountingDb extends _SlowDb {
  int sidebars = 0;

  @override
  Future<Sidebar> sidebar() {
    sidebars++;
    return Future.error('stop here');
  }
}

void midnightTests() {
  testWidgets('reloads when the day rolls over', (tester) async {
    final db = _CountingDb();
    var now = DateTime(2026, 11, 26, 23, 59, 0);
    final c = DeskController(db, now: () => now);
    await tester.pump(const Duration(seconds: 30));
    expect(db.sidebars, 0);
    now = DateTime(2026, 11, 26, 23, 59, 30);
    await tester.pump(const Duration(seconds: 40));
    expect(db.sidebars, 1);
    c.dispose();
  });
}

class _ChangingDb extends FakeDb {
  final events = StreamController<int>.broadcast();
  int sidebars = 0;

  /// When set, each sidebar call waits for it.
  Completer<void>? gate;

  @override
  Stream<int> changes() => events.stream;

  @override
  Future<Sidebar> sidebar() async {
    sidebars++;
    await gate?.future;
    return super.sidebar();
  }
}

void changeTests() {
  test('a change that alters nothing on screen does not notify', () async {
    final db = _ChangingDb();
    final c = DeskController(db);
    await c.reload();
    var notified = 0;
    c.addListener(() => notified++);
    db.events.add(1);
    await Future<void>.delayed(Duration.zero);
    await Future<void>.delayed(Duration.zero);
    expect(db.sidebars, 2);
    expect(notified, 0);
    expect(c.unchangedReloads, 1);
    c.dispose();
  });

  test('a burst of changes while reloading costs one more reload', () async {
    final db = _ChangingDb();
    final c = DeskController(db);
    await c.reload();
    db.sidebars = 0;
    db.gate = Completer<void>();
    for (var i = 0; i < 10; i++) {
      db.events.add(i);
    }
    await Future<void>.delayed(Duration.zero);
    expect(db.sidebars, 1);
    db.gate!.complete();
    await Future<void>.delayed(const Duration(milliseconds: 20));
    expect(db.sidebars, 2);
    c.dispose();
  });
}
