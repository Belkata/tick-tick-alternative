// M5a slice 3 (plan/18 M5 Calendar Log): your tasks on Day, Week and Month grids, in the
// Soft style of plan/ui-styles/calendar.html.
//
// The core decides where blocks go (`TaskDb.calendar`); this page only draws them and
// turns drags into `placeBlock` / `unschedule`. Every write ticks the change stream, which
// reloads the visible range, so `tt`, the phone and other windows show up live too.

import 'dart:async';

import 'package:kalender/kalender.dart';
// Not exported publicly; needed so an outside drag looks like a kalender reschedule.
// ignore: implementation_imports
import 'package:kalender/src/models/kalender_events/draggable_event.dart'
    show Reschedule;
// kalender is built on flutter/material; material_ui is a separate copy that it can't see.
// ignore: deprecated_member_use
import 'package:flutter/material.dart';

import '../desk/controller.dart';
import '../desk/detail.dart';
import '../desk/theme.dart';
import '../src/rust/api/tasks.dart';
import 'calendar_prefs.dart';

/// One block on the grid. Its id is the task plus the occurrence's start, so repeats of
/// one task stay distinct.
class TaskEvent extends KalenderEvent {
  TaskEvent(this.item)
    : super(
        id: '${item.taskId}@${item.startAt}',
        start: DateTime.fromMillisecondsSinceEpoch(item.startAt),
        end: DateTime.fromMillisecondsSinceEpoch(item.endAt),
        isAllDay: item.allDay,
        interaction: !item.movable
            ? EventInteraction.allowNone()
            // An all-day block moves between days but is always one day long.
            : item.allDay
            ? EventInteraction(allowStartResize: false, allowEndResize: false)
            : EventInteraction.allowAll(),
      );

  /// A placeholder for a task dragged in from the Unscheduled panel. The drop target
  /// replaces its start; the core keeps the task's own duration.
  TaskEvent.draft(TaskRow row, DateTime at)
    : this(
        CalendarItem(
          taskId: row.id,
          title: row.title,
          startAt: at.millisecondsSinceEpoch,
          endAt: at.add(const Duration(minutes: 30)).millisecondsSinceEpoch,
          allDay: false,
          done: false,
          movable: true,
          repeating: false,
          priority: row.priority,
          color: row.tags.where((t) => t.color != null).firstOrNull?.color,
        ),
      );

  final CalendarItem item;

  @override
  TaskEvent copyWithData({required DateTime start, required DateTime end}) =>
      TaskEvent(
        CalendarItem(
          taskId: item.taskId,
          title: item.title,
          startAt: start.millisecondsSinceEpoch,
          endAt: end.millisecondsSinceEpoch,
          allDay: item.allDay,
          done: item.done,
          movable: item.movable,
          repeating: item.repeating,
          priority: item.priority,
          color: item.color,
          icon: item.icon,
        ),
      );

  @override
  bool operator ==(Object other) =>
      other is TaskEvent && super == other && other.item == item;

  @override
  int get hashCode => Object.hash(super.hashCode, item);
}

const _views = ['Day', 'Week', 'Month'];

/// Month cells list a day's items in time order, all-day ones first (kalender's default
/// puts the longest first).
class _ByStart extends MultiDayLayoutStrategy {
  const _ByStart();

  @override
  MultiDayLayoutFrame generateFrame({
    required visibleRange,
    required events,
    required textDirection,
    required location,
    required cache,
  }) => defaultMultiDayFrameGenerator(
    visibleRange: visibleRange,
    events: events,
    textDirection: textDirection,
    location: location,
    cache: cache,
    eventComparator: (a, b) {
      if (a.isAllDay != b.isAllDay) return a.isAllDay ? -1 : 1;
      final c = a.start.compareTo(b.start);
      return c != 0 ? c : b.duration.compareTo(a.duration);
    },
  );

  @override
  bool operator ==(Object other) => other is _ByStart;

  @override
  int get hashCode => (_ByStart).hashCode;
}

class CalendarPage extends StatefulWidget {
  const CalendarPage({super.key, required this.c});

  /// The shell's controller: the database, the sidebar's filters and the selected task.
  final DeskController c;

  @override
  State<CalendarPage> createState() => _CalendarPageState();
}

