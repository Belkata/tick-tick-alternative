import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:utre/desk/theme.dart';

void main() {
  for (final b in Brightness.values) {
    test('${b.name}: dialogs and sheets use the untinted panel colour', () {
      final t = softTheme(b);
      final panel = t.extension<SoftColors>()!.panel;
      expect(t.dialogTheme.backgroundColor, panel);
      expect(t.bottomSheetTheme.backgroundColor, panel);
      expect(t.bottomSheetTheme.modalBackgroundColor, panel);
      expect(t.bottomSheetTheme.surfaceTintColor, Colors.transparent);
    });
  }
}
