@Tags(['golden'])
library;

// ignore: deprecated_member_use
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:utre/desk/shell.dart';

import 'fake_db.dart';
import 'golden_utils.dart';

Future<void> pumpShell(
  WidgetTester t,
  Brightness b,
  Size size, {
  TargetPlatform platform = TargetPlatform.linux,
  String? select,
}) async {
  setSurface(t, size);
  await t.pumpWidget(
    themed(
      DeskShell(db: FakeDb(tasks: sampleTasks(DateTime.now()))),
      b,
      platform: platform,
    ),
  );
  // The shell reads its saved window prefs through a platform channel (real event loop).
  await t.runAsync(
    () => Future<void>.delayed(const Duration(milliseconds: 100)),
  );
  await t.pump();
  await t.pump(const Duration(milliseconds: 400));
  if (select != null) {
    await t.tap(find.text(select));
    await t.pump();
    await t.pump(const Duration(milliseconds: 400));
  }
}

void main() {
  useTolerantComparator();

  goldenTheme('shell-desktop', (t, b, file) async {
    await pumpShell(
      t,
      b,
      const Size(1280, 780),
      select:
          'Prepare the quarterly review slides with the numbers from finance',
    );
    await expectLater(find.byType(MaterialApp), matchesGoldenFile(file));
  });

  goldenTheme('shell-phone', (t, b, file) async {
    await pumpShell(
      t,
      b,
      const Size(390, 780),
      platform: TargetPlatform.android,
    );
    await expectLater(find.byType(MaterialApp), matchesGoldenFile(file));
  });
}
