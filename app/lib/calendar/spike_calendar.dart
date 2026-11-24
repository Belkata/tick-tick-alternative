// M0 Spike 5: a kalender-based calendar with drag, resize and drop-from-outside.
// Data here is in-memory sample data; wiring to the Rust core comes in M5.

import 'package:kalender/kalender.dart';
// Not exported publicly; needed so an outside drag looks like a kalender reschedule.
// ignore: implementation_imports
import 'package:kalender/src/models/kalender_events/draggable_event.dart'
    show Reschedule;
// kalender is built on flutter/material; material_ui is a separate copy that it can't see.
// ignore: deprecated_member_use
import 'package:flutter/material.dart';

/// One tile on the calendar: either a task (editable) or a Google event (read-only).
class CalEvent extends KalenderEvent {
  CalEvent({
    super.id,
    required super.start,
    required super.end,
    required this.title,
    required this.isTask,
    super.isAllDay,
  }) : super(
         interaction: isTask
             ? EventInteraction.allowAll()
             : EventInteraction.allowNone(),
       );

  final String title;

  /// Tasks are ours and can be dragged and resized. Google events are read-only.
  final bool isTask;

  @override
  CalEvent copyWithData({required DateTime start, required DateTime end}) =>
      CalEvent(start: start, end: end, title: title, isTask: isTask);

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      (super == other &&
          other is CalEvent &&
          other.title == title &&
          other.isTask == isTask);

  @override
  int get hashCode => Object.hash(super.hashCode, title, isTask);
}

class SpikeCalendarPage extends StatefulWidget {
  const SpikeCalendarPage({super.key});

  @override
  State<SpikeCalendarPage> createState() => _SpikeCalendarPageState();
}

class _SpikeCalendarPageState extends State<SpikeCalendarPage> {
  final _events = DefaultEventsController();

  late final _views = <ViewConfiguration>[
    MultiDayViewConfiguration.singleDay(
      name: 'Day',
      initialTimeOfDay: const KalenderTime(hour: 8, minute: 0),
    ),
    MultiDayViewConfiguration.custom(
      name: '3 days',
      numberOfDays: 3,
      initialTimeOfDay: const KalenderTime(hour: 8, minute: 0),
    ),
    MultiDayViewConfiguration.week(
      name: 'Week',
      initialTimeOfDay: const KalenderTime(hour: 8, minute: 0),
    ),
    MonthViewConfiguration.singleMonth(name: 'Month'),
  ];

  late final _calendar = KalenderController(viewConfiguration: _views[1]);

  /// Tasks with no date: the "unscheduled sidebar" (a strip on phones).
  final _unscheduled = <String>[
    'Renew passport',
    'Book flights',
    'Call the dentist',
  ];

  @override
  void initState() {
    super.initState();
    _events.addEvents(_sampleEvents());
  }

  @override
  void dispose() {
    _calendar.dispose();
    _events.dispose();
    super.dispose();
  }

  List<CalEvent> _sampleEvents() {
    final now = DateTime.now();
    final today = DateTime(now.year, now.month, now.day);
    DateTime at(int dayOffset, int h, [int m = 0]) =>
        today.add(Duration(days: dayOffset, hours: h, minutes: m));

    return [
      for (var d = -1; d <= 5; d++)
        CalEvent(
          start: at(d, 9, 30),
          end: at(d, 9, 45),
          title: 'Standup',
          isTask: false,
        ),
      CalEvent(
        start: at(0, 12, 30),
        end: at(0, 13, 30),
        title: 'Lunch with Ana',
        isTask: false,
      ),
      CalEvent(
        start: at(0, 12),
        end: at(0, 13),
        title: 'Gym',
        isTask: true,
      ), // overlaps lunch
      CalEvent(
        start: at(0, 10),
        end: at(0, 11),
        title: 'Write spike log',
        isTask: true,
      ),
      CalEvent(
        start: at(1, 14),
        end: at(1, 15, 30),
        title: 'Review PR',
        isTask: true,
      ),
      CalEvent(
        start: at(1, 0),
        end: at(2, 0),
        title: "Mum's birthday",
        isTask: false,
        isAllDay: true,
      ),
      CalEvent(
        start: at(2, 0),
        end: at(3, 0),
        title: 'Pay rent',
        isTask: true,
        isAllDay: true,
      ),
    ];
  }

  void _onChanged(KalenderEvent original, KalenderEvent updated) {
    final existing = _events.byId(original.id);
    if (existing != null) {
      _events.updateEvent(event: existing, updatedEvent: updated);
      return;
    }
    // Dropped in from the unscheduled strip: schedule it.
    final title = (updated as CalEvent).title;
    _events.addEvent(updated);
    setState(() => _unscheduled.remove(title));
    _toast('Scheduled "$title"');
  }

  void _toast(String message) {
    ScaffoldMessenger.of(context)
      ..hideCurrentSnackBar()
      ..showSnackBar(
        SnackBar(content: Text(message), duration: const Duration(seconds: 2)),
      );
  }

