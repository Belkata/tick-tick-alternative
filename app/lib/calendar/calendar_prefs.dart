import 'dart:convert';
import 'dart:io';

import 'package:path_provider/path_provider.dart';

/// Per-device calendar choices (plan/18): which filter feeds the Unscheduled panel and
/// the last view. Kept in a small JSON file next to the app's support files, not in the
/// task database, so they never sync.
class CalendarPrefs {
  CalendarPrefs({this.unscheduledFilter, this.view});

  /// A saved filter's id, or null for the built-in "no date" default.
  String? unscheduledFilter;

  /// `Day`, `Week` or `Month`.
  String? view;

  static Future<File> _file() async {
    final dir = await getApplicationSupportDirectory();
    return File('${dir.path}/calendar.json');
  }

  static Future<CalendarPrefs> load() async {
    try {
      final f = await _file();
      if (!await f.exists()) return CalendarPrefs();
      final m = jsonDecode(await f.readAsString()) as Map<String, dynamic>;
      return CalendarPrefs(
        unscheduledFilter: m['unscheduled_filter'] as String?,
        view: m['view'] as String?,
      );
    } catch (_) {
      return CalendarPrefs();
    }
  }

  Future<void> save() async {
    try {
      final f = await _file();
      await f.parent.create(recursive: true);
      await f.writeAsString(
        jsonEncode({'unscheduled_filter': unscheduledFilter, 'view': view}),
      );
    } catch (_) {
      // A lost preference is harmless.
    }
  }
}
