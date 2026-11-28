import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

/// Talks to the Android side (see MainActivity.kt): the home-screen widget, reminder
/// alarms, and the device facts the core needs.
/// Does nothing on other platforms or in tests.
class WidgetBridge {
  static const _channel = MethodChannel('utre/widget');

  static bool get supported =>
      forceSupported || (!_isTest && Platform.isAndroid);

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

  /// The task a widget row was tapped on: what arrived before Flutter was ready, then
  /// each later tap through [listen]'s `onOpenTask`.
  static Future<String?> takeOpenTask() => _call<String>('takeOpenTask');

  /// Messages the Android side pushes: shared text and widget row taps. One handler
  /// serves both, because a channel has only one.
  static void listen({
    void Function(String text)? onShared,
    void Function(String taskId)? onOpenTask,
  }) {
    if (!supported) return;
    _channel.setMethodCallHandler((call) async {
      final arg = call.arguments;
      if (arg is! String) return;
      if (call.method == 'shared') onShared?.call(arg);
      if (call.method == 'openTask') onOpenTask?.call(arg);
    });
  }

  /// Tests pretend to be on Android so the channel is used.
  @visibleForTesting
  static bool forceSupported = false;

  static Future<T?> _call<T>(String method) async {
    if (!supported) return null;
    try {
      return await _channel.invokeMethod<T>(method);
    } on MissingPluginException {
      return null;
    }
  }

  /// Google's consent screen (the phone has no browser sign-in). Answers `ok`,
  /// `cancelled`, `not_configured` or `error: ...`; null off Android.
  static Future<String?> googleSignIn() => _call<String>('googleSignIn');

  /// Opens [url] in the Google Calendar app only (an ACTION_VIEW pinned to its package).
  /// False when the app is missing, can't take the link, or off Android.
  static Future<bool> openInCalendarApp(String url) async {
    if (!supported) return false;
    try {
      return await _channel.invokeMethod<bool>('openInCalendarApp', url) ??
          false;
    } on MissingPluginException {
      return false;
    }
  }

  /// The updater's state (see Updater.kt): `status` (idle, checking, uptodate, downloading,
  /// installing, ready, needs_permission, wrong_key, error), `latest`, `message`, `current`, `auto`,
  /// `canInstall`. Null off Android.
  static Future<Map<String, dynamic>?> updateState() async {
    if (!supported) return null;
    try {
      final m = await _channel.invokeMapMethod<String, dynamic>('updateState');
      return m;
    } on MissingPluginException {
      return null;
    }
  }

  /// "Check for updates": looks on any network and installs what it finds.
  static Future<void> updateCheck() => _call<void>('updateCheck');

  /// Whether new releases install on their own (per device).
  static Future<void> updateSetAuto(bool on) async {
    if (!supported) return;
    try {
      await _channel.invokeMethod<void>('updateSetAuto', on);
    } on MissingPluginException {
      // Not on Android.
    }
  }

  /// Android's "install unknown apps" screen for Utre.
  static Future<void> updateInstallSettings() => _call<void>('updateInstallSettings');

  /// Ask the launcher to place the widget. Returns false if the launcher can't.
  static Future<bool> pin() async {
    if (!supported) return false;
    return await _channel.invokeMethod<bool>('pin') ?? false;
  }
}
