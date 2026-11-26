import 'dart:ui' as ui;

// ignore: deprecated_member_use
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:utre/desk/task_list.dart';
import 'package:utre/desk/theme.dart';

import 'golden/fake_db.dart';
import 'golden/golden_utils.dart';

/// The done checkbox is centred on the title's FIRST line: with a facts line, wrapped titles,
/// a mouse or a touch screen, at normal and double text size.
void main() {
  setUpAll(loadAppFonts);

  for (final platform in [TargetPlatform.linux, TargetPlatform.android]) {
    for (final scale in [1.0, 2.0]) {
      testWidgets(
        'checkbox centred on the first title line ($platform, x$scale)',
        (t) async {
          t.view.physicalSize = const Size(480, 3000);
          t.view.devicePixelRatio = 1;
          addTearDown(t.view.reset);
          final rows = sampleTasks();
          await t.pumpWidget(
            MaterialApp(
              theme: softTheme(Brightness.light).copyWith(platform: platform),
              builder: (context, child) => MediaQuery(
                data: MediaQuery.of(context)
                    .copyWith(textScaler: TextScaler.linear(scale)),
                child: child!,
              ),
              home: Scaffold(
                body: ListView(
                  children: [
                    for (final r in rows)
                      TaskCard(
                        key: ValueKey(r.id),
                        row: r,
                        selected: false,
                        onTap: () {},
                        onCheck: () {},
                      ),
                  ],
                ),
              ),
            ),
          );
          for (final r in rows) {
            final title = t.renderObject<RenderParagraph>(
              find.byKey(Key('title-${r.id}')),
            );
            final first = title
                .getBoxesForSelection(
                  const TextSelection(baseOffset: 0, extentOffset: 1),
                  boxHeightStyle: ui.BoxHeightStyle.max,
                )
                .first;
            final lineCentre = title
                .localToGlobal(Offset(0, (first.top + first.bottom) / 2))
                .dy;
            final box = t.getRect(
              find.descendant(
                of: find.byKey(Key('check-${r.id}')),
                matching: find.byType(AnimatedContainer),
              ),
            );
            expect(
              (box.center.dy - lineCentre).abs(),
              lessThan(1.0),
              reason: '${r.title}: box ${box.center.dy} vs line $lineCentre',
            );
          }
        },
      );
    }
  }

  test(
    'FirstLineAlign pushes the text down when the box is taller than the line',
    () {
      final a = FirstLineAlign(box: 20, lineHeight: 18, textTop: 8);
      expect(a.lead, 7);
      expect(a.text, 8);
      final b = FirstLineAlign(box: 48, lineHeight: 18, textTop: 8);
      expect(b.lead, 0);
      expect(b.text, 15);
    },
  );
}
