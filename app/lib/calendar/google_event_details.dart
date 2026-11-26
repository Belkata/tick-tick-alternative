// ignore_for_file: deprecated_member_use
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:url_launcher/url_launcher.dart';

import '../desk/theme.dart';
import '../src/rust/api/tasks.dart';
import '../widget_bridge.dart';

/// Opens an event's web link: the Google Calendar app on a phone that has it, else the
/// browser. Tests replace [GoogleEventLink.open].
class GoogleEventLink {
  static Future<bool> Function(String url) open = _open;

  static Future<bool> _open(String url) async {
    final uri = Uri.tryParse(url);
    if (uri == null || !(uri.isScheme('https') || uri.isScheme('http'))) {
      return false;
    }
    // Android: the calendar app takes the link when it can (see MainActivity.kt).
    if (await WidgetBridge.openInCalendarApp(url)) return true;
    try {
      return await launchUrl(uri, mode: LaunchMode.externalApplication);
    } on Exception {
      return false;
    }
  }
}

/// Google's description is HTML-ish. Show it as plain text: tags dropped, line breaks and
/// list items kept, common entities decoded. Nothing is rendered as markup or opened.
String plainDescription(String html) {
  var t = html
      .replaceAll(RegExp(r'<\s*br\s*/?\s*>', caseSensitive: false), '\n')
      .replaceAll(
        RegExp(r'<\s*/\s*(p|div|li|h[1-6]|tr)\s*>', caseSensitive: false),
        '\n',
      )
      .replaceAll(RegExp(r'<\s*li[^>]*>', caseSensitive: false), '• ')
      .replaceAll(RegExp(r'<[^>]*>'), '');
  const entities = {
    '&nbsp;': ' ',
    '&lt;': '<',
    '&gt;': '>',
    '&quot;': '"',
    '&#39;': "'",
    '&apos;': "'",
  };
  entities.forEach((k, v) => t = t.replaceAll(k, v));
  t = t.replaceAll('&amp;', '&');
  return t.replaceAll(RegExp(r'\n{3,}'), '\n\n').trim();
}

/// What a tap on a Google event shows. Read-only: Google owns the event.
class GoogleEventDetails extends StatelessWidget {
  const GoogleEventDetails({
    super.key,
    required this.event,
    required this.calendarName,
    required this.when,
    required this.onClose,
    required this.onToast,
    this.sheet = false,
  });

  final GcalEvent event;
  final String? calendarName;

  /// The date and time range (or "all day"), already formatted.
  final String when;
  final VoidCallback onClose;
  final void Function(String message) onToast;

  /// Phone sheet: 48 px buttons.
  final bool sheet;

  Future<void> _open() async {
    final ok = await GoogleEventLink.open(event.htmlLink!);
    if (!ok) onToast('Could not open Google Calendar');
  }

  @override
  Widget build(BuildContext context) {
    final s = context.soft;
    final link = event.htmlLink;
    final description = event.description == null
        ? ''
        : plainDescription(event.description!);
    final location = event.location?.trim() ?? '';
    final minimum = sheet ? const Size(0, 48) : null;
    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Text(
          event.summary,
          key: const Key('google-event-title'),
          style: manrope(17, FontWeight.w800, color: s.fg),
        ),
        const SizedBox(height: 8),
        Text(when, style: manrope(13.5, FontWeight.w600, color: s.fg)),
        const SizedBox(height: 10),
        Row(
          children: [
            Container(
              key: const Key('google-event-dot'),
              width: 9,
              height: 9,
              decoration: BoxDecoration(
                color: hexColor(event.color, s.mute),
                shape: BoxShape.circle,
              ),
            ),
            const SizedBox(width: 8),
            Flexible(
              child: Text(
                calendarName ?? 'Google Calendar',
                style: manrope(12.5, FontWeight.w500, color: s.mute),
              ),
            ),
          ],
        ),
        if (location.isNotEmpty) ...[
          const SizedBox(height: 10),
          Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Icon(Icons.place_outlined, size: 16, color: s.mute),
              const SizedBox(width: 8),
              Expanded(
                child: SelectableText(
                  location,
                  key: const Key('google-event-location'),
                  style: manrope(13, FontWeight.w500, color: s.fg),
                ),
              ),
            ],
          ),
        ],
        if (description.isNotEmpty) ...[
          const SizedBox(height: 10),
          ConstrainedBox(
            constraints: const BoxConstraints(maxHeight: 160),
            child: SingleChildScrollView(
              child: SelectableText(
                description,
                key: const Key('google-event-description'),
                style: manrope(13, FontWeight.w500, color: s.fg),
              ),
            ),
          ),
        ],
        const SizedBox(height: 12),
        Text(
          'From Google Calendar. Change it there; Utre shows it read-only.',
          style: manrope(12, FontWeight.w500, color: s.mute),
        ),
        const SizedBox(height: 14),
        Wrap(
          alignment: WrapAlignment.end,
          spacing: 8,
          runSpacing: 8,
          children: [
            if (link != null)
              TextButton.icon(
                key: const Key('google-event-copy-link'),
                style: TextButton.styleFrom(minimumSize: minimum),
                icon: const Icon(Icons.link, size: 16),
                label: const Text('Copy link'),
                onPressed: () {
                  Clipboard.setData(ClipboardData(text: link));
                  onToast('Link to the event copied');
                },
              ),
            if (link != null)
              FilledButton.icon(
                key: const Key('google-event-open'),
                autofocus: true,
                style: FilledButton.styleFrom(minimumSize: minimum),
                icon: const Icon(Icons.open_in_new, size: 16),
                label: const Text('Open in Google Calendar'),
                onPressed: _open,
              ),
            TextButton(
              key: const Key('google-event-close'),
              style: TextButton.styleFrom(minimumSize: minimum),
              onPressed: onClose,
              child: const Text('Close'),
            ),
          ],
        ),
      ],
    );
  }
}

/// Desktop: a compact dialog (Esc closes it). Phone: a bottom sheet.
Future<void> showGoogleEventDetails(
  BuildContext context, {
  required GcalEvent event,
  required String? calendarName,
  required String when,
  required bool phone,
  required void Function(String message) onToast,
}) {
  Widget body(BuildContext ctx) => GoogleEventDetails(
    event: event,
    calendarName: calendarName,
    when: when,
    sheet: phone,
    onToast: onToast,
    onClose: () => Navigator.pop(ctx),
  );
  if (phone) {
    return showModalBottomSheet<void>(
      context: context,
      isScrollControlled: true,
      showDragHandle: true,
      shape: const RoundedRectangleBorder(
        borderRadius: BorderRadius.vertical(top: Radius.circular(22)),
      ),
      builder: (ctx) => SafeArea(
        child: Padding(
          key: const Key('google-event-sheet'),
          padding: const EdgeInsets.fromLTRB(20, 0, 20, 16),
          child: SingleChildScrollView(child: body(ctx)),
        ),
      ),
    );
  }
  return showDialog<void>(
    context: context,
    builder: (ctx) => Dialog(
      key: const Key('google-event-dialog'),
      shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(18)),
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 400),
        child: Padding(
          padding: const EdgeInsets.all(22),
          child: SingleChildScrollView(child: body(ctx)),
        ),
      ),
    ),
  );
}
