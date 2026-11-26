// The chosen style (plan/09): Soft, with Material outlined tag chips.
// Tokens come from plan/ui-styles/styles.html (`.s-soft .mock` and `.s-mix`).
// ignore: deprecated_member_use
import 'package:flutter/material.dart';

/// Colours the Material scheme has no slot for.
@immutable
class SoftColors extends ThemeExtension<SoftColors> {
  const SoftColors({
    required this.bg,
    required this.side,
    required this.panel,
    required this.fg,
    required this.mute,
    required this.line,
    required this.selFg,
    required this.accent,
    required this.today,
    required this.late,
    required this.p1,
    required this.p2,
    required this.p3,
    required this.cardShadow,
  });

  final Color bg,
      side,
      panel,
      fg,
      mute,
      line,
      selFg,
      accent,
      today,
      late,
      p1,
      p2,
      p3;
  final bool cardShadow;

  static const light = SoftColors(
    bg: Color(0xFFF7F8FB),
    side: Color(0xFFEEF1F7),
    panel: Color(0xFFFFFFFF),
    fg: Color(0xFF1F2433),
    // Text greys and the teal/red used as text clear 4.5:1 on the light panels (plan/20 round 4).
    mute: Color(0xFF646B80),
    line: Color(0xFFE3E7EF),
    selFg: Color(0xFF0B7A6D),
    accent: Color(0xFF12A594),
    today: Color(0xFF0B7A6D),
    late: Color(0xFFC93540),
    p1: Color(0xFF4C8DF6),
    p2: Color(0xFFEB9B2C),
    p3: Color(0xFFE2525C),
    cardShadow: true,
  );

  static const dark = SoftColors(
    bg: Color(0xFF151821),
    side: Color(0xFF11141B),
    panel: Color(0xFF1B1F2A),
    fg: Color(0xFFE6E9F2),
    mute: Color(0xFF8F97AD),
    line: Color(0xFF262B38),
    selFg: Color(0xFF5FD4C4),
    accent: Color(0xFF3CC9B5),
    today: Color(0xFF5FD4C4),
    late: Color(0xFFFF7880),
    p1: Color(0xFF4C8DF6),
    p2: Color(0xFFEB9B2C),
    p3: Color(0xFFFF7880),
    cardShadow: false,
  );

  /// The checkbox border for a priority (0 none … 3 high).
  Color priority(int p) => switch (p) {
    3 => p3,
    2 => p2,
    1 => p1,
    _ => mute,
  };

  @override
  SoftColors copyWith() => this;

  @override
  SoftColors lerp(SoftColors? other, double t) =>
      t < 0.5 ? this : (other ?? this);
}

extension SoftContext on BuildContext {
  SoftColors get soft => Theme.of(this).extension<SoftColors>()!;
}

const _font = 'Manrope';

/// Manrope is a variable font: set the `wght` axis as well as the weight.
TextStyle manrope(double size, FontWeight weight, {Color? color}) => TextStyle(
  fontFamily: _font,
  fontSize: size,
  fontWeight: weight,
  fontVariations: [FontVariation('wght', weight.value.toDouble())],
  color: color,
);

ThemeData softTheme(Brightness brightness) {
  final c = brightness == Brightness.light ? SoftColors.light : SoftColors.dark;
  final scheme =
      ColorScheme.fromSeed(
        seedColor: c.accent,
        brightness: brightness,
      ).copyWith(
        primary: c.accent,
        surface: c.panel,
        onSurface: c.fg,
        outline: c.line,
      );
  final base = ThemeData(
    colorScheme: scheme,
    useMaterial3: true,
    fontFamily: _font,
  );
  return base.copyWith(
    scaffoldBackgroundColor: c.bg,
    extensions: [c],
    dividerColor: c.line,
    canvasColor: c.panel,
    // Material 3 tints dialogs and menus with a grey-green surface; Soft wants the panel colour.
    dialogTheme: DialogThemeData(
      backgroundColor: c.panel,
      surfaceTintColor: Colors.transparent,
      shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(18)),
      titleTextStyle: manrope(17, FontWeight.w800, color: c.fg),
    ),
    popupMenuTheme: PopupMenuThemeData(
      color: c.panel,
      surfaceTintColor: Colors.transparent,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(12),
        side: BorderSide(color: c.line),
      ),
    ),
    textTheme: base.textTheme
        .apply(bodyColor: c.fg, displayColor: c.fg)
        .copyWith(
          bodyMedium: manrope(13.5, FontWeight.w500, color: c.fg),
          bodySmall: manrope(12, FontWeight.w500, color: c.mute),
          titleLarge: manrope(19, FontWeight.w800, color: c.fg),
          titleMedium: manrope(16, FontWeight.w800, color: c.fg),
        ),
    inputDecorationTheme: InputDecorationTheme(
      filled: true,
      fillColor: c.panel,
      isDense: true,
      contentPadding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
      hintStyle: manrope(13.5, FontWeight.w500, color: c.mute),
      border: OutlineInputBorder(
        borderRadius: BorderRadius.circular(10),
        borderSide: BorderSide(color: c.line),
      ),
      enabledBorder: OutlineInputBorder(
        borderRadius: BorderRadius.circular(10),
        borderSide: BorderSide(color: c.line),
      ),
      focusedBorder: OutlineInputBorder(
        borderRadius: BorderRadius.circular(10),
        borderSide: BorderSide(color: c.accent, width: 1.5),
      ),
    ),
  );
}

