import 'dart:async';

/// The save logic of a text field that writes as you type, without a write per keystroke:
/// edits are saved after a quiet [delay], and at once on [flush] (blur, Enter, switching to
/// another task, closing). A value that arrives from the store (a sync, another window) never
/// replaces text the user has typed and not yet saved.
class DebouncedEdit {
  DebouncedEdit({
    required String initial,
    this.delay = const Duration(milliseconds: 300),
    this.normalize = _same,
  }) : _synced = normalize(initial);

  /// Quiet time before a save; null saves only on [flush].
  final Duration? delay;

  /// What the store makes of a text (a title is trimmed): the echo of a save compares equal
  /// to what was typed, so it never rewrites the field under the cursor.
  final String Function(String) normalize;
  static String _same(String s) => s;

  /// The last value known to be in the store.
  String _synced;
  String? _pending;
  Future<void>? Function(String)? _saver;
  Timer? _timer;

  /// Typed text that is not saved yet.
  bool get dirty => _pending != null;

  /// The user changed the text to [text]. [save] is bound now, so a flush after the field
  /// moved on to another record still writes to the record that was edited.
  void edited(String text, Future<void>? Function(String) save) {
    if (normalize(text) == _synced && !dirty) return;
    _pending = text;
    _saver = save;
    _timer?.cancel();
    final d = delay;
    if (d != null) _timer = Timer(d, flush);
  }

  /// Save what is pending now.
  void flush() {
    _timer?.cancel();
    final text = _pending;
    final save = _saver;
    _pending = null;
    _saver = null;
    if (text == null || save == null || normalize(text) == _synced) return;
    _synced = normalize(text);
    save(text);
  }

  /// The store holds [value] and the field shows [current]. Returns the text to show, or
  /// null to keep what the user typed.
  String? incoming(String value, String current) {
    if (dirty) return null;
    _synced = normalize(value);
    return normalize(current) == _synced ? null : value;
  }

  void dispose() {
    flush();
    _timer?.cancel();
  }
}
