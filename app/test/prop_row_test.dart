// ignore: deprecated_member_use
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:utre/desk/detail.dart';
import 'package:utre/desk/theme.dart';

Future<void> pump(WidgetTester t, double width) => t.pumpWidget(
  MaterialApp(
    theme: softTheme(Brightness.light),
    home: Scaffold(
      body: Align(
        alignment: Alignment.topLeft,
        child: SizedBox(
          width: width,
          child: const PropCard(
            children: [
              PropRow(
                icon: Icons.event_outlined,
                label: 'Due',
                child: Text('Fri 4 Dec 14:30', key: Key('value')),
              ),
              PropRow(
                icon: Icons.repeat_rounded,
                label: 'Repeat',
                child: Text('Never'),
              ),
            ],
          ),
        ),
      ),
    ),
  ),
);

void main() {
  testWidgets('the detail pane at its minimum width drops the row icons', (
    t,
  ) async {
    await pump(t, 250);
    expect(find.byIcon(Icons.event_outlined), findsNothing);
    expect(find.byIcon(Icons.repeat_rounded), findsNothing);
    final box = t.getRect(find.byKey(const Key('value')));
    // The value starts right after the label column, not 28 px later.
    expect(box.left, lessThan(100));
  });

  testWidgets('a wider pane keeps them', (t) async {
    await pump(t, 400);
    expect(find.byIcon(Icons.event_outlined), findsOneWidget);
    expect(find.byIcon(Icons.repeat_rounded), findsOneWidget);
  });
}