/// `#rrggbb` → a colour, or [fallback] when unset or malformed.
Color hexColor(String? hex, Color fallback) {
  final h = hex?.replaceFirst('#', '');
  if (h == null || h.length != 6) return fallback;
  final v = int.tryParse(h, radix: 16);
  return v == null ? fallback : Color(0xFF000000 | v);
}

/// A Material outlined chip: a coloured dot inside a thin border tinted with the tag colour.
class TagPill extends StatelessWidget {
  const TagPill({
    super.key,
    required this.label,
    this.color,
    this.icon,
    this.onDeleted,
  });

  final String label;
  final String? color;
  final String? icon;
  final VoidCallback? onDeleted;

  @override
  Widget build(BuildContext context) {
    final s = context.soft;
    final tc = hexColor(color, s.mute);
    return Container(
      padding: EdgeInsets.fromLTRB(7, 1, onDeleted == null ? 8 : 3, 1),
      decoration: BoxDecoration(
        borderRadius: BorderRadius.circular(8),
        border: Border.all(color: Color.lerp(s.line, tc, 0.6)!),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          if (icon != null)
            Text(icon!, style: const TextStyle(fontSize: 10))
          else
            Container(
              width: 6,
              height: 6,
              decoration: BoxDecoration(color: tc, shape: BoxShape.circle),
            ),
          const SizedBox(width: 4),
          Flexible(
            child: Text(
              label,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: manrope(11, FontWeight.w500, color: s.fg),
            ),
          ),
          if (onDeleted != null)
            Semantics(
              button: true,
              label: 'Remove tag $label',
              excludeSemantics: true,
              child: InkWell(
                onTap: onDeleted,
                borderRadius: BorderRadius.circular(8),
                child: Padding(
                  // A finger needs more than 12 px to hit the x.
                  padding: isTouch(context)
                      ? const EdgeInsets.fromLTRB(6, 9, 8, 9)
                      : const EdgeInsets.only(left: 2),
                  child: Icon(Icons.close, size: 12, color: s.mute),
                ),
              ),
            ),
        ],
      ),
    );
  }
}

/// The rounded-square checkbox whose border shows the priority.
class PriorityCheck extends StatelessWidget {
  const PriorityCheck({
    super.key,
    required this.priority,
    required this.done,
    required this.onTap,
    this.size = 17,
    this.hit = 3,
  });

  final int priority;
  final bool done;
  final VoidCallback onTap;
  final double size;

  /// Space around the box that still counts as the tap (3 with a mouse; a touch screen
  /// wants a 48 px target).
  final double hit;

  @override
  Widget build(BuildContext context) {
    final s = context.soft;
    final edge = done ? s.mute : s.priority(priority);
    return Semantics(
      checked: done,
      button: true,
      label: done ? 'Mark as not done' : 'Mark as done',
      excludeSemantics: true,
      child: InkWell(
        onTap: onTap,
        borderRadius: BorderRadius.circular(6),
        child: Padding(
          padding: EdgeInsets.all(hit),
          child: AnimatedContainer(
            duration: motion(context),
            curve: Curves.easeOut,
            width: size,
            height: size,
            decoration: BoxDecoration(
              color: done
                  ? s.mute
                  : (s.cardShadow ? Colors.white : Colors.transparent),
              borderRadius: BorderRadius.circular(6),
              border: Border.all(color: edge, width: 1.6),
            ),
            // The tick pops in rather than appearing.
            child: AnimatedScale(
              duration: motion(context),
              curve: Curves.easeOutBack,
              scale: done ? 1 : 0,
              child: Icon(Icons.check, size: size - 4, color: s.panel),
            ),
          ),
        ),
      ),
    );
  }
}

/// A touch screen (phone or tablet): controls there want 48 px targets.
bool isTouch(BuildContext context) => switch (Theme.of(context).platform) {
  TargetPlatform.android || TargetPlatform.iOS => true,
  _ => false,
};

/// A short transition length, or none when the system asks for reduced motion
/// (`MediaQuery.disableAnimations`: Android's "remove animations", the OS setting on desktop).
Duration motion(BuildContext context, [int ms = 140]) =>
    MediaQuery.disableAnimationsOf(context)
    ? Duration.zero
    : Duration(milliseconds: ms);