  @override
  Widget build(BuildContext context) {
    final tiles = TileComponents(tileBuilder: _tile);
    return Scaffold(
      appBar: AppBar(
        title: ValueListenableBuilder(
          valueListenable: _calendar.floatingVisibleRange,
          builder: (context, range, _) {
            final month = range.dominantMonthDate;
            return Text('${month.monthNameLocalized()} ${month.year}');
          },
        ),
        actions: [
          IconButton(
            tooltip: 'Today',
            onPressed: () => _calendar.animateToDate(DateTime.now()),
            icon: const Icon(Icons.today_outlined),
          ),
          IconButton(
            tooltip: 'Previous',
            onPressed: _calendar.animateToPreviousPage,
            icon: const Icon(Icons.chevron_left),
          ),
          IconButton(
            tooltip: 'Next',
            onPressed: _calendar.animateToNextPage,
            icon: const Icon(Icons.chevron_right),
          ),
          ListenableBuilder(
            listenable: _calendar,
            builder: (context, _) => DropdownButton<ViewConfiguration>(
              key: const Key('view-picker'),
              value: _calendar.viewConfiguration,
              underline: const SizedBox.shrink(),
              items: [
                for (final v in _views)
                  DropdownMenuItem(value: v, child: Text(v.name)),
              ],
              onChanged: (v) {
                _calendar.viewConfiguration = v!;
                // A new configuration keeps the old first visible day, which can land on
                // the previous week or month. Go back to today once the new view is built.
                WidgetsBinding.instance.addPostFrameCallback(
                  (_) => _calendar.jumpToDate(DateTime.now()),
                );
              },
            ),
          ),
          const SizedBox(width: 8),
        ],
      ),
      body: Column(
        children: [
          Expanded(
            child: KalenderView(
              eventsController: _events,
              kalenderController: _calendar,
              views: [
                MultiDayViewParts(
                  header: MultiDayHeader(tileComponents: tiles),
                  body: MultiDayBody(tileComponents: tiles),
                ),
                MonthViewParts(body: MonthBody(tileComponents: tiles)),
                const ScheduleViewParts(),
              ],
              callbacks: KalenderCallbacks(
                onEventCreate: (e) => CalEvent(
                  start: e.start,
                  end: e.end,
                  title: 'New task',
                  isTask: true,
                ),
                onEventCreated: (e) => _events.addEvent(e),
                onEventChanged: _onChanged,
                // Selecting is up to the app; a selected task shows its resize handles.
                onEventTapped: (e) => (e as CalEvent).isTask
                    ? _calendar.selectEvent(e)
                    : _toast(e.title),
                onTapped: (_) => _calendar.deselectEvent(),
              ),
            ),
          ),
          _UnscheduledStrip(titles: _unscheduled),
        ],
      ),
    );
  }

  Widget _tile(
    BuildContext context,
    KalenderEvent event,
    KalenderDateTimeRange range,
  ) {
    final e = event as CalEvent;
    final scheme = Theme.of(context).colorScheme;
    final bg = e.isTask
        ? scheme.primaryContainer
        : scheme.surfaceContainerHighest;
    final fg = e.isTask ? scheme.onPrimaryContainer : scheme.onSurfaceVariant;
    final tile = Container(
      margin: const EdgeInsets.all(1),
      padding: const EdgeInsets.symmetric(horizontal: 4, vertical: 2),
      decoration: BoxDecoration(
        color: bg,
        borderRadius: BorderRadius.circular(8),
        border: e.isTask
            ? null
            : Border(left: BorderSide(color: scheme.outline, width: 3)),
      ),
      child: Text(
        e.title,
        maxLines: 2,
        overflow: TextOverflow.ellipsis,
        style: TextStyle(color: fg, fontSize: 12, fontWeight: FontWeight.w600),
      ),
    );
    if (e.isTask) return tile;
    // A read-only tile has no draggable, so a long-press would fall through to the
    // grid and create a task underneath it. Swallow it here.
    return GestureDetector(
      behavior: HitTestBehavior.opaque,
      onLongPress: () {},
      child: tile,
    );
  }
}

/// Unscheduled tasks. Long-press one and drag it onto the grid to give it a time.
///
/// kalender has no public "external drop" API, but its drop targets accept a
/// [Reschedule] payload and fall back to the payload's event when it isn't in
/// the controller. The drop then arrives as `onEventChanged(original, updated)`.
class _UnscheduledStrip extends StatelessWidget {
  const _UnscheduledStrip({required this.titles});

  final List<String> titles;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return Material(
      color: scheme.surfaceContainerLow,
      child: SafeArea(
        top: false,
        child: SizedBox(
          height: 64,
          child: titles.isEmpty
              ? const Center(child: Text('Nothing unscheduled'))
              : ListView(
                  scrollDirection: Axis.horizontal,
                  padding: const EdgeInsets.symmetric(
                    horizontal: 12,
                    vertical: 12,
                  ),
                  children: [
                    for (final title in titles)
                      Padding(
                        padding: const EdgeInsets.only(right: 8),
                        child: LongPressDraggable<Reschedule>(
                          key: Key('unscheduled-$title'),
                          data: Reschedule(event: _draft(title)),
                          // kalender reads the drop time from the feedback's top-left corner,
                          // so pin that corner to the finger rather than centring the chip on it.
                          dragAnchorStrategy: pointerDragAnchorStrategy,
                          feedback: Material(
                            color: Colors.transparent,
                            child: Chip(
                              label: Text(title),
                              backgroundColor: scheme.primaryContainer,
                            ),
                          ),
                          childWhenDragging: Opacity(
                            opacity: 0.4,
                            child: Chip(label: Text(title)),
                          ),
                          child: Chip(
                            avatar: const Icon(Icons.drag_indicator, size: 18),
                            label: Text(title),
                          ),
                        ),
                      ),
                  ],
                ),
        ),
      ),
    );
  }

  /// A one-hour placeholder event; the drop target replaces its start with the drop time.
  static CalEvent _draft(String title) {
    final now = DateTime.now();
    return CalEvent(
      start: now,
      end: now.add(const Duration(hours: 1)),
      title: title,
      isTask: true,
    );
  }
}