class _CalendarPageState extends State<CalendarPage> {
  final _events = DefaultEventsController();
  late final Map<String, ViewConfiguration> _configs = {
    'Day': MultiDayViewConfiguration.singleDay(
      name: 'Day',
      initialTimeOfDay: const KalenderTime(hour: 7, minute: 30),
      initialHeightPerMinute: 0.85,
    ),
    'Week': MultiDayViewConfiguration.week(
      name: 'Week',
      initialTimeOfDay: const KalenderTime(hour: 7, minute: 30),
      initialHeightPerMinute: 0.8,
    ),
    'Month': MonthViewConfiguration.singleMonth(name: 'Month'),
  };
  late final KalenderController _calendar;
  late final StreamSubscription<int> _changes;
  final _titleFocus = FocusNode(debugLabel: 'calendar-detail-title');

  CalendarPrefs _prefs = CalendarPrefs();
  List<TaskRow> _unscheduled = const [];
  bool _dropHover = false;

  /// The block last tapped, so one occurrence of a repeat is outlined rather than all.
  String? _selectedBlock;
  int _generation = 0;

  DeskController get c => widget.c;
  TaskDb get db => widget.c.db;

  @override
  void initState() {
    super.initState();
    _calendar = KalenderController(viewConfiguration: _configs['Week']!);
    _calendar.visibleDateTimeRange.addListener(_reload);
    _changes = db.changes().listen((_) => _reload());
    c.addListener(_onShell);
    CalendarPrefs.load().then((p) {
      if (!mounted) return;
      _prefs = p;
      final narrow = MediaQuery.sizeOf(context).width < 760;
      _setView(p.view ?? (narrow ? 'Day' : 'Week'), save: false);
      _reload();
    });
  }

  @override
  void dispose() {
    _calendar.visibleDateTimeRange.removeListener(_reload);
    _changes.cancel();
    c.removeListener(_onShell);
    _calendar.dispose();
    _events.dispose();
    _titleFocus.dispose();
    super.dispose();
  }

  /// The shell repaints on its own changes (selection, sidebar); follow it.
  void _onShell() {
    if (mounted) setState(() {});
  }

  /// Fetch the visible range (padded by a week either side, so paging is instant) and
  /// the Unscheduled panel. Older answers are dropped.
  Future<void> _reload() async {
    final gen = ++_generation;
    final range = _calendar.visibleDateTimeRange.value;
    final from = range.start.subtract(const Duration(days: 7));
    final until = range.end.add(const Duration(days: 7));
    try {
      final items = await db.calendar(
        fromMs: from.millisecondsSinceEpoch,
        untilMs: until.millisecondsSinceEpoch,
      );
      final filters = c.sidebar?.filters ?? const <NavItem>[];
      var source = _prefs.unscheduledFilter;
      if (source != null &&
          c.sidebar != null &&
          !filters.any((f) => f.id == source)) {
        source = null; // the filter was deleted
      }
      final rows = await db.unscheduled(filterId: source);
      if (gen != _generation || !mounted) return;
      _events.replaceEvents([for (final i in items) TaskEvent(i)]);
      setState(() => _unscheduled = rows);
    } catch (e) {
      if (gen != _generation || !mounted) return;
      _toast('$e');
    }
  }

  void _setView(String name, {bool save = true}) {
    final config = _configs[name] ?? _configs['Week']!;
    if (_calendar.viewConfiguration.name != config.name) {
      _calendar.viewConfiguration = config;
      // A new configuration keeps the old first visible day; come back to today.
      WidgetsBinding.instance.addPostFrameCallback(
        (_) => _calendar.jumpToDate(DateTime.now()),
      );
    }
    if (save) {
      _prefs.view = name;
      _prefs.save();
    }
    setState(() {});
  }

  void _toast(String message) {
    ScaffoldMessenger.of(context)
      ..hideCurrentSnackBar()
      ..showSnackBar(
        SnackBar(
          content: Text(
            message
                .replaceFirst(RegExp(r'^AnyhowException\(?'), '')
                .replaceFirst(RegExp(r'\)$'), ''),
          ),
        ),
      );
  }

  // ----- Edits -----

