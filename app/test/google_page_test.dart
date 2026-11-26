import 'dart:async';

// ignore: deprecated_member_use
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:utre/desk/google.dart';
import 'package:utre/desk/theme.dart';
import 'package:utre/src/rust/api/tasks.dart';

/// A stand-in for the core: a status, some calendars, and a log of what the page asked for.
class FakeGoogle implements GoogleSource {
  FakeGoogle({
    this.onPhone = true,
    this.available = true,
    this.state = const GoogleStatus(
      configured: true,
      signedIn: false,
      signingIn: false,
    ),
    List<GcalCalendar>? calendars,
  }) : cals = calendars ?? [];

  @override
  bool onPhone;
  @override
  bool available;
  GoogleStatus state;
  List<GcalCalendar> cals;
  final calls = <String>[];
  final _changes = StreamController<int>.broadcast();

  @override
  Future<GoogleStatus> status() async => state;

  @override
  Future<List<GcalCalendar>> calendars() async => cals;

  @override
  Future<String?> signIn() async {
    calls.add('signIn');
    state = const GoogleStatus(
      configured: true,
      signedIn: true,
      signingIn: false,
    );
    return null;
  }

  @override
  Future<void> signOut() async {
    calls.add('signOut');
    state = const GoogleStatus(
      configured: true,
      signedIn: false,
      signingIn: false,
    );
  }

  @override
  Future<void> sync() async => calls.add('sync');

  @override
  Future<void> select(String calendarId, bool on) async {
    calls.add('select $calendarId $on');
    cals = [
      for (final c in cals)
        c.id == calendarId
            ? GcalCalendar(
                id: c.id,
                summary: c.summary,
                color: c.color,
                selected: on,
              )
            : c,
    ];
  }

  @override
  Stream<int> changes() => _changes.stream;

  // The phone doesn't send tasks out.
  @override
  Future<GooglePushStatus?> pushStatus() async => null;

  @override
  Future<void> pushSet(bool enabled) async {}

  @override
  Future<void> pushNow() async {}
}

Future<void> show(WidgetTester tester, FakeGoogle g) async {
  tester.view.physicalSize = const Size(390 * 3, 844 * 3);
  tester.view.devicePixelRatio = 3;
  addTearDown(tester.view.reset);
  await tester.pumpWidget(
    MaterialApp(
      theme: softTheme(Brightness.light),
      home: Scaffold(body: GooglePage(source: g)),
    ),
  );
  await tester.pump();
  await tester.pump();
}

/// The page polls on a timer; remove it so the test can finish.
Future<void> close(WidgetTester tester) async {
  await tester.pumpWidget(const SizedBox());
}

String stateText(WidgetTester tester) =>
    tester.widget<Text>(find.byKey(const Key('google-state'))).data!;

void main() {
  testWidgets('without the OAuth client the phone page says what is missing', (
    tester,
  ) async {
    final g = FakeGoogle(
      state: const GoogleStatus(
        configured: false,
        signedIn: false,
        signingIn: false,
      ),
    );
    await show(tester, g);
    final text = stateText(tester);
    expect(text, contains('Android'));
    expect(text, contains('dev.utre.utre'));
    expect(text, contains('SHA-1'));
    expect(text, contains('utre.googleAndroidClientId'));
    expect(find.byKey(const Key('google-sign-in')), findsNothing);
    await close(tester);
  });

  testWidgets('desktop without the OAuth client names the file instead', (
    tester,
  ) async {
    final g = FakeGoogle(
      onPhone: false,
      state: const GoogleStatus(
        configured: false,
        signedIn: false,
        signingIn: false,
      ),
    );
    await show(tester, g);
    expect(stateText(tester), contains('google-client.json'));
    await close(tester);
  });

  testWidgets('without a daemon the desktop page explains how to start it', (
    tester,
  ) async {
    await show(tester, FakeGoogle(onPhone: false, available: false));
    expect(find.textContaining('utre-daemon'), findsOneWidget);
    await close(tester);
  });

  testWidgets('signed out, the button signs in and the page switches', (
    tester,
  ) async {
    final g = FakeGoogle();
    await show(tester, g);
    expect(find.byKey(const Key('google-sign-in')), findsOneWidget);
    // The phone has no browser link to copy.
    expect(find.byKey(const Key('google-copy-sign-in-link')), findsNothing);
    await tester.tap(find.byKey(const Key('google-sign-in')));
    await tester.pump();
    await tester.pump();
    expect(g.calls, ['signIn']);
    expect(find.byKey(const Key('google-sign-in')), findsNothing);
    expect(find.text('Signed in'), findsOneWidget);
    await close(tester);
  });

  testWidgets('signed in lists calendars; a switch selects and fetches', (
    tester,
  ) async {
    final g = FakeGoogle(
      state: const GoogleStatus(
        configured: true,
        signedIn: true,
        signingIn: false,
        lastSyncMs: 1,
      ),
      calendars: const [
        GcalCalendar(
          id: 'a',
          summary: 'Work',
          color: '#16a765',
          selected: true,
        ),
        GcalCalendar(id: 'b', summary: 'Family', selected: false),
      ],
    );
    await show(tester, g);
    expect(find.text('Work'), findsOneWidget);
    expect(find.text('Family'), findsOneWidget);
    await tester.tap(find.byKey(const Key('google-cal-b')));
    await tester.pump();
    await tester.pump();
    expect(g.calls, ['select b true', 'sync']);
    expect(
      tester.widget<Switch>(find.byKey(const Key('google-cal-b'))).value,
      isTrue,
    );
    await close(tester);
  });

  testWidgets('signing out asks first, then forgets', (tester) async {
    final g = FakeGoogle(
      state: const GoogleStatus(
        configured: true,
        signedIn: true,
        signingIn: false,
      ),
    );
    await show(tester, g);
    await tester.tap(find.byKey(const Key('google-sign-out')));
    await tester.pumpAndSettle();
    expect(find.text('Sign out of Google?'), findsOneWidget);
    expect(g.calls, isEmpty);
    await tester.tap(find.widgetWithText(FilledButton, 'Sign out'));
    await tester.pumpAndSettle();
    expect(g.calls, ['signOut']);
    expect(find.byKey(const Key('google-sign-in')), findsOneWidget);
    await close(tester);
  });

  testWidgets('a failed fetch shows its reason on the signed-in card', (
    tester,
  ) async {
    final g = FakeGoogle(
      state: const GoogleStatus(
        configured: true,
        signedIn: true,
        signingIn: false,
        lastError: 'no network',
      ),
    );
    await show(tester, g);
    expect(find.text('no network'), findsOneWidget);
    await close(tester);
  });
}
