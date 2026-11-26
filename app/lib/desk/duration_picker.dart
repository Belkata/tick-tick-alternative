// A task's length (Duration in the detail pane): a compact dialog on desktop and a
// bottom sheet on touch screens, with preset chips and a typed field.
//
// Typed text is `90`, `45m`, `1h`, `1h30`, `1:30` or `1.5h`, the same shapes the CLI
// takes (`utre_core::parse_duration`); nothing smarter than that.
// ignore: deprecated_member_use
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'theme.dart';

/// The longest length the picker accepts: a week.
const maxDurationMin = 7 * 24 * 60;

/// The preset lengths, in minutes.
const durationPresets = [15, 30, 45, 60, 90, 120, 180];

/// Reads a typed length as minutes (`90`, `45m`, `45 min`, `1h`, `1h30`, `1h30m`,
/// `1:30`, `1.5h`). Null when it isn't a length, is zero, or is over a week.
int? parseDurationText(String input) {
  final t = input.toLowerCase().replaceAll(RegExp(r'\s+'), '');
  if (t.isEmpty) return null;
  int? min;
  RegExpMatch? m;
  if ((m = RegExp(r'^(\d{1,4}):(\d{2})$').firstMatch(t)) != null) {
    final mm = int.parse(m!.group(2)!);
    if (mm >= 60) return null;
    min = int.parse(m.group(1)!) * 60 + mm;
  } else if ((m = RegExp(r'^(\d{1,3})[.,](\d{1,2})h(?:ours?)?$').firstMatch(t)) !=
      null) {
    final frac = m!.group(2)!;
    final scale = frac.length == 1 ? 10 : 100;
    min = int.parse(m.group(1)!) * 60 + (int.parse(frac) * 60 / scale).round();
  } else if ((m = RegExp(
        r'^(\d{1,4})h(?:ours?|r)?(?:(\d{1,3})(?:m|min|mins|minutes)?)?$',
      ).firstMatch(t)) !=
      null) {
    min = int.parse(m!.group(1)!) * 60 + int.parse(m.group(2) ?? '0');
  } else if ((m = RegExp(
        r'^(\d{1,6})(?:m|min|mins|minutes)?$',
      ).firstMatch(t)) !=
      null) {
    min = int.parse(m!.group(1)!);
  }
  if (min == null || min <= 0 || min > maxDurationMin) return null;
  return min;
}

/// `45 min`, `2 h`, `1 h 30 min`.
String durationLabel(int min) {
  final h = min ~/ 60, m = min % 60;
  if (h == 0) return '$m min';
  if (m == 0) return '$h h';
  return '$h h $m min';
}

/// The compact form on task cards: `45m`, `2h`, `1h30`.
String durationShort(int min) {
  final h = min ~/ 60, m = min % 60;
  if (h == 0) return '${m}m';
  if (m == 0) return '${h}h';
  return '${h}h${m.toString().padLeft(2, '0')}';
}

/// What the picker returns: a length, or "no duration".
@immutable
class PickedDuration {
  const PickedDuration(this.minutes);
  const PickedDuration.cleared() : minutes = null;

  final int? minutes;
}

/// Opens the picker: a bottom sheet on touch screens, a compact dialog on desktop.
/// Null means dismissed; [PickedDuration.cleared] means "No duration".
Future<PickedDuration?> showDurationPicker(
  BuildContext context, {
  int? initial,
}) {
  final panel = DurationPickerPanel(initial: initial);
  if (isTouch(context)) {
    return showModalBottomSheet<PickedDuration>(
      context: context,
      isScrollControlled: true,
      useSafeArea: true,
      showDragHandle: true,
      builder: (ctx) => SingleChildScrollView(
        padding: EdgeInsets.only(bottom: MediaQuery.viewInsetsOf(ctx).bottom),
        child: panel,
      ),
    );
  }
  return showDialog<PickedDuration>(
    context: context,
    builder: (_) => Dialog(
      insetPadding: const EdgeInsets.all(24),
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 344),
        child: SingleChildScrollView(child: panel),
      ),
    ),
  );
}

/// The picker's content. Pops a [PickedDuration] from the nearest route.
class DurationPickerPanel extends StatefulWidget {
  const DurationPickerPanel({super.key, this.initial});

  final int? initial;

  @override
  State<DurationPickerPanel> createState() => _DurationPickerPanelState();
}

class _DurationPickerPanelState extends State<DurationPickerPanel> {
  late final TextEditingController _text;
  final _focus = FocusNode(debugLabel: 'duration-text');

