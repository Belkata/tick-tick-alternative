// ignore: deprecated_member_use
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:utre/desk/theme.dart';

Finder ringBorder() => find.descendant(
  of: find.byType(FocusRing),
  matching: find.byWidgetPredicate(
    (w) => w is DecoratedBox && (w.decoration as BoxDecoration).border != null,
  ),
);

Future<void> pump(WidgetTester t, Brightness b) => t.pumpWidget(
  MaterialApp(
    theme: softTheme(b),
    home: Scaffold(
      body: Row(
        children: [
          FocusRing(
            child: InkWell(
              key: const Key('card'),
              onTap: () {},
              child: const SizedBox(width: 80, height: 40),
            ),
          ),
          IconButton(
            key: const Key('icon'),
            icon: const Icon(Icons.add),
            onPressed: () {},
          ),
        ],
      ),
    ),
  ),
);

void main() {
  tearDown(
    () => FocusManager.instance.highlightStrategy =
        FocusHighlightStrategy.automatic,
  );

  for (final b in Brightness.values) {
    testWidgets('keyboard focus draws a ring (${b.name})', (t) async {
      FocusManager.instance.highlightStrategy =
          FocusHighlightStrategy.alwaysTraditional;
      await pump(t, b);
      expect(ringBorder(), findsNothing);
      await t.sendKeyEvent(LogicalKeyboardKey.tab);
      await t.pump();
      final box = t.widget<DecoratedBox>(ringBorder());
      final border = (box.decoration as BoxDecoration).border! as Border;
      final ring =
          (b == Brightness.light ? SoftColors.light : SoftColors.dark).ring;
      expect(border.top.color, ring);
      expect(border.top.width, 2);

      // Next Tab moves to the icon button: the card's ring goes, the button's shows.
      await t.sendKeyEvent(LogicalKeyboardKey.tab);
      await t.pump();
      expect(ringBorder(), findsNothing);
      final btn = t.widget<IconButton>(find.byKey(const Key('icon')));
      expect(btn.onPressed, isNotNull);
      final style = Theme.of(t.element(find.byKey(const Key('icon'))))
          .iconButtonTheme
          .style!;
      final side = style.side!.resolve({WidgetState.focused});
      expect(side?.color, ring);
      expect(style.side!.resolve(<WidgetState>{}), isNull);
    });
  }

  testWidgets('a mouse or touch interaction shows no ring', (t) async {
    FocusManager.instance.highlightStrategy =
        FocusHighlightStrategy.alwaysTouch;
    await pump(t, Brightness.light);
    await t.tap(find.byKey(const Key('card')));
    await t.pump();
    expect(ringBorder(), findsNothing);
  });

  testWidgets('Tab runs through each pane before the next', (t) async {
    FocusManager.instance.highlightStrategy =
        FocusHighlightStrategy.alwaysTraditional;
    final nodes = List.generate(4, (_) => FocusNode());
    addTearDown(() {
      for (final n in nodes) {
        n.dispose();
      }
    });
    Widget btn(int i) =>
        TextButton(focusNode: nodes[i], onPressed: () {}, child: Text('b$i'));
    // Left pane: b0 at the top, b1 far down. Right pane: b2 at the top, b3 low.
    // Without groups reading order would visit b0, b2, b1, b3.
    await t.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: Row(
            children: [
              Expanded(
                child: FocusTraversalGroup(
                  child: Column(
                    children: [btn(0), const SizedBox(height: 200), btn(1)],
                  ),
                ),
              ),
              Expanded(
                child: FocusTraversalGroup(
                  child: Column(
                    children: [btn(2), const SizedBox(height: 100), btn(3)],
                  ),
                ),
              ),
            ],
          ),
        ),
      ),
    );
    final seen = <int>[];
    for (var i = 0; i < 4; i++) {
      await t.sendKeyEvent(LogicalKeyboardKey.tab);
      await t.pump();
      seen.add(nodes.indexWhere((n) => n.hasFocus));
    }
    expect(seen, [0, 1, 2, 3]);
  });
}
