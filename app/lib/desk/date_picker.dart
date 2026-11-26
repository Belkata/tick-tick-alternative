// A date and optional time picker for a task's Due and Start (one widget for
// both fields, a compact dialog on desktop and a bottom sheet on touch screens).
//
// It speaks the core's date text (`2026-12-01` all-day, `2026-12-01 14:30`
// timed), so nothing about the data model changes: the result goes to
// `setDue` / `setStart` like typed text does.
// ignore: deprecated_member_use
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'theme.dart';

/// A date, with a time of day unless it is an all-day date.
@immutable
class When {
  const When(this.day, [this.time]);

  /// The calendar day (only year, month and day count).
  final DateTime day;

  /// Null for an all-day date.
  final TimeOfDay? time;

  bool get allDay => time == null;

  /// The text the core parses: `2026-12-01` or `2026-12-01 14:30`.
  String get text {
    final d =
        '${day.year.toString().padLeft(4, '0')}-${_two(day.month)}-${_two(day.day)}';
    final t = time;
    return t == null ? d : '$d ${_two(t.hour)}:${_two(t.minute)}';
  }

  /// Minutes on a common axis, for ordering (an all-day date counts as 00:00).
  int get _key =>
      DateUtils.dateOnly(day).difference(DateTime(2000)).inDays * 1440 +
      (time == null ? 0 : time!.hour * 60 + time!.minute);

  @override
  bool operator ==(Object other) =>
      other is When &&
      DateUtils.isSameDay(day, other.day) &&
      time == other.time;

  @override
  int get hashCode => Object.hash(day.year, day.month, day.day, time);

  @override
  String toString() => text;
}

/// What the picker returns: a date, or "no date".
@immutable
class PickedWhen {
  const PickedWhen(this.when);
  const PickedWhen.cleared() : when = null;

  final When? when;

  /// The text for `setDue` / `setStart`; null clears the field.
  String? get text => when?.text;
}

String _two(int n) => n.toString().padLeft(2, '0');

/// A stored date as a [When]. All-day dates are floating (UTC midnight of the
/// calendar day); timed ones are shown in the local zone.
When whenFromMs(int ms, bool allDay) {
  if (allDay) {
    final d = DateTime.fromMillisecondsSinceEpoch(ms, isUtc: true);
    return When(DateTime(d.year, d.month, d.day));
  }
  final d = DateTime.fromMillisecondsSinceEpoch(ms);
  return When(
    DateTime(d.year, d.month, d.day),
    TimeOfDay(hour: d.hour, minute: d.minute),
  );
}

const _months = [
  'jan', 'feb', 'mar', 'apr', 'may', 'jun',
  'jul', 'aug', 'sep', 'oct', 'nov', 'dec', //
];
const _weekdays = ['mon', 'tue', 'wed', 'thu', 'fri', 'sat', 'sun'];

/// Reads back the core's `when_label` text (`today`, `Fri 14:30`, `Fri 3 Dec`,
/// `3 Dec 2025`): the bridge only gives the start date as a label.
/// Null when it can't be read.
When? parseWhenLabel(String? label, DateTime now) {
  if (label == null) return null;
  final parts = label.trim().toLowerCase().split(RegExp(r'\s+'));
  if (parts.isEmpty || parts.first.isEmpty) return null;
  TimeOfDay? time;
  final last = RegExp(r'^(\d{1,2}):(\d{2})$').firstMatch(parts.last);
  if (last != null) {
    time = TimeOfDay(
      hour: int.parse(last.group(1)!),
      minute: int.parse(last.group(2)!),
    );
    parts.removeLast();
  }
  if (parts.isEmpty) return null;
  final today = DateUtils.dateOnly(now);
  DateTime? day;
  if (parts.length == 1) {
    switch (parts.first) {
      case 'today':
        day = today;
      case 'tomorrow':
        day = today.add(const Duration(days: 1));
      case 'yesterday':
        day = today.subtract(const Duration(days: 1));
      default:
        final wd = _weekdays.indexOf(parts.first);
        if (wd >= 0) {
          // The label only uses a bare weekday for the next 2 to 6 days.
          for (var i = 2; i <= 6; i++) {
            final c = DateTime(today.year, today.month, today.day + i);
            if (c.weekday == wd + 1) day = c;
          }
        }
    }
  } else {
    // `Fri 3 Dec` or `3 Dec 2025`.
    final nums = [for (final p in parts) int.tryParse(p)];
    final m = parts.map(_months.indexOf).where((i) => i >= 0);
    if (m.isNotEmpty) {
      final month = m.first + 1;
      final n = nums.whereType<int>().toList();
      if (n.length == 1) {
        day = DateTime(today.year, month, n.first);
      } else if (n.length == 2) {
        day = DateTime(n[1], month, n[0]);
      }
    }
  }
  return day == null ? null : When(day, time);
}