  @override
  void initState() {
    super.initState();
    final i = widget.initial;
    _text = TextEditingController(text: i == null ? '' : durationShort(i));
    _text.addListener(() => setState(() {}));
  }

  @override
  void dispose() {
    _text.dispose();
    _focus.dispose();
    super.dispose();
  }

  int? get _typed => parseDurationText(_text.text);
  bool get _bad => _text.text.trim().isNotEmpty && _typed == null;

  void _done() {
    final m = _typed;
    if (m == null) return;
    Navigator.of(context).pop(PickedDuration(m));
  }

  ButtonStyle _btn(bool touch) => ButtonStyle(
    minimumSize: WidgetStatePropertyAll(Size(64, touch ? 48 : 38)),
    shape: WidgetStatePropertyAll(
      RoundedRectangleBorder(borderRadius: BorderRadius.circular(12)),
    ),
  );

  @override
  Widget build(BuildContext context) {
    final s = context.soft;
    final touch = isTouch(context);
    final current = _typed;
    return Padding(
      padding: const EdgeInsets.fromLTRB(16, 14, 16, 14),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Expanded(
                child: Text(
                  'Duration',
                  style: manrope(16, FontWeight.w800, color: s.fg),
                ),
              ),
              TextButton(
                key: const Key('duration-clear'),
                style: _btn(touch),
                onPressed: () =>
                    Navigator.of(context).pop(const PickedDuration.cleared()),
                child: Text('No duration', style: TextStyle(color: s.mute)),
              ),
            ],
          ),
          const SizedBox(height: 4),
          Wrap(
            spacing: 6,
            runSpacing: touch ? 0 : 4,
            children: [
              for (final p in durationPresets)
                ChoiceChip(
                  key: Key('duration-preset-$p'),
                  label: Text(durationShort(p)),
                  tooltip: durationLabel(p),
                  selected: current == p,
                  showCheckmark: false,
                  materialTapTargetSize: touch
                      ? MaterialTapTargetSize.padded
                      : MaterialTapTargetSize.shrinkWrap,
                  labelStyle: manrope(
                    12.5,
                    FontWeight.w700,
                    color: current == p ? s.selFg : s.fg,
                  ),
                  selectedColor: s.accent.withValues(alpha: 0.16),
                  backgroundColor: Colors.transparent,
                  side: BorderSide(color: current == p ? s.accent : s.line),
                  shape: RoundedRectangleBorder(
                    borderRadius: BorderRadius.circular(20),
                  ),
                  onSelected: (_) =>
                      Navigator.of(context).pop(PickedDuration(p)),
                ),
            ],
          ),
          const SizedBox(height: 10),
          TextField(
            key: const Key('duration-text'),
            controller: _text,
            focusNode: _focus,
            autofocus: true,
            textInputAction: TextInputAction.done,
            keyboardType: TextInputType.text,
            inputFormatters: [
              FilteringTextInputFormatter.allow(RegExp(r'[0-9a-zA-Z:., ]')),
            ],
            onSubmitted: (_) => _done(),
            style: manrope(14, FontWeight.w600, color: s.fg),
            decoration: InputDecoration(
              isDense: true,
              hintText: '90, 1h30, 1:30, 45m',
              labelText: 'Length',
              errorText: _bad ? 'Try 90, 45m, 1h30 or 1:30 (up to a week).' : null,
              border: OutlineInputBorder(
                borderRadius: BorderRadius.circular(12),
              ),
              suffixText: current == null ? null : durationLabel(current),
            ),
          ),
          const SizedBox(height: 8),
          Wrap(
            alignment: WrapAlignment.end,
            spacing: 8,
            runSpacing: 4,
            children: [
              TextButton(
                key: const Key('duration-cancel'),
                style: _btn(touch),
                onPressed: () => Navigator.of(context).pop(),
                child: const Text('Cancel'),
              ),
              FilledButton(
                key: const Key('duration-done'),
                style: _btn(touch).copyWith(
                  backgroundColor: WidgetStateProperty.resolveWith(
                    (st) =>
                        st.contains(WidgetState.disabled) ? s.line : s.accent,
                  ),
                  foregroundColor: WidgetStateProperty.resolveWith(
                    (st) => st.contains(WidgetState.disabled)
                        ? s.mute
                        : Theme.of(context).colorScheme.onPrimary,
                  ),
                ),
                onPressed: current == null ? null : _done,
                child: const Text('Done'),
              ),
            ],
          ),
        ],
      ),
    );
  }
}
