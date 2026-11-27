import 'dart:convert';
import 'dart:io';

import 'package:path_provider/path_provider.dart';

/// Per-device window choices, kept in `desk.json` next to the app's support files
/// (like `CalendarPrefs`), so they never sync: the detail pane's width and which parent
/// tags are folded in the sidebar.
class DeskPrefs {
  DeskPrefs({this.detailWidth, Set<String>? collapsedTags})
    : collapsedTags = collapsedTags ?? {};

  /// The task detail pane's width on wide windows; null for the default.
  double? detailWidth;

  /// Parent tags folded in the sidebar (ids).
  Set<String> collapsedTags;

  static Future<File> _file() async {
    final dir = await getApplicationSupportDirectory();
    return File('${dir.path}/desk.json');
  }

  static Future<DeskPrefs> load() async {
    try {
      final f = await _file();
      if (!await f.exists()) return DeskPrefs();
      final m = jsonDecode(await f.readAsString()) as Map<String, dynamic>;
      return DeskPrefs(
        detailWidth: (m['detail_width'] as num?)?.toDouble(),
        collapsedTags: {
          ...?(m['collapsed_tags'] as List?)?.whereType<String>(),
        },
      );
    } catch (_) {
      return DeskPrefs();
    }
  }

  Future<void> save() async {
    try {
      final f = await _file();
      await f.parent.create(recursive: true);
      await f.writeAsString(
        jsonEncode({
          'detail_width': detailWidth,
          'collapsed_tags': collapsedTags.toList(),
        }),
      );
    } catch (_) {
      // A lost preference is harmless.
    }
  }
}
