import 'ago.dart';

import 'dart:async';

// ignore: deprecated_member_use
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../src/rust/api/tasks.dart';
import '../widget_bridge.dart';
import 'filter_builder.dart' show confirm;
import 'theme.dart';

/// What the Google page needs, so a widget test can stand in for the core.
abstract class GoogleSource {
  /// Whether Google can work here: utre-daemon is serving the database, or this is the
  /// phone, which fetches in-process.
  bool get available;

  /// The phone signs in through Google's consent screen, not a browser link.
  bool get onPhone;

  Future<GoogleStatus> status();
  Future<List<GcalCalendar>> calendars();

  /// Starts the sign-in. Desktop: opens the browser and returns its URL. Phone: shows the
  /// consent screen, returns null once the user agreed (or backed out), then fetches.
  Future<String?> signIn();
  Future<void> signOut();
  Future<void> sync();
  Future<void> select(String calendarId, bool on);
  Stream<int> changes();

  /// Sending tasks out to Google (slice 4) runs in utre-daemon, so the phone has none:
  /// [pushStatus] answers null there and the page hides the switch.
  Future<GooglePushStatus?> pushStatus();
  Future<void> pushSet(bool enabled);
  Future<void> pushNow();
}

/// The real thing: [TaskDb] on desktop (through utre-daemon) and on the phone (in-process).
class TaskDbGoogle implements GoogleSource {
  TaskDbGoogle(this.db);

  final TaskDb db;

  @override
  bool get available => db.googleAvailable();

  @override
  bool get onPhone => WidgetBridge.supported;

  @override
  Future<GoogleStatus> status() => db.googleStatus();

  @override
  Future<List<GcalCalendar>> calendars() => db.gcalCalendars();

  @override
  Future<String?> signIn() async {
    if (!onPhone) return db.googleSignIn();
    final answer = await WidgetBridge.googleSignIn();
    if (answer == 'ok') {
      await db.googleSync();
    } else if (answer == 'not_configured') {
      throw 'Google isn\'t set up in this build';
    } else if (answer != null && answer.startsWith('error: ')) {
      throw answer.substring(7);
    }
    // Anything else: the user backed out.
    return null;
  }

  @override
  Future<void> signOut() => db.googleSignOut();

  @override
  Future<void> sync() => db.googleSync();

  @override
  Future<void> select(String calendarId, bool on) =>
      db.gcalSelect(calendarId: calendarId, selected: on);

  @override
  Stream<int> changes() => db.changes();

  @override
  Future<GooglePushStatus?> pushStatus() async =>
      onPhone ? null : await db.googlePushStatus();

  @override
  Future<void> pushSet(bool enabled) => db.googlePushSet(enabled: enabled);

  @override
  Future<void> pushNow() => db.googlePushNow();
}

/// Google Calendar page (M5b, plan/06 and plan/19): sign in, pick which of your Google
/// calendars show in Utre's calendar. On desktop the sign-in and the fetching run in
/// utre-daemon (the page says so when it isn't running); the phone does both itself.
class GooglePage extends StatefulWidget {
  const GooglePage({super.key, required this.source});

  final GoogleSource source;

  @override
  State<GooglePage> createState() => _GooglePageState();
}

class _GooglePageState extends State<GooglePage> {
  GoogleStatus? _status;
  GooglePushStatus? _push;
  List<GcalCalendar> _calendars = const [];
  String? _error;
  String? _signInUrl;
  bool _busy = false;
  StreamSubscription<int>? _changes;
  Timer? _clock;
  int _ticks = 0;

  GoogleSource get g => widget.source;

  @override
  void initState() {
    super.initState();
    _changes = g.changes().listen((_) => _load());
    // While the browser sign-in is open, look often; otherwise keep "fetched 2 min ago"
    // current.
    _clock = Timer.periodic(const Duration(seconds: 3), (_) {
      if (_status?.signingIn == true || ++_ticks % 10 == 0) _load();
    });
    _load();
  }

  @override
  void dispose() {
    _changes?.cancel();
    _clock?.cancel();
    super.dispose();
  }

  Future<void> _load() async {
    if (!g.available) {
      if (mounted) setState(() => _status = null);
      return;
    }
    try {
      final status = await g.status();
      final calendars = await g.calendars();
      final push = status.signedIn ? await g.pushStatus() : null;
      if (!mounted) return;
      setState(() {
        _status = status;
        _push = push;
        _calendars = calendars;
        _error = null;
        if (!status.signingIn) _signInUrl = null;
      });
    } catch (e) {
      if (mounted) setState(() => _error = _clean(e));
    }
  }

  Future<void> _run(Future<void> Function() action) async {
    setState(() => _busy = true);
    try {
      await action();
    } catch (e) {
      _snack(_clean(e));
    } finally {
      if (mounted) setState(() => _busy = false);
      await _load();
    }
  }

