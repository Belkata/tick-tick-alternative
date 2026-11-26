// ignore_for_file: deprecated_member_use
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:utre/calendar/google_event_details.dart';
import 'package:utre/desk/theme.dart';
import 'package:utre/src/rust/api/tasks.dart';

GcalEvent _event({String? link, String? location, String? description}) =>
    GcalEvent(
      calendarId: 'me',
      id: 'e1',
      summary: 'Dentist',
      startAt: 0,
      endAt: 3600000,
      allDay: false,
      htmlLink: link,
      color: '#8e24aa',
      location: location,
      description: description,
    );

Future<List<String>> _pump(
  WidgetTester tester,
  GcalEvent event, {
  required bool phone,
  Brightness brightness = Brightness.light,
}) async {
  final toasts = <String>[];
  await tester.pumpWidget(
    MaterialApp(
      theme: softTheme(brightness),
      home: Scaffold(
        body: Builder(
          builder: (context) => TextButton(
            key: const Key('go'),
            onPressed: () => showGoogleEventDetails(
              context,
              event: event,
              calendarName: 'Family',
              when: 'Tue, 1 Dec · 09:00–10:00',
              phone: phone,
              onToast: toasts.add,
            ),
            child: const Text('go'),
          ),
        ),
      ),
    ),
  );
  await tester.tap(find.byKey(const Key('go')));
  await tester.pumpAndSettle();
  return toasts;
}

void main() {
  final opened = <String>[];
  var result = true;
  setUp(() {
    opened.clear();
    result = true;
    GoogleEventLink.open = (url) async {
      opened.add(url);
      return result;
    };
  });

  for (final phone in [false, true]) {
    final name = phone ? 'phone sheet' : 'desktop dialog';
    testWidgets('$name shows the facts and opens the link', (tester) async {
      final toasts = await _pump(
        tester,
        _event(
          link: 'https://www.google.com/calendar/event?eid=abc',
          location: 'Main St 1',
          description:
              '<p>Bring the <b>card</b> &amp; ID</p><ul><li>one</li></ul>',
        ),
        phone: phone,
      );
      expect(
        find.byKey(Key(phone ? 'google-event-sheet' : 'google-event-dialog')),
        findsOneWidget,
      );
      expect(find.text('Dentist'), findsOneWidget);
      expect(find.text('Tue, 1 Dec · 09:00–10:00'), findsOneWidget);
      expect(find.text('Family'), findsOneWidget);
      expect(find.text('Main St 1'), findsOneWidget);
      expect(find.textContaining('Bring the card & ID'), findsOneWidget);
      expect(find.textContaining('<b>'), findsNothing);
      await tester.tap(find.byKey(const Key('google-event-open')));
      await tester.pump();
      expect(opened, ['https://www.google.com/calendar/event?eid=abc']);
      expect(toasts, isEmpty);
    });

    testWidgets('$name has no button without a link', (tester) async {
      await _pump(tester, _event(), phone: phone, brightness: Brightness.dark);
      expect(find.byKey(const Key('google-event-open')), findsNothing);
      expect(find.byKey(const Key('google-event-copy-link')), findsNothing);
      expect(find.byKey(const Key('google-event-location')), findsNothing);
      expect(find.byKey(const Key('google-event-description')), findsNothing);
      expect(find.byKey(const Key('google-event-close')), findsOneWidget);
    });

    testWidgets('$name tells when opening fails and closes with Esc', (
      tester,
    ) async {
      result = false;
      final toasts = await _pump(
        tester,
        _event(link: 'https://g/x'),
        phone: phone,
      );
      await tester.tap(find.byKey(const Key('google-event-open')));
      await tester.pump();
      expect(toasts, ['Could not open Google Calendar']);
      await tester.sendKeyEvent(LogicalKeyboardKey.escape);
      await tester.pumpAndSettle();
      expect(find.byKey(const Key('google-event-title')), findsNothing);
    });
  }

  testWidgets('Enter activates the focused open button', (tester) async {
    await _pump(tester, _event(link: 'https://g/x'), phone: false);
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.pump();
    expect(opened, ['https://g/x']);
  });

  test('plainDescription drops markup', () {
    expect(
      plainDescription('a<br>b<script>x</script> &lt;ok&gt;'),
      'a\nbx <ok>',
    );
  });
}
