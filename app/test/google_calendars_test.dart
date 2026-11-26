// ignore_for_file: deprecated_member_use
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:utre/desk/google_calendars.dart';
import 'package:utre/desk/theme.dart';
import 'package:utre/src/rust/api/tasks.dart';

import 'google_page_test.dart' show FakeGoogle;

const _signedIn = GoogleStatus(
  configured: true,
  signedIn: true,
  signingIn: false,
  lastSyncMs: 1,
);

List<GcalCalendar> _cals() => const [
  GcalCalendar(
    id: 'me',
    summary: 'Boris',
    color: '#16a765',
    selected: true,
    primary: true,
    accessRole: 'owner',
  ),
  GcalCalendar(
    id: 'fam',
    summary: 'Family',
    selected: false,
    primary: false,
    accessRole: 'writer',
  ),
  GcalCalendar(
    id: 'hol',
    summary: 'Holidays',
    selected: true,
    primary: false,
    accessRole: 'reader',
  ),
];

Future<GoogleCalendarsController> _pump(
  WidgetTester tester,
  FakeGoogle g, {
  required bool phone,
}) async {
  final controller = GoogleCalendarsController(g);
  addTearDown(controller.dispose);
  await tester.pumpWidget(
    MaterialApp(
      theme: softTheme(Brightness.light),
      home: Scaffold(
        body: Builder(
          builder: (context) => Column(
            children: [
              if (phone)
                TextButton(
                  key: const Key('open'),
                  onPressed: () =>
                      showGoogleCalendarsSheet(context, controller),
                  child: const Text('open'),
                )
              else
                GoogleCalendarsMenu(controller: controller),
            ],
          ),
        ),
      ),
    ),
  );
  await tester.pump();
  return controller;
}

void main() {
  testWidgets('desktop: no button until signed in to Google', (tester) async {
    final g = FakeGoogle(onPhone: false, calendars: _cals());
    await _pump(tester, g, phone: false);
    expect(find.byKey(const Key('cal-google-calendars')), findsNothing);
    g.state = _signedIn;
    g.pingChanges();
    await tester.pump();
    await tester.pump();
    expect(find.byKey(const Key('cal-google-calendars')), findsOneWidget);
  });

  testWidgets('desktop: groups, toggles, fetches when switched on', (
    tester,
  ) async {
    final g = FakeGoogle(onPhone: false, state: _signedIn, calendars: _cals());
    await _pump(tester, g, phone: false);
    await tester.tap(find.byKey(const Key('cal-google-calendars')));
    await tester.pumpAndSettle();
    expect(find.text('My calendars'), findsOneWidget);
    expect(find.text('Other calendars'), findsOneWidget);
    expect(find.text('Applies to this device only.'), findsOneWidget);
    // Hiding one doesn't fetch; showing one does, at once.
    await tester.tap(find.byKey(const Key('gcal-vis-hol')).first);
    await tester.pumpAndSettle();
    expect(g.calls, ['select hol false']);
    await tester.tap(find.byKey(const Key('gcal-vis-fam')).first);
    await tester.pumpAndSettle();
    expect(g.calls, ['select hol false', 'select fam true', 'sync']);
    final fam = tester.widget<CheckboxMenuButton>(
      find.byType(CheckboxMenuButton).at(1),
    );
    expect(fam.value, isTrue);
  });

  testWidgets('desktop: keyboard opens the menu and Space toggles', (
    tester,
  ) async {
    final g = FakeGoogle(onPhone: false, state: _signedIn, calendars: _cals());
    await _pump(tester, g, phone: false);
    await tester.sendKeyEvent(LogicalKeyboardKey.tab);
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.pumpAndSettle();
    expect(find.text('My calendars'), findsOneWidget);
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowDown);
    await tester.sendKeyEvent(LogicalKeyboardKey.space);
    await tester.pumpAndSettle();
    expect(g.calls.first, startsWith('select '));
    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await tester.pumpAndSettle();
    expect(find.text('My calendars'), findsNothing);
  });

  testWidgets('phone: bottom sheet with 48 px rows', (tester) async {
    final g = FakeGoogle(state: _signedIn, calendars: _cals());
    await _pump(tester, g, phone: true);
    await tester.tap(find.byKey(const Key('open')));
    await tester.pumpAndSettle();
    expect(find.text('Google calendars'), findsOneWidget);
    expect(find.text('Holidays'), findsOneWidget);
    for (final id in ['me', 'fam', 'hol']) {
      final size = tester.getSize(find.byKey(Key('gcal-vis-$id')));
      expect(size.height, greaterThanOrEqualTo(48));
    }
    await tester.tap(find.byKey(const Key('gcal-vis-fam')));
    await tester.pumpAndSettle();
    expect(g.calls, ['select fam true', 'sync']);
    await tester.tap(find.byKey(const Key('gcal-vis-me')));
    await tester.pumpAndSettle();
    expect(g.calls.last, 'select me false');
  });
}
