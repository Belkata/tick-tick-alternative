@Tags(['golden'])
library;

// ignore: deprecated_member_use
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:utre/desk/date_picker.dart';

import 'fake_db.dart';
import 'golden_utils.dart';

class _Host extends StatelessWidget {
  const _Host();

  @override
  Widget build(BuildContext context) => Center(
    child: TextButton(
      key: const Key('open'),
      onPressed: () => showWhenPicker(
        context,
        title: 'Due date',
        isStart: false,
        initial: When(at(3), const TimeOfDay(hour: 14, minute: 30)),
        now: goldenNow,
      ),
      child: const Text('open'),
    ),
  );
}

Future<void> openPicker(
  WidgetTester t,
  Brightness b,
  Size size,
  TargetPlatform platform,
) async {
  setSurface(t, size);
  await t.pumpWidget(
    themed(const Scaffold(body: _Host()), b, platform: platform),
  );
  await t.tap(find.byKey(const Key('open')));
  await t.pump();
  // Dialog and sheet slide in: let them land.
  await t.pump(const Duration(milliseconds: 800));
}

void main() {
  useTolerantComparator();

  goldenTheme('date-picker-desktop', (t, b, file) async {
    await openPicker(t, b, const Size(760, 720), TargetPlatform.linux);
    await expectLater(find.byType(MaterialApp), matchesGoldenFile(file));
  });

  goldenTheme('date-picker-phone', (t, b, file) async {
    await openPicker(t, b, const Size(390, 780), TargetPlatform.android);
    await expectLater(find.byType(MaterialApp), matchesGoldenFile(file));
  });
}
