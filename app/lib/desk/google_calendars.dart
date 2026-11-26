// Which Google calendars show in the calendar view (plan/19). The choice is per device:
// the core keeps it next to the Google cache, never as a synced op. Until a calendar is
// chosen here it follows Google's own "selected" flag.
//
// [GoogleCalendarsController] is the shared state (signed in? which calendars?),
// [GoogleCalendarsMenu] the desktop toolbar button and [showGoogleCalendarsSheet] the
// phone's bottom sheet.

import 'dart:async';

// ignore: deprecated_member_use
import 'package:flutter/material.dart';

import '../src/rust/api/tasks.dart';
import 'google.dart' show GoogleSource;
import 'theme.dart';

class GoogleCalendarsController extends ChangeNotifier {
  GoogleCalendarsController(this.source) {
    try {
      _changes = source.changes().listen((_) => refresh());
    } catch (_) {}
    refresh();
  }

  final GoogleSource source;
  StreamSubscription<int>? _changes;
  bool _disposed = false;
  bool _signedIn = false;
  List<GcalCalendar> _calendars = const [];

  List<GcalCalendar> get calendars => _calendars;

  /// Worth showing: Google works here, this device is signed in and has calendars.
  bool get visible => _signedIn && _calendars.isNotEmpty;

  Future<void> refresh() async {
    var signedIn = false;
    var calendars = const <GcalCalendar>[];
    try {
      if (source.available) {
        signedIn = (await source.status()).signedIn;
        if (signedIn) calendars = await source.calendars();
      }
    } catch (_) {}
    if (_disposed) return;
    if (signedIn == _signedIn && _same(calendars, _calendars)) return;
    _signedIn = signedIn;
    _calendars = calendars;
    notifyListeners();
  }

  static bool _same(List<GcalCalendar> a, List<GcalCalendar> b) {
    if (a.length != b.length) return false;
    for (var i = 0; i < a.length; i++) {
      if (a[i].id != b[i].id ||
          a[i].selected != b[i].selected ||
          a[i].summary != b[i].summary ||
          a[i].color != b[i].color) {
        return false;
      }
    }
    return true;
  }

  /// Shows or hides a calendar. Showing one fetches its events at once. Returns an error
  /// message, or null.
  Future<String?> setVisible(GcalCalendar c, bool on) async {
    _calendars = [
      for (final o in _calendars)
        o.id == c.id
            ? GcalCalendar(
                id: o.id,
                summary: o.summary,
                color: o.color,
                selected: on,
                primary: o.primary,
                accessRole: o.accessRole,
              )
            : o,
    ];
    notifyListeners();
    String? error;
    try {
      await source.select(c.id, on);
      if (on) await source.sync();
    } catch (e) {
      error = '$e'.replaceFirst(RegExp(r'^AnyhowException\(?'), '');
    }
    await refresh();
    return error;
  }

  @override
  void dispose() {
    _disposed = true;
    _changes?.cancel();
    super.dispose();
  }
}

/// Mine first (owner), the rest after, each as Google lists them: primary first.
({List<GcalCalendar> mine, List<GcalCalendar> other}) _groups(
  List<GcalCalendar> all,
) {
  bool mine(GcalCalendar c) => c.primary || c.accessRole == 'owner';
  return (
    mine: [
      for (final c in all)
        if (mine(c)) c,
    ],
    other: [
      for (final c in all)
        if (!mine(c)) c,
    ],
  );
}

void _toast(BuildContext context, String message) {
  ScaffoldMessenger.of(context)
    ..hideCurrentSnackBar()
    ..showSnackBar(SnackBar(content: Text(message)));
}

Future<void> _toggle(
  BuildContext context,
  GoogleCalendarsController g,
  GcalCalendar c,
  bool on,
) async {
  final error = await g.setVisible(c, on);
  if (error != null && context.mounted) _toast(context, error);
}

Widget _dot(SoftColors s, GcalCalendar c) => Container(
  width: 12,
  height: 12,
  decoration: BoxDecoration(
    color: hexColor(c.color, s.mute),
    shape: BoxShape.circle,
  ),
);

/// Desktop: a "Calendars" button in the calendar toolbar that opens a menu with a
/// checkbox per Google calendar. Arrow keys move, Space toggles, Escape closes. It
/// is empty space when this device isn't signed in to Google.
class GoogleCalendarsMenu extends StatelessWidget {
  const GoogleCalendarsMenu({
    super.key,
    required this.controller,
    this.compact = false,
  });

  final GoogleCalendarsController controller;

  /// Icon only, for a narrow toolbar.
  final bool compact;

