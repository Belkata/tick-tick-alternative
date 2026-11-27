// ignore: deprecated_member_use
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:utre/desk/detail.dart';
import 'package:utre/desk/theme.dart';

/// Opens the `+ tag` field, types [text] and presses Enter; returns what was added.
Future<List<String>> typeAndEnter(WidgetTester t, String text) async {
  final added = <String>[];
  await t.pumpWidget(
    MaterialApp(
      theme: softTheme(Brightness.light),
      home: Scaffold(
        body: Align(
          alignment: Alignment.topLeft,
          child: AddTag(
            paths: const ['ai-safety', 'ai', 'home/garden', 'work'],
            onAdd: added.add,
          ),
        ),
      ),
    ),
  );
  await t.tap(find.byKey(const Key('add-tag')));
  await t.pumpAndSettle();
  await t.enterText(find.byKey(const Key('tag-field')), text);
  await t.pumpAndSettle();
  await t.testTextInput.receiveAction(TextInputAction.done);
  await t.pumpAndSettle();
  return added;
}

void main() {
  testWidgets('Enter on a partial tag adds only the suggestion', (t) async {
    expect(await typeAndEnter(t, 'ai-saf'), ['ai-safety']);
  });

  testWidgets('Enter on a full path prefers the exact match', (t) async {
    expect(await typeAndEnter(t, 'ai'), ['ai']);
  });

  testWidgets('Enter with no suggestion adds the typed text', (t) async {
    expect(await typeAndEnter(t, 'home/kitchen'), ['home/kitchen']);
  });
}