  void _snack(String text) {
    if (mounted) {
      ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text(text)));
    }
  }

  Future<void> _signIn() => _run(() async {
    final url = await g.signIn();
    if (mounted && url != null) setState(() => _signInUrl = url);
  });

  Future<void> _signOut() async {
    final ok = await confirm(
      context,
      title: 'Sign out of Google?',
      body:
          'This device forgets its Google sign-in and removes the Google events it '
          'saved. Your tasks and your Google account are not touched.',
      action: 'Sign out',
    );
    if (!ok) return;
    await _run(g.signOut);
  }

  Future<void> _toggle(GcalCalendar c, bool on) => _run(() async {
    await g.select(c.id, on);
    // Showing a calendar: fetch its events now rather than at the next poll.
    if (on) await g.sync();
  });

  Future<void> _togglePush(bool on) => _run(() async {
    await g.pushSet(on);
    // Sending: don't wait for the daemon's next pass.
    if (on) await g.pushNow();
  });

  String _pushLine(GooglePushStatus p) {
    if (!p.enabled) return 'Off. Nothing is sent to Google.';
    final when = p.lastPushMs == null
        ? 'Not sent yet'
        : 'Sent ${agoText(p.lastPushMs!)}';
    return '$when. ${p.unchanged + p.created + p.updated} events on Google.';
  }

  @override
  Widget build(BuildContext context) {
    final s = context.soft;
    final status = _status;
    final body = <Widget>[
      Text('Google Calendar', style: manrope(19, FontWeight.w800, color: s.fg)),
      const SizedBox(height: 14),
    ];
    if (status == null) {
      body.add(
        _Card(
          child: Text(
            _error ??
                (g.available
                    ? 'Loading…'
                    : 'Google Calendar runs in utre-daemon, which isn\'t running for '
                          'this database.\n'
                          'Start it with: systemctl --user start utre'),
            style: manrope(13.5, FontWeight.w500, color: s.mute),
          ),
        ),
      );
    } else if (!status.configured) {
      body.add(
        _Card(
          child: Text(
            g.onPhone
                ? 'Google isn\'t set up in this build of Utre. Missing: an Android '
                      'OAuth client in Google Cloud for the package dev.utre.utre and '
                      'this app\'s signing key (SHA-1), and its client id in the build '
                      '(utre.googleAndroidClientId). See plan/19 in the repository.'
                : 'Google isn\'t set up on this device. Save the OAuth client file '
                      '(Google Cloud → Credentials → Desktop app → Download JSON) as '
                      '~/.config/utre/google-client.json and restart utre-daemon.',
            key: const Key('google-state'),
            style: manrope(13.5, FontWeight.w500, color: s.mute),
          ),
        ),
      );
    } else if (!status.signedIn) {
      body.addAll([
        _Card(
          child: Text(
            status.signingIn
                ? (g.onPhone
                      ? 'Waiting for you to finish signing in…'
                      : 'Waiting for you to finish signing in, in your browser…')
                : 'Show your Google Calendar events next to your tasks. Utre asks '
                      'to read your calendars, and to manage only the one it '
                      'creates. Events stay on your devices.',
            key: const Key('google-state'),
            style: manrope(13.5, FontWeight.w500, color: s.mute),
          ),
        ),
        const SizedBox(height: 4),
        Wrap(
          spacing: 8,
          runSpacing: 8,
          crossAxisAlignment: WrapCrossAlignment.center,
          children: [
            FilledButton.icon(
              key: const Key('google-sign-in'),
              onPressed: _busy || status.signingIn ? null : _signIn,
              icon: const Icon(Icons.login, size: 18),
              label: const Text('Sign in with Google'),
            ),
            if (status.signingIn && _signInUrl != null)
              TextButton.icon(
                key: const Key('google-copy-sign-in-link'),
                onPressed: () {
                  Clipboard.setData(ClipboardData(text: _signInUrl!));
                  _snack('Sign-in link copied');
                },
                icon: const Icon(Icons.copy, size: 16),
                label: const Text('Copy the sign-in link'),
              ),
          ],
        ),
        const SizedBox(height: 10),
        Text(
          g.onPhone
              ? 'Google may warn that the app is unverified. Choose Advanced, then '
                    'continue: it only means Google hasn\'t reviewed Utre. The phone '
                    'checks your calendars about every 15 minutes.'
              : 'Google may warn that the app is unverified. Choose Advanced, then '
                    'continue: it only means Google hasn\'t reviewed Utre.',
          style: manrope(12, FontWeight.w500, color: s.mute),
        ),
      ]);
    } else {
      final line = status.lastSyncMs != null
          ? 'Fetched ${agoText(status.lastSyncMs!)}'
          : (_calendars.any((c) => c.selected)
                ? 'Not fetched yet'
                : 'Choose the calendars to show');
      body.addAll([
        _Card(
          child: Row(
            children: [
              Icon(Icons.event_available, color: s.accent),
              const SizedBox(width: 12),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text('Signed in', style: manrope(14, FontWeight.w700)),
                    Text(
                      line,
                      key: const Key('google-state'),
                      style: manrope(12, FontWeight.w500, color: s.mute),
                    ),
                    if (status.lastError != null)
                      Text(
                        status.lastError!,
                        style: manrope(12, FontWeight.w500, color: s.late),
                      ),
                  ],
                ),
              ),
            ],
          ),
        ),
        const SizedBox(height: 4),
        Wrap(
          spacing: 8,
          runSpacing: 8,
          crossAxisAlignment: WrapCrossAlignment.center,
          children: [
            OutlinedButton.icon(
              key: const Key('google-sync'),
              onPressed: _busy ? null : () => _run(g.sync),
              icon: const Icon(Icons.sync, size: 18),
              label: const Text('Fetch now'),
            ),
            OutlinedButton.icon(
              key: const Key('google-sign-out'),
              onPressed: _busy ? null : _signOut,
              icon: const Icon(Icons.logout, size: 18),
              label: const Text('Sign out'),
            ),
            if (_busy)
              const Padding(
                padding: EdgeInsets.all(8),
                child: SizedBox(
                  width: 18,
                  height: 18,
                  child: CircularProgressIndicator(strokeWidth: 2),
                ),
              ),
          ],
        ),
        const SizedBox(height: 18),
        Text(
          'Calendars to show',
          style: manrope(12, FontWeight.w700, color: s.mute),
        ),
        const SizedBox(height: 8),
        if (_calendars.isEmpty)
          Text(
            'No calendars yet. Press “Fetch now”.',
            style: manrope(13, FontWeight.w500, color: s.mute),
          ),
        for (final c in _calendars)
          _Card(
            child: Row(
              children: [
                Container(
                  width: 10,
                  height: 10,
                  decoration: BoxDecoration(
                    color: hexColor(c.color, s.mute),
                    shape: BoxShape.circle,
                  ),
                ),
                const SizedBox(width: 12),
                Expanded(
                  child: Text(
                    c.summary,
                    style: manrope(14, FontWeight.w600),
                    overflow: TextOverflow.ellipsis,
                  ),
                ),
                Switch(
                  key: Key('google-cal-${c.id}'),
                  value: c.selected,
                  onChanged: _busy ? null : (on) => _toggle(c, on),
                ),
              ],
            ),
          ),
        const SizedBox(height: 10),
        Text(
          'Google events show read-only in the calendar. Edit them in Google Calendar.',
          style: manrope(12, FontWeight.w500, color: s.mute),
        ),
        const SizedBox(height: 18),
        Text(
          'Tasks to Google',
          style: manrope(12, FontWeight.w700, color: s.mute),
        ),
        const SizedBox(height: 8),
        if (_push != null)
          _Card(
            child: Row(
              children: [
                Icon(Icons.upload, color: s.accent),
                const SizedBox(width: 12),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        'Send timed tasks to Google',
                        style: manrope(14, FontWeight.w700),
                      ),
                      Text(
                        _pushLine(_push!),
                        key: const Key('google-push-state'),
                        style: manrope(12, FontWeight.w500, color: s.mute),
                      ),
                      if (_push!.lastError != null)
                        Text(
                          _push!.lastError!,
                          style: manrope(12, FontWeight.w500, color: s.late),
                        ),
                    ],
                  ),
                ),
                Switch(
                  key: const Key('google-push'),
                  value: _push!.enabled,
                  onChanged: _busy ? null : _togglePush,
                ),
              ],
            ),
          ),
        const SizedBox(height: 10),
        Text(
          'Tasks with a date (all-day ones too) go to a calendar called “Tasks” '
          'that Utre creates in your Google account: repeats as the next few '
          'months of single events, done tasks with a ✓ for 30 days. It is '
          'one-way: changes made to those events in Google are overwritten.',
          style: manrope(12, FontWeight.w500, color: s.mute),
        ),
      ]);
    }
    return ListView(
      padding: EdgeInsets.fromLTRB(
        24,
        18,
        24,
        24 + MediaQuery.viewPaddingOf(context).bottom,
      ),
      children: body,
    );
  }
}

class _Card extends StatelessWidget {
  const _Card({required this.child});

  final Widget child;

  @override
  Widget build(BuildContext context) {
    final s = context.soft;
    return Padding(
      padding: const EdgeInsets.only(bottom: 8),
      child: Material(
        color: s.panel,
        elevation: s.cardShadow ? 0.6 : 0,
        shadowColor: const Color(0x22141E3C),
        borderRadius: BorderRadius.circular(12),
        child: Padding(padding: const EdgeInsets.all(14), child: child),
      ),
    );
  }
}

String _clean(Object e) => '$e'
    .replaceFirst(RegExp(r'^(AnyhowException|Exception)\(?'), '')
    .replaceFirst(RegExp(r'\)$'), '');