  /// A move, a resize, or a drop from the Unscheduled panel (an event kalender doesn't
  /// hold yet). The grid shows the new place at once; the reload confirms it.
  Future<void> _changed(KalenderEvent original, KalenderEvent updated) async {
    if (updated is! TaskEvent) return;
    final existing = _events.byId(original.id);
    final fromPanel = existing == null;
    if (existing != null) {
      _events.updateEvent(event: existing, updatedEvent: updated);
    }
    final item = updated.item;
    await c.run(
      () => db.placeBlock(
        id: item.taskId,
        startMs: item.startAt,
        // From the panel the task keeps its own duration (else 30 min).
        endMs: fromPanel || item.allDay ? null : item.endAt,
        allDay: item.allDay,
      ),
    );
    if (fromPanel) {
      _selectedBlock = null;
      c.select(item.taskId);
    }
    await _reload();
  }

  Future<void> _unschedule(TaskEvent e) async {
    if (e.item.repeating) {
      _toast(
        'Stop the repeat before taking “${e.item.title}” off the calendar.',
      );
      return;
    }
    await c.run(() => db.unschedule(id: e.item.taskId));
    await _reload();
  }

  void _tapped(KalenderEvent e) {
    if (e is! TaskEvent) return;
    _selectedBlock = e.id;
    if (e.item.movable) {
      _calendar.selectEvent(e);
    } else {
      _calendar.deselectEvent();
    }
    c.select(e.item.taskId);
  }

  void _clearSelection() {
    _selectedBlock = null;
    _calendar.deselectEvent();
    c.select(null);
  }

  // ----- Build -----

