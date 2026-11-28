// ignore: deprecated_member_use
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:utre/desk/theme.dart';
import 'package:utre/desk/updates.dart';

class FakeUpdates implements UpdatesSource {
  FakeUpdates(this.s);
  Map<String, dynamic> s;
  final calls = <String>[];

  @override
  Future<Map<String, dynamic>?> state() async => s;
  @override
  Future<void> check() async {
    calls.add('check');
    s = {...s, 'status': 'downloading', 'latest': '0.9.0'};
  }

  @override
  Future<void> setAuto(bool on) async {
    calls.add('auto $on');
    s = {...s, 'auto': on};
  }

  @override
  Future<void> installSettings() async => calls.add('settings');
}

Widget _app(UpdatesSource u) => MaterialApp(
  theme: softTheme(Brightness.light),
  home: Scaffold(body: UpdatesPage(source: u)),
);

void main() {
  final base = {
    'status': 'uptodate',
    'current': '0.8.10',
    'auto': true,
    'canInstall': true,
  };

  testWidgets('shows the version, checks, and toggles automatic installs', (t) async {
    final u = FakeUpdates(base);
    await t.pumpWidget(_app(u));
    await t.pump();
    expect(find.text('Utre 0.8.10'), findsOneWidget);
    expect(find.text('Up to date'), findsOneWidget);
    expect(find.byKey(const Key('update-permission')), findsNothing);

    await t.tap(find.byKey(const Key('update-check')));
    await t.pump();
    expect(u.calls, ['check']);
    expect(find.text('Downloading 0.9.0…'), findsOneWidget);

    await t.tap(find.byKey(const Key('update-auto')));
    await t.pump();
    expect(u.calls, ['check', 'auto false']);
    await t.pumpWidget(const SizedBox());
  });

  testWidgets('without install permission it says so and opens settings', (t) async {
    final u = FakeUpdates({...base, 'canInstall': false, 'status': 'error', 'message': 'Install failed'});
    await t.pumpWidget(_app(u));
    await t.pump();
    expect(find.text('Install failed'), findsOneWidget);
    await t.tap(find.byKey(const Key('update-permission-button')));
    expect(u.calls, ['settings']);
    await t.pumpWidget(const SizedBox());
  });

  testWidgets('a release-key update explains the one-time reinstall', (t) async {
    final u = FakeUpdates({...base, 'status': 'wrong_key', 'latest': '0.9.0'});
    await t.pumpWidget(_app(u));
    await t.pump();
    expect(find.textContaining('uninstall Utre and install 0.9.0 from GitHub'), findsOneWidget);
    await t.pumpWidget(const SizedBox());
  });
}
