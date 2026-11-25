import 'dart:async';

// ignore: deprecated_member_use
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../src/rust/api/tasks.dart';
import 'filter_builder.dart' show confirm;
import 'theme.dart';

/// Google Calendar page (M5b, plan/06 and plan/19): sign in, pick which of your Google
/// calendars show in Utre's calendar. The sign-in and the fetching run in utre-daemon, so
/// everything here goes through it; without a daemon the page says so.
class GooglePage extends StatefulWidget {
  const GooglePage({super.key, required this.db});

  final TaskDb db;

  @override
  State<GooglePage> createState() => _GooglePageState();
}

class _GooglePageState extends State<GooglePage> {
  GoogleStatus? _status;
  List<GcalCalendar> _calendars = const [];
  String? _error;
  String? _signInUrl;
  bool _busy = false;
  StreamSubscription<int>? _changes;
  Timer? _clock;
  int _ticks = 0;

  TaskDb get db => widget.db;

  @override
  void initState() {
    super.initState();
    _changes = db.changes().listen((_) => _load());
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
    if (!db.googleAvailable()) {
      if (mounted) setState(() => _status = null);
      return;
    }
    try {
      final status = await db.googleStatus();
      final calendars = await db.gcalCalendars();
      if (!mounted) return;
      setState(() {
        _status = status;
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
    final url = await db.googleSignIn();
    if (mounted) setState(() => _signInUrl = url);
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
    await _run(db.googleSignOut);
  }

  Future<void> _toggle(GcalCalendar c, bool on) => _run(() async {
    await db.gcalSelect(calendarId: c.id, selected: on);
    // Showing a calendar: fetch its events now rather than at the next 5-minute poll.
    if (on) await db.googleSync();
  });

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
                (db.googleAvailable()
                    ? 'Loading…'
                    : 'Google Calendar runs in utre-daemon, which isn\'t running for '
                          'this database (and isn\'t on the phone yet).\n'
                          'Start it with: systemctl --user start utre'),
            style: manrope(13.5, FontWeight.w500, color: s.mute),
          ),
        ),
      );
    } else if (!status.configured) {
      body.add(
        _Card(
          child: Text(
            'Google isn\'t set up on this device. Save the OAuth client file '
            '(Google Cloud → Credentials → Desktop app → Download JSON) as '
            '~/.config/utre/google-client.json and restart utre-daemon.',
            style: manrope(13.5, FontWeight.w500, color: s.mute),
          ),
        ),
      );
    } else if (!status.signedIn) {
      body.addAll([
        _Card(
          child: Text(
            status.signingIn
                ? 'Waiting for you to finish signing in, in your browser…'
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
          'Google may warn that the app is unverified. Choose Advanced, then '
          'continue: it only means Google hasn\'t reviewed Utre.',
          style: manrope(12, FontWeight.w500, color: s.mute),
        ),
      ]);
    } else {
      final line = status.lastSyncMs != null
          ? 'Fetched ${_ago(status.lastSyncMs!)}'
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
              onPressed: _busy ? null : () => _run(db.googleSync),
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
      ]);
    }
    return ListView(padding: const EdgeInsets.all(20), children: body);
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

String _ago(int ms) {
  final s = (DateTime.now().millisecondsSinceEpoch - ms) ~/ 1000;
  if (s < 60) return 'just now';
  if (s < 3600) return '${s ~/ 60} min ago';
  if (s < 86400) return '${s ~/ 3600} h ago';
  return '${s ~/ 86400} days ago';
}

String _clean(Object e) => '$e'
    .replaceFirst(RegExp(r'^(AnyhowException|Exception)\(?'), '')
    .replaceFirst(RegExp(r'\)$'), '');