  @override
  Widget build(BuildContext context) {
    final s = context.soft;
    return LayoutBuilder(
      builder: (context, box) {
        final wide = box.maxWidth >= 760;
        final side = c.selectedId != null && c.detail != null
            ? _detail(s)
            : _panel(s);
        return ColoredBox(
          color: s.bg,
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    _bar(s, wide),
                    Expanded(child: _kalender(s)),
                  ],
                ),
              ),
              if (wide) ...[
                VerticalDivider(width: 1, color: s.line),
                SizedBox(width: box.maxWidth >= 1200 ? 280 : 250, child: side),
              ],
            ],
          ),
        );
      },
    );
  }

  Widget _bar(SoftColors s, bool wide) {
    final current = _calendar.viewConfiguration.name;
    return Padding(
      padding: EdgeInsets.fromLTRB(wide ? 18 : 12, 12, 12, 10),
      child: Row(
        children: [
          ValueListenableBuilder(
            valueListenable: _calendar.visibleDateTimeRange,
            builder: (context, range, _) {
              final mid = range.start.add(
                range.end.difference(range.start) ~/ 2,
              );
              return Text.rich(
                TextSpan(
                  children: [
                    TextSpan(text: '${_monthName(mid.month)} '),
                    TextSpan(
                      text: '${mid.year}',
                      style: manrope(19, FontWeight.w600, color: s.mute),
                    ),
                  ],
                ),
                style: manrope(19, FontWeight.w800, color: s.fg),
              );
            },
          ),
          const Spacer(),
          _iconButton(
            s,
            Icons.chevron_left,
            'Previous',
            _calendar.animateToPreviousPage,
          ),
          const SizedBox(width: 6),
          OutlinedButton(
            key: const Key('cal-today'),
            style: OutlinedButton.styleFrom(
              side: BorderSide(color: s.line),
              backgroundColor: s.panel,
              foregroundColor: s.fg,
              shape: RoundedRectangleBorder(
                borderRadius: BorderRadius.circular(10),
              ),
              padding: const EdgeInsets.symmetric(horizontal: 14),
              minimumSize: const Size(0, 32),
              textStyle: manrope(13, FontWeight.w700),
            ),
            onPressed: () => _calendar.animateToDate(DateTime.now()),
            child: const Text('Today'),
          ),
          const SizedBox(width: 6),
          _iconButton(
            s,
            Icons.chevron_right,
            'Next',
            _calendar.animateToNextPage,
          ),
          const SizedBox(width: 12),
          _Segmented(values: _views, selected: current, onSelected: _setView),
        ],
      ),
    );
  }

  Widget _iconButton(
    SoftColors s,
    IconData icon,
    String tip,
    VoidCallback onTap,
  ) {
    return Tooltip(
      message: tip,
      child: InkWell(
        borderRadius: BorderRadius.circular(10),
        onTap: onTap,
        child: Container(
          width: 32,
          height: 32,
          decoration: BoxDecoration(
            color: s.panel,
            border: Border.all(color: s.line),
            borderRadius: BorderRadius.circular(10),
          ),
          child: Icon(icon, size: 18, color: s.fg),
        ),
      ),
    );
  }

  Widget _kalender(SoftColors s) {
    final tiles = TileComponents(
      tileBuilder: _tile,
      feedbackTileBuilder: (context, event, size) => SizedBox(
        width: size.width,
        height: size.height,
        child: Opacity(
          opacity: 0.85,
          child: _tile(context, event, event.dateTimeRange),
        ),
      ),
      tileWhenDraggingBuilder: (context, event) => Opacity(
        opacity: 0.35,
        child: _tile(context, event, event.dateTimeRange),
      ),
      dropTargetTile: (context, event) => _dropTarget(s, event),
      dragAnchorStrategy: pointerDragAnchorStrategy,
      verticalResizeHandle: Center(
        child: Container(
          width: 18,
          height: 3,
          decoration: BoxDecoration(
            color: s.fg.withValues(alpha: 0.35),
            borderRadius: BorderRadius.circular(2),
          ),
        ),
      ),
    );
    final hourLabel = manrope(10.5, FontWeight.w500, color: s.mute);
    return KalenderTheme(
      data: KalenderThemeData(
        hourLinesStyle: HourLinesStyle(
          color: s.line.withValues(alpha: 0.7),
          thickness: 1,
        ),
        daySeparatorStyle: DaySeparatorStyle(
          color: s.line.withValues(alpha: 0.7),
          width: 1,
        ),
        timelineStyle: TimelineStyle(textStyle: hourLabel, width: 52),
        timeIndicatorStyle: TimeIndicatorStyle(
          lineColor: s.late,
          circleColor: s.late,
          thickness: 2,
          circleSize: const Size(10, 10),
        ),
        weekDayHeaderStyle: WeekDayHeaderStyle(
          textStyle: manrope(11.5, FontWeight.w600, color: s.mute),
        ),
        monthGridStyle: MonthGridStyle(
          color: s.line.withValues(alpha: 0.7),
          thickness: 1,
        ),
        dayNumberStyle: DayNumberStyle(
          todayBackgroundColor: s.accent,
          todayForegroundColor: Theme.of(context).colorScheme.onPrimary,
        ),
      ),
      child: KalenderView(
        eventsController: _events,
        kalenderController: _calendar,
        interaction: KalenderInteraction(allowEventCreation: false),
        components: KalenderComponents(
          multiDayComponents: MultiDayComponents(
            headerComponents: MultiDayHeaderComponents(
              dayHeaderBuilder: _dayHeader,
              weekNumberBuilder: (_, _) => const SizedBox.shrink(),
            ),
            bodyComponents: MultiDayBodyComponents(
              // Label the hours only; kalender also asks for the half hours.
              timelineStringBuilder: (context, t) => t.minute == 0
                  ? '${t.hour.toString().padLeft(2, '0')}:00'
                  : '',
            ),
          ),
        ),
        views: [
          MultiDayViewParts(
            header: MultiDayHeader(
              tileComponents: tiles,
              configuration: const MultiDayHeaderConfiguration(
                tileHeight: 24,
                eventPadding: EdgeInsets.fromLTRB(2, 0, 3, 3),
              ),
            ),
            body: MultiDayBody(
              tileComponents: tiles,
              snapping: KalenderSnapping(
                snapIntervalMinutes: 15,
                snapToTimeIndicator: false,
              ),
              configuration: const MultiDayBodyConfiguration(
                eventLayoutStrategy: EventLayoutStrategy.sideBySide(),
                horizontalPadding: EdgeInsets.fromLTRB(2, 0, 3, 0),
                minimumTileHeight: 20,
              ),
            ),
          ),
          MonthViewParts(
            body: MonthBody(
              tileComponents: tiles,
              configuration: const MonthBodyConfiguration(
                multiDayLayoutStrategy: _ByStart(),
                tileHeight: 20,
                eventPadding: EdgeInsets.fromLTRB(4, 0, 4, 2),
              ),
            ),
          ),
        ],
        callbacks: KalenderCallbacks(
          onEventChanged: _changed,
          onEventTapped: _tapped,
          onTapped: (_) => _clearSelection(),
        ),
      ),
    );
  }

  Widget _dayHeader(BuildContext context, DateTime date) {
    final s = context.soft;
    final now = DateTime.now();
    final today =
        date.year == now.year && date.month == now.month && date.day == now.day;
    const names = ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun'];
    return LayoutBuilder(
      // Day view puts the header in the narrow timeline gutter: stack it there.
      builder: (context, box) => Padding(
        padding: box.maxWidth < 90
            ? const EdgeInsets.fromLTRB(2, 2, 2, 6)
            : const EdgeInsets.fromLTRB(8, 4, 4, 8),
        child: Flex(
          direction: box.maxWidth < 90 ? Axis.vertical : Axis.horizontal,
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.center,
          children: [
            Text(
              names[date.weekday - 1],
              style: manrope(
                12.5,
                FontWeight.w600,
                color: today ? s.today : s.mute,
              ),
            ),
            const SizedBox(width: 6, height: 2),
            Container(
              padding: today
                  ? const EdgeInsets.symmetric(horizontal: 7, vertical: 1)
                  : null,
              decoration: today
                  ? BoxDecoration(
                      color: s.accent,
                      borderRadius: BorderRadius.circular(9),
                    )
                  : null,
              child: Text(
                '${date.day}',
                style: manrope(
                  today ? 15 : 17,
                  FontWeight.w800,
                  color: today ? Theme.of(context).colorScheme.onPrimary : s.fg,
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }

  /// A block: a tint of its colour with a coloured edge, the title and the time.
  /// Completed ones fade; later repeats show ↻; the selected one gets a teal outline.
  Widget _tile(
    BuildContext context,
    KalenderEvent event,
    KalenderDateTimeRange range,
  ) {
    final s = context.soft;
    final e = event as TaskEvent;
    final i = e.item;
    final dark = Theme.of(context).brightness == Brightness.dark;
    final tc = hexColor(i.color, s.accent);
    final bg = Color.lerp(s.panel, tc, dark ? 0.30 : 0.22)!;
    // The tapped block, or else the movable occurrence of the selected task.
    final selected =
        c.selectedId == i.taskId &&
        (_selectedBlock != null && _events.byId(_selectedBlock!) != null
            ? _selectedBlock == e.id
            : i.movable);
    final start = DateTime.fromMillisecondsSinceEpoch(i.startAt);
    final end = DateTime.fromMillisecondsSinceEpoch(i.endAt);
    final time = i.allDay ? '' : '${_hm(start)}–${_hm(end)}';
    Widget title(int lines) => Text(
      i.icon != null ? '${i.icon} ${i.title}' : i.title,
      maxLines: lines,
      overflow: TextOverflow.ellipsis,
      style: manrope(12, FontWeight.w700, color: s.fg),
    );
    final rep = i.repeating && !i.movable && !i.done
        ? Icon(Icons.repeat, size: 11, color: s.mute)
        : null;

    final tile = LayoutBuilder(
      builder: (context, box) {
        final short = box.maxHeight < 34;
        final monthChip =
            !i.allDay &&
            box.maxHeight <= 22 &&
            end.difference(start).inHours < 20;
        Widget content;
        if (monthChip &&
            _calendar.viewConfiguration is MonthViewConfiguration) {
          // Month: a dot, the time and the title on one line.
          return Opacity(
            opacity: i.done ? 0.42 : 1,
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 2),
              child: Row(
                children: [
                  Container(
                    width: 7,
                    height: 7,
                    decoration: BoxDecoration(
                      color: tc,
                      shape: BoxShape.circle,
                    ),
                  ),
                  const SizedBox(width: 5),
                  Text(
                    _hm(start),
                    style: manrope(10.5, FontWeight.w500, color: s.mute),
                  ),
                  const SizedBox(width: 5),
                  Expanded(
                    child: Text(
                      i.title,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: manrope(11.5, FontWeight.w600, color: s.fg),
                    ),
                  ),
                  ?rep,
                ],
              ),
            ),
          );
        }
        // Narrow blocks (side by side) keep the title and drop the time.
        final roomForTime = box.maxWidth >= 90;
        if (short || i.allDay) {
          content = Row(
            children: [
              Flexible(
                child: Text(
                  i.title,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: manrope(
                    i.allDay ? 11.5 : 12,
                    FontWeight.w700,
                    color: s.fg,
                  ),
                ),
              ),
              if (!i.allDay && roomForTime) ...[
                const SizedBox(width: 6),
                Text(
                  _hm(start),
                  style: manrope(11, FontWeight.w500, color: s.mute),
                ),
              ],
              if (rep != null) ...[const Spacer(), rep],
            ],
          );
        } else {
          // As many title lines as fit above the time (16 px each).
          final lines = ((box.maxHeight - 22) / 16).floor().clamp(1, 4);
          content = ClipRect(
            child: OverflowBox(
              alignment: Alignment.topLeft,
              maxHeight: double.infinity,
              child: Column(
                mainAxisSize: MainAxisSize.min,
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Row(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Expanded(child: title(lines)),
                      ?rep,
                    ],
                  ),
                  const SizedBox(height: 1),
                  Text(
                    time,
                    maxLines: 1,
                    overflow: TextOverflow.clip,
                    style: manrope(11, FontWeight.w500, color: s.mute),
                  ),
                ],
              ),
            ),
          );
        }
        return Opacity(
          opacity: i.done ? 0.42 : 1,
          child: Container(
            margin: const EdgeInsets.all(0.5),
            decoration: BoxDecoration(
              color: bg,
              borderRadius: BorderRadius.circular(i.allDay ? 8 : 10),
              border: selected ? Border.all(color: s.accent, width: 2) : null,
            ),
            clipBehavior: Clip.antiAlias,
            child: Row(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Container(width: 3, color: tc),
                Expanded(
                  child: Padding(
                    padding: EdgeInsets.fromLTRB(
                      6,
                      short || i.allDay ? 0 : 4,
                      6,
                      0,
                    ),
                    child: short || i.allDay
                        ? Align(alignment: Alignment.centerLeft, child: content)
                        : content,
                  ),
                ),
              ],
            ),
          ),
        );
      },
    );
    return Tooltip(
      message: i.allDay ? i.title : '${i.title}\n$time',
      waitDuration: const Duration(milliseconds: 700),
      child: tile,
    );
  }

  /// Where a dragged block will land. kalender also draws this over the selected block,
  /// so it carries no text: the timeline marks the start and end times.
  Widget _dropTarget(SoftColors s, KalenderEvent event) {
    return Container(
      decoration: BoxDecoration(
        color: s.accent.withValues(alpha: 0.10),
        border: Border.all(color: s.accent, width: 2),
        borderRadius: BorderRadius.circular(10),
      ),
    );
  }

  // ----- Right side: Unscheduled panel or the selected task -----

  Widget _detail(SoftColors s) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Container(
          color: s.panel,
          padding: const EdgeInsets.fromLTRB(8, 6, 8, 0),
          alignment: Alignment.centerLeft,
          child: TextButton.icon(
            key: const Key('cal-back-to-unscheduled'),
            onPressed: _clearSelection,
            icon: Icon(Icons.arrow_back, size: 16, color: s.mute),
            label: Text(
              'Unscheduled',
              style: manrope(12.5, FontWeight.w600, color: s.mute),
            ),
          ),
        ),
        Expanded(
          child: DetailPane(c: c, titleFocus: _titleFocus),
        ),
      ],
    );
  }

  Widget _panel(SoftColors s) {
    final filters = c.sidebar?.filters ?? const <NavItem>[];
    final source = filters.any((f) => f.id == _prefs.unscheduledFilter)
        ? _prefs.unscheduledFilter
        : null;
    return DragTarget<Reschedule>(
      onWillAcceptWithDetails: (d) {
        final e = d.data.event;
        final ok =
            e is TaskEvent && e.item.movable && _events.byId(e.id) != null;
        if (ok != _dropHover) setState(() => _dropHover = ok);
        return ok;
      },
      onLeave: (_) => setState(() => _dropHover = false),
      onAcceptWithDetails: (d) {
        setState(() => _dropHover = false);
        _unschedule(d.data.event as TaskEvent);
      },
      builder: (context, _, _) => AnimatedContainer(
        duration: const Duration(milliseconds: 120),
        decoration: BoxDecoration(
          color: _dropHover ? s.accent.withValues(alpha: 0.08) : s.bg,
          border: _dropHover ? Border.all(color: s.accent, width: 2) : null,
        ),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Padding(
              padding: const EdgeInsets.fromLTRB(14, 16, 14, 10),
              child: Row(
                children: [
                  Text(
                    'Unscheduled',
                    style: manrope(15, FontWeight.w800, color: s.fg),
                  ),
                  const SizedBox(width: 8),
                  Text(
                    '${_unscheduled.length}',
                    style: manrope(13, FontWeight.w600, color: s.mute),
                  ),
                ],
              ),
            ),
            Padding(
              padding: const EdgeInsets.fromLTRB(14, 0, 14, 10),
              child: Container(
                padding: const EdgeInsets.symmetric(horizontal: 10),
                decoration: BoxDecoration(
                  color: s.panel,
                  border: Border.all(color: s.line),
                  borderRadius: BorderRadius.circular(10),
                ),
                child: DropdownButtonHideUnderline(
                  child: DropdownButton<String?>(
                    key: const Key('cal-unscheduled-source'),
                    value: source,
                    isExpanded: true,
                    isDense: true,
                    padding: const EdgeInsets.symmetric(vertical: 8),
                    borderRadius: BorderRadius.circular(10),
                    dropdownColor: s.panel,
                    style: manrope(13, FontWeight.w600, color: s.fg),
                    icon: Icon(Icons.expand_more, size: 18, color: s.mute),
                    selectedItemBuilder: (context) => [
                      for (final label in [
                        'No date',
                        ...filters.map((f) => f.name),
                      ])
                        Row(
                          children: [
                            Text(
                              'From ',
                              style: manrope(
                                12,
                                FontWeight.w600,
                                color: s.mute,
                              ),
                            ),
                            Flexible(
                              child: Text(
                                label,
                                overflow: TextOverflow.ellipsis,
                              ),
                            ),
                          ],
                        ),
                    ],
                    items: [
                      const DropdownMenuItem(
                        value: null,
                        child: Text('No date'),
                      ),
                      for (final f in filters)
                        DropdownMenuItem(value: f.id, child: Text(f.name)),
                    ],
                    onChanged: (v) {
                      setState(() => _prefs.unscheduledFilter = v);
                      _prefs.save();
                      _reload();
                    },
                  ),
                ),
              ),
            ),
            Expanded(
              child: _unscheduled.isEmpty
                  ? Center(
                      child: Text(
                        'Nothing here',
                        style: manrope(12.5, FontWeight.w500, color: s.mute),
                      ),
                    )
                  : ListView.separated(
                      padding: const EdgeInsets.fromLTRB(12, 0, 12, 12),
                      itemCount: _unscheduled.length,
                      separatorBuilder: (_, _) => const SizedBox(height: 6),
                      itemBuilder: (context, i) => _card(s, _unscheduled[i]),
                    ),
            ),
            Container(
              margin: const EdgeInsets.fromLTRB(14, 0, 14, 14),
              padding: const EdgeInsets.only(top: 10),
              decoration: BoxDecoration(
                border: Border(top: BorderSide(color: s.line)),
              ),
              child: Text(
                'Drag a task onto the grid to time-block it. Drop a block back here to unschedule it.',
                style: manrope(11.5, FontWeight.w500, color: s.mute),
              ),
            ),
          ],
        ),
      ),
    );
  }

  Widget _card(SoftColors s, TaskRow row, {bool lifted = false}) {
    final card = Container(
      padding: const EdgeInsets.fromLTRB(10, 9, 10, 9),
      decoration: BoxDecoration(
        color: s.panel,
        borderRadius: BorderRadius.circular(12),
        border: lifted ? Border.all(color: s.accent) : null,
        boxShadow: lifted
            ? const [
                BoxShadow(
                  color: Color(0x33141E3C),
                  blurRadius: 24,
                  offset: Offset(0, 8),
                ),
              ]
            : s.cardShadow
            ? const [
                BoxShadow(
                  color: Color(0x0F141E3C),
                  blurRadius: 2,
                  offset: Offset(0, 1),
                ),
              ]
            : null,
      ),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Container(
            width: 16,
            height: 16,
            margin: const EdgeInsets.only(top: 1),
            decoration: BoxDecoration(
              borderRadius: BorderRadius.circular(6),
              border: Border.all(color: s.priority(row.priority), width: 1.6),
            ),
          ),
          const SizedBox(width: 9),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  row.title,
                  style: manrope(13, FontWeight.w600, color: s.fg),
                ),
                if (row.tags.isNotEmpty) ...[
                  const SizedBox(height: 5),
                  Wrap(
                    spacing: 4,
                    runSpacing: 4,
                    children: [
                      for (final t in row.tags)
                        TagPill(label: t.name, color: t.color, icon: t.icon),
                    ],
                  ),
                ],
              ],
            ),
          ),
        ],
      ),
    );
    if (lifted) return card;
    final feedback = Material(
      color: Colors.transparent,
      child: SizedBox(
        width: 220,
        child: Transform.rotate(
          angle: -0.025,
          child: _card(s, row, lifted: true),
        ),
      ),
    );
    final placeholder = Container(
      height: 40,
      decoration: BoxDecoration(
        borderRadius: BorderRadius.circular(12),
        border: Border.all(color: s.line, width: 1.5),
      ),
    );
    final draggable = Theme.of(context).platform == TargetPlatform.android
        ? LongPressDraggable<Reschedule>(
            data: Reschedule(event: TaskEvent.draft(row, DateTime.now())),
            dragAnchorStrategy: pointerDragAnchorStrategy,
            feedback: feedback,
            childWhenDragging: placeholder,
            child: card,
          )
        : Draggable<Reschedule>(
            data: Reschedule(event: TaskEvent.draft(row, DateTime.now())),
            // kalender reads the drop time from the feedback's top-left corner, so pin
            // that corner to the pointer.
            dragAnchorStrategy: pointerDragAnchorStrategy,
            feedback: feedback,
            childWhenDragging: placeholder,
            child: card,
          );
    return GestureDetector(
      key: Key('unscheduled-${row.id}'),
      onTap: () => c.select(row.id),
      child: draggable,
    );
  }
}

