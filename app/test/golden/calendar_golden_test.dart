@Tags(['golden'])
library;

import 'package:flutter_test/flutter_test.dart';
// ignore: deprecated_member_use
import 'package:flutter/material.dart';
import 'package:utre/calendar/calendar_page.dart';
import 'package:utre/desk/controller.dart';

import 'fake_db.dart';
import 'golden_utils.dart';

Future<void> pumpCalendar(
  WidgetTester t,
  Brightness b, {
  required Size size,
  String? view,
  TargetPlatform platform = TargetPlatform.linux,
}) async {
  setSurface(t, size);
  final c = DeskController(
    FakeDb(blocks: sampleBlocks(), google: sampleGoogle()),
    now: () => goldenNow,
  );
  addTearDown(c.dispose);
  await t.pumpWidget(
    themed(
      Scaffold(body: CalendarPage(c: c)),
      b,
      platform: platform,
    ),
  );
  // The page reads its saved prefs through a platform channel, which answers on the real
  // event loop (nothing is saved in a test): let that finish before looking.
  await t.runAsync(
    () => Future<void>.delayed(const Duration(milliseconds: 100)),
  );
  await t.pump();
  await t.pump(const Duration(milliseconds: 100));
  if (view != null) {
    // On a phone the views sit in a menu behind the picker.
    if (size.width < 760) {
      await t.tap(find.byKey(const Key('view-picker')));
      await t.pump();
      await t.pump(const Duration(milliseconds: 700));
    }
    await t.tap(find.byKey(Key('view-$view')));
    await t.pump();
    await t.pump(const Duration(milliseconds: 50));
  }
  await t.pump(const Duration(milliseconds: 400));
  await t.pump(const Duration(milliseconds: 400));
}

void main() {
  useTolerantComparator();

  for (final view in ['Day', 'Week', 'Month']) {
    goldenTheme('calendar-${view.toLowerCase()}', (t, b, file) async {
      await pumpCalendar(
        t,
        b,
        size: const Size(1100, 760),
        view: view == 'Week' ? null : view,
      );
      await expectLater(find.byType(MaterialApp), matchesGoldenFile(file));
    });
  }

  goldenTheme('calendar-phone-day', (t, b, file) async {
    await pumpCalendar(
      t,
      b,
      size: const Size(390, 780),
      platform: TargetPlatform.android,
    );
    await expectLater(find.byType(MaterialApp), matchesGoldenFile(file));
  });

  goldenTheme('calendar-phone-2days', (t, b, file) async {
    await pumpCalendar(
      t,
      b,
      size: const Size(390, 780),
      view: '2 Days',
      platform: TargetPlatform.android,
    );
    await expectLater(find.byType(MaterialApp), matchesGoldenFile(file));
  });
}