/// The Monday after [today] (the "Next week" quick choice).
DateTime nextWeek(DateTime today) {
  final t = DateUtils.dateOnly(today);
  return DateTime(t.year, t.month, t.day + (8 - t.weekday));
}

/// The other date of the task, for checking `start <= due`.
@immutable
class WhenLimit {
  const WhenLimit(this.when, this.label);

  final When when;

  /// What to call it in a message ("due" or "start").
  final String label;
}

/// Why [chosen] can't be saved next to [other] (the core refuses a start after
/// the due date), or null. Dates of a different kind (all-day vs timed) are not
/// compared: the core drops the other date on such a switch, see [kindNote].
String? orderProblem({
  required bool isStart,
  required When chosen,
  WhenLimit? other,
}) {
  if (other == null || other.when.allDay != chosen.allDay) return null;
  final bad = isStart
      ? chosen._key > other.when._key
      : chosen._key < other.when._key;
  if (!bad) return null;
  return isStart
      ? 'Start can\'t be after the due date.'
      : 'Due can\'t be before the start date.';
}

/// A heads-up when saving [chosen] would drop the other date.
String? kindNote({required When chosen, WhenLimit? other}) {
  if (other == null || other.when.allDay == chosen.allDay) return null;
  return 'The ${other.label} date is ${other.when.allDay ? 'all-day' : 'timed'}, '
      'so it will be cleared.';
}

/// Reads a typed time: `9`, `9:30`, `09:30` or `0930`.
TimeOfDay? parseTimeText(String input) {
  final m = RegExp(r'^\s*(\d{1,2})(?::?(\d{2}))?\s*$').firstMatch(input);
  if (m == null) return null;
  final h = int.parse(m.group(1)!);
  final min = int.parse(m.group(2) ?? '0');
  if (h > 23 || min > 59) return null;
  return TimeOfDay(hour: h, minute: min);
}

