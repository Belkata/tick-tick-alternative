import 'dart:convert';
import 'dart:io';

import 'package:path_provider/path_provider.dart';

/// Per-device window choices, kept in `desk.json` next to the app's support files
/// (like `CalendarPrefs`), so they never sync: for now the detail pane's width.
class DeskPrefs {
  DeskPrefs({this.detailWidth});

  /// The task detail pane's width on wide windows; null for the default.
  double? detailWidth;

  static Future<File> _file() async {
    final dir = await getApplicationSupportDirectory();
    return File('${dir.path}/desk.json');
  }

  static Future<DeskPrefs> load() async {
    try {
      final f = await _file();
      if (!await f.exists()) return DeskPrefs();
      final m = jsonDecode(await f.readAsString()) as Map<String, dynamic>;
      return DeskPrefs(detailWidth: (m['detail_width'] as num?)?.toDouble());
    } catch (_) {
      return DeskPrefs();
    }
  }

  Future<void> save() async {
    try {
      final f = await _file();
      await f.parent.create(recursive: true);
      await f.writeAsString(jsonEncode({'detail_width': detailWidth}));
    } catch (_) {
      // A lost preference is harmless.
    }
  }
}