/// Day / Week / Month, as in the mockup.
class _Segmented extends StatelessWidget {
  const _Segmented({
    required this.values,
    required this.selected,
    required this.onSelected,
  });

  final List<String> values;
  final String selected;
  final ValueChanged<String> onSelected;

  @override
  Widget build(BuildContext context) {
    final s = context.soft;
    return Container(
      key: const Key('view-picker'),
      padding: const EdgeInsets.all(3),
      decoration: BoxDecoration(
        color: s.side,
        borderRadius: BorderRadius.circular(10),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          for (final v in values)
            GestureDetector(
              key: Key('view-$v'),
              onTap: () => onSelected(v),
              child: Container(
                padding: const EdgeInsets.symmetric(
                  horizontal: 12,
                  vertical: 5,
                ),
                decoration: v == selected
                    ? BoxDecoration(
                        color: s.panel,
                        borderRadius: BorderRadius.circular(8),
                        boxShadow: s.cardShadow
                            ? const [
                                BoxShadow(
                                  color: Color(0x14141E3C),
                                  blurRadius: 3,
                                  offset: Offset(0, 1),
                                ),
                              ]
                            : null,
                      )
                    : null,
                child: Text(
                  v,
                  style: manrope(
                    12.5,
                    v == selected ? FontWeight.w700 : FontWeight.w600,
                    color: v == selected ? s.fg : s.mute,
                  ),
                ),
              ),
            ),
        ],
      ),
    );
  }
}

String _hm(DateTime t) =>
    '${t.hour.toString().padLeft(2, '0')}:${t.minute.toString().padLeft(2, '0')}';

String _monthName(int m) => const [
  'January',
  'February',
  'March',
  'April',
  'May',
  'June',
  'July',
  'August',
  'September',
  'October',
  'November',
  'December',
][m - 1];
