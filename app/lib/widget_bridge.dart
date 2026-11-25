import 'dart:io';

import 'package:flutter/services.dart';

/// Talks to the Android side (see MainActivity.kt): the home-screen widget, reminder
/// alarms, and the device facts the core needs.
/// Does nothing on other platforms or in tests.
class WidgetBridge {
  static const _channel = MethodChannel('utre/widget');

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

  /// The app changed tasks: redraw widgets and reschedule reminder alarms.
  static Future<void> changed() async {
    await refresh();
    await _call<void>('reminders');
  }

  /// The device's IANA zone (`Europe/Sofia`); null off Android.
  static Future<String?> timeZone() => _call<String>('timeZone');

  /// What paired devices call this phone (`Google Pixel 8`).
  static Future<String?> deviceName() => _call<String>('deviceName');

  /// Where the sync node's key lives (next to the database).
  static Future<String?> keyPath() => _call<String>('keyPath');

  /// Ask for notification permission once (Android 13+), for reminders.
  static Future<void> askNotifications() => _call<void>('askNotifications');

  /// Text shared to the app from another app (Android share sheet): what arrived before
  /// Flutter was ready, then each later share through [onShared].
  static Future<String?> takeShared() => _call<String>('takeShared');

  static void onShared(void Function(String text) handler) {
    if (!supported) return;
    _channel.setMethodCallHandler((call) async {
      if (call.method == 'shared' && call.arguments is String) {
        handler(call.arguments as String);
      }
    });
  }

  static Future<T?> _call<T>(String method) async {
    if (!supported) return null;
    try {
      return await _channel.invokeMethod<T>(method);
    } on MissingPluginException {
      return null;
    }
  }

  /// Ask the launcher to place the widget. Returns false if the launcher can't.
  static Future<bool> pin() async {
    if (!supported) return false;
    return await _channel.invokeMethod<bool>('pin') ?? false;
  }
}
