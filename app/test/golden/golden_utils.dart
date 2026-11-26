import 'dart:convert';
import 'dart:io';

// ignore: deprecated_member_use
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:utre/desk/theme.dart';

/// Goldens are generated and compared on Linux only: text and anti-aliasing differ a little
/// between platforms. Elsewhere (the Mac) the tests skip. CI is the source of truth.
final Object goldenSkip = Platform.isLinux
    ? false
    : 'goldens are only compared on Linux (see test/golden/README.md)';

/// A byte-for-byte match is too strict across Linux machines (GPU-less rasterisers differ in
/// the last bit of some anti-aliased edges): allow a tiny fraction of differing pixels.
class _TolerantComparator extends LocalFileComparator {
  _TolerantComparator(super.testFile, this.tolerance);

  /// The fraction of pixels allowed to differ (0.002 = 0.2 %).
  final double tolerance;

  @override
  Future<bool> compare(Uint8List imageBytes, Uri golden) async {
    final result = await GoldenFileComparator.compareLists(
      imageBytes,
      await getGoldenBytes(golden),
    );
    if (result.passed || result.diffPercent <= tolerance) return true;
    final error = await generateFailureOutput(result, golden, basedir);
    throw FlutterError(error);
  }
}

/// Load every font the app declares (Manrope, and Material Icons through
/// `uses-material-design`) from the test asset bundle. Without this the test renderer draws
/// every glyph as an Ahem box. Nothing is fetched from the network.
Future<void> loadAppFonts() async {
  TestWidgetsFlutterBinding.ensureInitialized();
  final manifest =
      json.decode(await rootBundle.loadString('FontManifest.json')) as List;
  for (final entry in manifest) {
    final family = (entry as Map)['family'] as String;
    final loader = FontLoader(family);
    for (final font in entry['fonts'] as List) {
      loader.addFont(rootBundle.load((font as Map)['asset'] as String));
    }
    await loader.load();
  }
}

/// Call once in `main()` of a golden test file.
void useTolerantComparator([double tolerance = 0.002]) {
  setUpAll(loadAppFonts);
  final base = goldenFileComparator;
  if (base is LocalFileComparator) {
    goldenFileComparator = _TolerantComparator(
      Uri.parse('${base.basedir}/x_test.dart'),
      tolerance,
    );
  }
}

/// Runs [body] once per theme as `<name>-light` and `<name>-dark`.
void goldenTheme(
  String name,
  Future<void> Function(WidgetTester t, Brightness b, String file) body,
) {
  for (final b in Brightness.values.reversed) {
    final mode = b == Brightness.light ? 'light' : 'dark';
    testWidgets(
      '$name ($mode)',
      (t) => body(t, b, 'goldens/$name-$mode.png'),
      skip: goldenSkip != false,
      tags: ['golden'],
    );
  }
}

/// Size the test window (logical px at a device pixel ratio of 1, so the PNGs stay small).
void setSurface(WidgetTester t, Size size) {
  t.view.physicalSize = size;
  t.view.devicePixelRatio = 1;
  addTearDown(t.view.reset);
  // flutter_test turns real shadows into hard black outlines unless told otherwise.
  debugDisableShadows = false;
  addTearDown(() => debugDisableShadows = true);
}

/// A Soft-themed app around [home], like the real app's MaterialApp.
Widget themed(
  Widget home,
  Brightness b, {
  TargetPlatform platform = TargetPlatform.linux,
}) => MaterialApp(
  debugShowCheckedModeBanner: false,
  theme: softTheme(b).copyWith(platform: platform),
  home: home,
);
