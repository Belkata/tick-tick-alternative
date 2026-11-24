import 'dart:io';

import 'package:flutter/services.dart';

/// Talks to the Android home-screen widget (see MainActivity.kt).
/// Does nothing on other platforms or in tests.
class WidgetBridge {
  static const _channel = MethodChannel('tick_alt/widget');

  static bool get supported => !_isTest && Platform.isAndroid;

  static final _isTest = Platform.environment.containsKey('FLUTTER_TEST');

  /// Redraw placed widgets after the app changed tasks.
  static Future<void> refresh() async {
    if (!supported) return;
    try {
      await _channel.invokeMethod<void>('refresh');
    } on MissingPluginException {
      // Running without the Android host (e.g. an integration test harness).
    }
  }

  /// Ask the launcher to place the widget. Returns false if the launcher can't.
  static Future<bool> pin() async {
    if (!supported) return false;
    return await _channel.invokeMethod<bool>('pin') ?? false;
  }
}