/// Opens the picker: a bottom sheet on touch screens, a compact dialog on
/// desktop. Null means dismissed; [PickedWhen.cleared] means "No date".
Future<PickedWhen?> showWhenPicker(
  BuildContext context, {
  required String title,
  required bool isStart,
  When? initial,
  WhenLimit? other,
  DateTime? now,
}) {
  final panel = WhenPickerPanel(
    title: title,
    isStart: isStart,
    initial: initial,
    other: other,
    now: now ?? DateTime.now(),
  );
  if (isTouch(context)) {
    return showModalBottomSheet<PickedWhen>(
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
  return showDialog<PickedWhen>(
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

/// The picker's content. Pops a [PickedWhen] from the nearest route.
class WhenPickerPanel extends StatefulWidget {
  const WhenPickerPanel({
    super.key,
    required this.title,
    required this.isStart,
    required this.now,
    this.initial,
    this.other,
  });

  final String title;
  final bool isStart;
  final DateTime now;
  final When? initial;
  final WhenLimit? other;

  @override
  State<WhenPickerPanel> createState() => _WhenPickerPanelState();
}

class _WhenPickerPanelState extends State<WhenPickerPanel> {
  static const _defaultTime = TimeOfDay(hour: 9, minute: 0);

  late DateTime _month;
  DateTime? _day;
  bool _timed = false;
  late final TextEditingController _timeText;
  final _focus = FocusNode(debugLabel: 'when-picker');

  DateTime get _today => DateUtils.dateOnly(widget.now);

  @override
  void initState() {
    super.initState();
    final i = widget.initial;
    _day = i == null ? null : DateUtils.dateOnly(i.day);
    _timed = i?.time != null;
    _timeText = TextEditingController(text: _fmt(i?.time ?? _defaultTime));
    final base = _day ?? _today;
    _month = DateTime(base.year, base.month);
  }

  @override
  void dispose() {
    _timeText.dispose();
    _focus.dispose();
    super.dispose();
  }

  static String _fmt(TimeOfDay t) => '${_two(t.hour)}:${_two(t.minute)}';

  TimeOfDay? get _time => _timed ? parseTimeText(_timeText.text) : null;
  bool get _timeValid => !_timed || _time != null;

  When? get _chosen {
    final d = _day;
    if (d == null || !_timeValid) return null;
    return When(d, _time);
  }

  String? get _problem {
    final c = _chosen;
    return c == null
        ? null
        : orderProblem(isStart: widget.isStart, chosen: c, other: widget.other);
  }

  void _pick(DateTime day) => setState(() {
    _day = DateUtils.dateOnly(day);
    _month = DateTime(day.year, day.month);
  });

  void _shiftMonth(int by) =>
      setState(() => _month = DateTime(_month.year, _month.month + by));

  /// The same day [by] months on (the 31st clamps to the end of a shorter month).
  static DateTime _addMonths(DateTime d, int by) {
    final first = DateTime(d.year, d.month + by);
    final last = DateUtils.getDaysInMonth(first.year, first.month);
    return DateTime(first.year, first.month, d.day > last ? last : d.day);
  }

  void _done() {
    final c = _chosen;
    if (c == null || _problem != null) return;
    Navigator.of(context).pop(PickedWhen(c));
  }

  Future<void> _clockPicker() async {
    final t = await showTimePicker(
      context: context,
      initialTime: _time ?? _defaultTime,
    );
    if (t != null && mounted) setState(() => _timeText.text = _fmt(t));
  }

  KeyEventResult _onKey(FocusNode node, KeyEvent e) {
    if (e is KeyUpEvent) return KeyEventResult.ignored;
    final k = e.logicalKey;
    final base = _day ?? _today;
    final move = switch (k) {
      LogicalKeyboardKey.arrowLeft => -1,
      LogicalKeyboardKey.arrowRight => 1,
      LogicalKeyboardKey.arrowUp => -7,
      LogicalKeyboardKey.arrowDown => 7,
      _ => 0,
    };
    if (move != 0) {
      _pick(DateTime(base.year, base.month, base.day + move));
      return KeyEventResult.handled;
    }
    if (k == LogicalKeyboardKey.pageUp || k == LogicalKeyboardKey.pageDown) {
      final by = k == LogicalKeyboardKey.pageUp ? -1 : 1;
      // With a day chosen it moves too: the saved day must stay on screen.
      final d = _day;
      if (d == null) {
        _shiftMonth(by);
      } else {
        _pick(_addMonths(d, by));
      }
      return KeyEventResult.handled;
    }
    if (k == LogicalKeyboardKey.enter || k == LogicalKeyboardKey.numpadEnter) {
      if (_day == null) _pick(_today);
      _done();
      return KeyEventResult.handled;
    }
    return KeyEventResult.ignored;
  }

  @override
  Widget build(BuildContext context) {
    final s = context.soft;
    final touch = isTouch(context);
    final chosen = _chosen;
    final problem = _problem;
    final note = chosen == null
        ? null
        : kindNote(chosen: chosen, other: widget.other);
    final canSave = chosen != null && problem == null;
    return Focus(
      focusNode: _focus,
      autofocus: true,
      onKeyEvent: _onKey,
      child: Padding(
        padding: const EdgeInsets.fromLTRB(16, 14, 16, 14),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                Expanded(
                  child: Text(
                    widget.title,
                    style: manrope(16, FontWeight.w800, color: s.fg),
                  ),
                ),
                // Clearing is not a date to choose, so it isn't one of the quick
                // choices (which then fit on one line).
                TextButton(
                  key: const Key('when-clear'),
                  style: _btn(touch),
                  onPressed: () =>
                      Navigator.of(context).pop(const PickedWhen.cleared()),
                  child: Text('No date', style: TextStyle(color: s.mute)),
                ),
              ],
            ),
            const SizedBox(height: 4),
            _quick(context),
            const SizedBox(height: 8),
            _monthHeader(context),
            _grid(context, touch),
            const SizedBox(height: 6),
            _timeRow(context, touch),
            if (_timed && !_timeValid)
              _message(context, 'Type a time like 14:30.', s.late),
            if (problem != null) _message(context, problem, s.late),
            if (note != null && problem == null)
              _message(context, note, s.mute),
            const SizedBox(height: 8),
            Row(
              mainAxisAlignment: MainAxisAlignment.end,
              children: [
                TextButton(
                  key: const Key('when-cancel'),
                  style: _btn(touch),
                  onPressed: () => Navigator.of(context).pop(),
                  child: const Text('Cancel'),
                ),
                const SizedBox(width: 8),
                FilledButton(
                  key: const Key('when-done'),
                  style: _btn(touch).copyWith(
                    backgroundColor: WidgetStateProperty.resolveWith(
                      (st) =>
                          st.contains(WidgetState.disabled) ? s.line : s.accent,
                    ),
                    foregroundColor: WidgetStateProperty.resolveWith(
                      (st) => st.contains(WidgetState.disabled)
                          ? s.mute
                          : Colors.white,
                    ),
                  ),
                  onPressed: canSave ? _done : null,
                  child: const Text('Done'),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }

  ButtonStyle _btn(bool touch) => ButtonStyle(
    minimumSize: WidgetStatePropertyAll(Size(64, touch ? 48 : 38)),
    shape: WidgetStatePropertyAll(
      RoundedRectangleBorder(borderRadius: BorderRadius.circular(12)),
    ),
  );

  Widget _message(BuildContext context, String text, Color color) => Padding(
    padding: const EdgeInsets.only(top: 6),
    child: Text(text, style: manrope(12, FontWeight.w600, color: color)),
  );

  Widget _quick(BuildContext context) {
    final s = context.soft;
    final touch = isTouch(context);
    Widget chip(
      String key,
      String label,
      VoidCallback onTap, {
      bool on = false,
    }) => ChoiceChip(
      key: Key(key),
      label: Text(label),
      selected: on,
      showCheckmark: false,
      materialTapTargetSize: touch
          ? MaterialTapTargetSize.padded
          : MaterialTapTargetSize.shrinkWrap,
      labelStyle: manrope(12.5, FontWeight.w700, color: on ? s.selFg : s.fg),
      selectedColor: s.accent.withValues(alpha: 0.16),
      backgroundColor: Colors.transparent,
      side: BorderSide(color: on ? s.accent : s.line),
      shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(20)),
      onSelected: (_) => onTap(),
    );
    bool is_(DateTime d) => _day != null && DateUtils.isSameDay(_day, d);
    final tomorrow = DateTime(_today.year, _today.month, _today.day + 1);
    final nw = nextWeek(_today);
    return Wrap(
      spacing: 6,
      runSpacing: touch ? 0 : 4,
      children: [
        chip('when-today', 'Today', () => _pick(_today), on: is_(_today)),
        chip(
          'when-tomorrow',
          'Tomorrow',
          () => _pick(tomorrow),
          on: is_(tomorrow),
        ),
        chip('when-next-week', 'Next week', () => _pick(nw), on: is_(nw)),
      ],
    );
  }

  Widget _monthHeader(BuildContext context) {
    final s = context.soft;
    final touch = isTouch(context);
    final name = MaterialLocalizations.of(context).formatMonthYear(_month);
    Widget arrow(String key, IconData i, int by, String tip) => IconButton(
      key: Key(key),
      tooltip: tip,
      iconSize: 20,
      constraints: BoxConstraints.tightFor(
        width: touch ? 48 : 36,
        height: touch ? 48 : 36,
      ),
      onPressed: () => _shiftMonth(by),
      icon: Icon(i, color: s.mute),
    );
    return Row(
      children: [
        Expanded(
          child: Text(
            name,
            key: const Key('when-month'),
            style: manrope(14, FontWeight.w800, color: s.fg),
          ),
        ),
        arrow('when-prev', Icons.chevron_left_rounded, -1, 'Previous month'),
        arrow('when-next', Icons.chevron_right_rounded, 1, 'Next month'),
      ],
    );
  }

  Widget _grid(BuildContext context, bool touch) {
    final s = context.soft;
    final first = _month;
    // Monday-first; the grid starts on the Monday on or before the 1st.
    final start = DateTime(first.year, first.month, 1 - (first.weekday - 1));
    final rowH = touch ? 48.0 : 34.0;
    final heads = ['M', 'T', 'W', 'T', 'F', 'S', 'S'];
    return Column(
      children: [
        Row(
          children: [
            for (final h in heads)
              Expanded(
                child: Center(
                  child: Text(
                    h,
                    style: manrope(11.5, FontWeight.w700, color: s.mute),
                  ),
                ),
              ),
          ],
        ),
        for (var w = 0; w < 6; w++)
          Row(
            children: [
              for (var d = 0; d < 7; d++)
                Expanded(
                  child: _cell(
                    context,
                    DateTime(start.year, start.month, start.day + w * 7 + d),
                    rowH,
                  ),
                ),
            ],
          ),
      ],
    );
  }

  Widget _cell(BuildContext context, DateTime day, double h) {
    final s = context.soft;
    final inMonth = day.month == _month.month;
    final selected = _day != null && DateUtils.isSameDay(_day, day);
    final isToday = DateUtils.isSameDay(_today, day);
    final fg = selected
        ? Colors.white
        : isToday
        ? s.today
        : inMonth
        ? s.fg
        : s.mute.withValues(alpha: 0.55);
    return Semantics(
      button: true,
      selected: selected,
      label: MaterialLocalizations.of(context).formatFullDate(day),
      excludeSemantics: true,
      child: InkResponse(
        key: Key('when-day-${day.year}-${_two(day.month)}-${_two(day.day)}'),
        onTap: () => _pick(day),
        radius: h / 2,
        child: SizedBox(
          height: h,
          child: Center(
            child: Container(
              width: h > 40 ? 40 : 30,
              height: h > 40 ? 40 : 30,
              alignment: Alignment.center,
              decoration: BoxDecoration(
                shape: BoxShape.circle,
                color: selected ? s.accent : null,
                border: isToday && !selected
                    ? Border.all(color: s.accent, width: 1.5)
                    : null,
              ),
              child: Text(
                '${day.day}',
                style: manrope(
                  13,
                  selected || isToday ? FontWeight.w800 : FontWeight.w600,
                  color: fg,
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }

  Widget _timeRow(BuildContext context, bool touch) {
    final s = context.soft;
    return ConstrainedBox(
      constraints: BoxConstraints(minHeight: touch ? 48 : 40),
      child: Row(
        children: [
          Icon(Icons.schedule_rounded, size: 18, color: s.mute),
          const SizedBox(width: 8),
          Text('Time', style: manrope(13, FontWeight.w700, color: s.fg)),
          const SizedBox(width: 4),
          SizedBox(
            width: 52,
            child: FittedBox(
              alignment: Alignment.centerLeft,
              child: Switch(
                key: const Key('when-timed'),
                value: _timed,
                onChanged: (v) => setState(() => _timed = v),
              ),
            ),
          ),
          if (_timed) ...[
            SizedBox(
              width: 84,
              child: TextField(
                key: const Key('when-time'),
                controller: _timeText,
                keyboardType: TextInputType.datetime,
                textAlign: TextAlign.center,
                style: manrope(14, FontWeight.w700, color: s.fg),
                inputFormatters: [
                  FilteringTextInputFormatter.allow(RegExp(r'[0-9:]')),
                  LengthLimitingTextInputFormatter(5),
                ],
                decoration: InputDecoration(
                  isDense: true,
                  hintText: 'HH:MM',
                  errorText: null,
                  contentPadding: const EdgeInsets.symmetric(
                    vertical: 8,
                    horizontal: 6,
                  ),
                  enabledBorder: OutlineInputBorder(
                    borderRadius: BorderRadius.circular(10),
                    borderSide: BorderSide(color: _timeValid ? s.line : s.late),
                  ),
                  focusedBorder: OutlineInputBorder(
                    borderRadius: BorderRadius.circular(10),
                    borderSide: BorderSide(
                      color: _timeValid ? s.accent : s.late,
                    ),
                  ),
                ),
                onChanged: (_) => setState(() {}),
                onSubmitted: (_) {
                  if (_day == null) _pick(_today);
                  _done();
                },
              ),
            ),
            IconButton(
              key: const Key('when-clock'),
              tooltip: 'Pick a time',
              constraints: BoxConstraints.tightFor(
                width: touch ? 48 : 36,
                height: touch ? 48 : 36,
              ),
              onPressed: _clockPicker,
              icon: Icon(Icons.access_time_rounded, size: 20, color: s.mute),
            ),
          ] else
            Text(
              'All day',
              style: manrope(12.5, FontWeight.w600, color: s.mute),
            ),
        ],
      ),
    );
  }
}
