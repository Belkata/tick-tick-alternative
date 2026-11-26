@Tags(['golden'])
library;

// ignore: deprecated_member_use
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:utre/desk/task_list.dart';

import 'fake_db.dart';
import 'golden_utils.dart';

/// Every card state in one column: priorities, overdue / today facts, tags, checklist,
/// subtasks, a repeat, a long title, a done task, and a selected one.
Widget cards() => Scaffold(
  body: ListView(
    padding: const EdgeInsets.all(12),
    children: [
      for (final r in sampleTasks())
        TaskCard(row: r, selected: r.id == 't2', onTap: () {}, onCheck: () {}),
    ],
  ),
);

void main() {
  useTolerantComparator();

  goldenTheme('task-card', (t, b, file) async {
    setSurface(t, const Size(480, 760));
    await t.pumpWidget(themed(cards(), b));
    await t.pump(const Duration(milliseconds: 300));
    await expectLater(find.byType(MaterialApp), matchesGoldenFile(file));
  });

  goldenTheme('task-card-phone', (t, b, file) async {
    setSurface(t, const Size(390, 780));
    await t.pumpWidget(themed(cards(), b, platform: TargetPlatform.android));
    await t.pump(const Duration(milliseconds: 300));
    await expectLater(find.byType(MaterialApp), matchesGoldenFile(file));
  });
}