  @override
  Widget build(BuildContext context) {
    final s = context.soft;
    return ListenableBuilder(
      listenable: controller,
      builder: (context, _) {
        if (!controller.visible) return const SizedBox.shrink();
        final groups = _groups(controller.calendars);
        Widget header(String text) => Padding(
          padding: const EdgeInsets.fromLTRB(16, 10, 16, 4),
          child: Text(
            text,
            style: manrope(11.5, FontWeight.w700, color: s.mute),
          ),
        );
        Widget row(GcalCalendar c) => CheckboxMenuButton(
          key: Key('gcal-vis-${c.id}'),
          value: c.selected,
          closeOnActivate: false,
          style: MenuItemButton.styleFrom(
            minimumSize: const Size(220, 40),
            foregroundColor: s.fg,
            textStyle: manrope(13, FontWeight.w600),
          ),
          onChanged: (on) => _toggle(context, controller, c, on ?? false),
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              _dot(s, c),
              const SizedBox(width: 10),
              ConstrainedBox(
                constraints: const BoxConstraints(maxWidth: 240),
                child: Text(c.summary, overflow: TextOverflow.ellipsis),
              ),
            ],
          ),
        );
        return MenuAnchor(
          style: MenuStyle(
            backgroundColor: WidgetStatePropertyAll(s.panel),
            side: WidgetStatePropertyAll(BorderSide(color: s.line)),
            shape: WidgetStatePropertyAll(
              RoundedRectangleBorder(borderRadius: BorderRadius.circular(14)),
            ),
          ),
          menuChildren: [
            if (groups.mine.isNotEmpty) header('My calendars'),
            for (final c in groups.mine) row(c),
            if (groups.other.isNotEmpty) header('Other calendars'),
            for (final c in groups.other) row(c),
            Padding(
              padding: const EdgeInsets.fromLTRB(16, 8, 16, 8),
              child: Text(
                'Applies to this device only.',
                style: manrope(11.5, FontWeight.w500, color: s.mute),
              ),
            ),
          ],
          builder: (context, menu, _) {
            void toggle() => menu.isOpen ? menu.close() : menu.open();
            final style = OutlinedButton.styleFrom(
              side: BorderSide(color: s.line),
              backgroundColor: s.panel,
              foregroundColor: s.fg,
              shape: RoundedRectangleBorder(
                borderRadius: BorderRadius.circular(10),
              ),
              padding: EdgeInsets.symmetric(horizontal: compact ? 8 : 12),
              minimumSize: Size(compact ? 32 : 0, 32),
              textStyle: manrope(13, FontWeight.w700),
            );
            return Tooltip(
              message: 'Choose which Google calendars show',
              child: compact
                  ? OutlinedButton(
                      key: const Key('cal-google-calendars'),
                      style: style,
                      onPressed: toggle,
                      child: const Icon(Icons.layers_outlined, size: 18),
                    )
                  : OutlinedButton.icon(
                      key: const Key('cal-google-calendars'),
                      style: style,
                      onPressed: toggle,
                      icon: const Icon(Icons.layers_outlined, size: 18),
                      label: const Text('Calendars'),
                    ),
            );
          },
        );
      },
    );
  }
}

/// Phone: the same choice as a bottom sheet with 48 px rows.
Future<void> showGoogleCalendarsSheet(
  BuildContext context,
  GoogleCalendarsController controller,
) {
  final s = context.soft;
  return showModalBottomSheet<void>(
    context: context,
    isScrollControlled: true,
    backgroundColor: s.panel,
    showDragHandle: true,
    shape: const RoundedRectangleBorder(
      borderRadius: BorderRadius.vertical(top: Radius.circular(22)),
    ),
    builder: (sheet) => ListenableBuilder(
      listenable: controller,
      builder: (sheet, _) {
        final groups = _groups(controller.calendars);
        Widget header(String text) => Padding(
          padding: const EdgeInsets.fromLTRB(20, 12, 20, 4),
          child: Text(text, style: manrope(12, FontWeight.w700, color: s.mute)),
        );
        Widget row(GcalCalendar c) => InkWell(
          key: Key('gcal-vis-${c.id}'),
          onTap: () => _toggle(sheet, controller, c, !c.selected),
          child: ConstrainedBox(
            constraints: const BoxConstraints(minHeight: 48),
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 12),
              child: Row(
                children: [
                  Checkbox(
                    value: c.selected,
                    onChanged: (on) =>
                        _toggle(sheet, controller, c, on ?? false),
                  ),
                  _dot(s, c),
                  const SizedBox(width: 12),
                  Expanded(
                    child: Text(
                      c.summary,
                      overflow: TextOverflow.ellipsis,
                      style: manrope(15, FontWeight.w600, color: s.fg),
                    ),
                  ),
                ],
              ),
            ),
          ),
        );
        return SafeArea(
          child: ConstrainedBox(
            constraints: BoxConstraints(
              maxHeight: MediaQuery.sizeOf(sheet).height * 0.75,
            ),
            child: ListView(
              shrinkWrap: true,
              children: [
                Padding(
                  padding: const EdgeInsets.fromLTRB(20, 0, 20, 4),
                  child: Text(
                    'Google calendars',
                    style: manrope(18, FontWeight.w800, color: s.fg),
                  ),
                ),
                if (groups.mine.isNotEmpty) header('My calendars'),
                for (final c in groups.mine) row(c),
                if (groups.other.isNotEmpty) header('Other calendars'),
                for (final c in groups.other) row(c),
                Padding(
                  padding: const EdgeInsets.fromLTRB(20, 10, 20, 16),
                  child: Text(
                    'Applies to this device only.',
                    style: manrope(12, FontWeight.w500, color: s.mute),
                  ),
                ),
              ],
            ),
          ),
        );
      },
    ),
  );
}
