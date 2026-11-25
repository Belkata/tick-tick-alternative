// The filter builder's model: an editable rule tree that converts to and from the core's
// JSON (utre_core::FilterRule). Groups are `{"all": […]}` / `{"any": […]}`, conditions are
// `{"field", "op", "value"}`, and NOT wraps either: `{"not": …}`.
import 'dart:convert';

/// The fields a condition can test, with the operators each accepts (see core/src/filter.rs).
enum RuleField {
  tag('Tag', ['has', 'has_any', 'has_all', 'has_none']),
  list('List', ['is', 'is_not', 'none']),
  inbox('In Inbox', ['is']),
  due('Due date', [
    'on',
    'before',
    'on_or_before',
    'after',
    'between',
    'none',
    'overdue',
  ]),
  start('Start date', [
    'on',
    'before',
    'on_or_before',
    'after',
    'between',
    'none',
  ]),
  priority('Priority', ['is', '>=', '<=']),
  hasTime('Has a time', ['is']),
  repeating('Repeats', ['is']),
  status('Status', ['is']),
  text('Text', ['contains']);

  const RuleField(this.label, this.ops);

  final String label;
  final List<String> ops;

  /// The name in the JSON.
  String get json => switch (this) {
    RuleField.hasTime => 'has_time',
    _ => name,
  };

  static RuleField? fromJson(String s) =>
      RuleField.values.where((f) => f.json == s).firstOrNull;

  /// What a fresh condition on this field starts with.
  Object? get defaultValue => switch (this) {
    RuleField.tag || RuleField.text => '',
    RuleField.list => null,
    RuleField.inbox || RuleField.hasTime || RuleField.repeating => true,
    RuleField.due || RuleField.start => 'today',
    RuleField.priority => 2,
    RuleField.status => 'open',
  };
}

/// How an operator reads in the builder.
String opLabel(String op) => switch (op) {
  'has' => 'has',
  'has_any' => 'has any of',
  'has_all' => 'has all of',
  'has_none' => 'has none of',
  'is' => 'is',
  'is_not' => 'is not',
  'none' => 'is empty',
  'on' => 'is on',
  'before' => 'is before',
  'on_or_before' => 'is on or before',
  'after' => 'is after',
  'between' => 'is between',
  'overdue' => 'is overdue',
  '>=' => 'is at least',
  '<=' => 'is at most',
  'contains' => 'contains',
  _ => op,
};

/// Operators that take no value.
bool opNeedsValue(String op) => op != 'none' && op != 'overdue';

sealed class RuleNode {
  RuleNode({this.negate = false});

  /// Wrapped in `{"not": …}`.
  bool negate;

  Object? toJson();

  /// Wraps the inner JSON in NOT when negated.
  Object? wrap(Object? inner) => negate ? {'not': inner} : inner;

  static RuleNode fromJson(Object? json) {
    if (json is Map<String, dynamic>) {
      if (json.containsKey('not')) {
        final inner = fromJson(json['not']);
        return inner..negate = !inner.negate;
      }
      if (json['all'] is List) {
        return RuleGroup.fromList(all: true, json['all'] as List);
      }
      if (json['any'] is List) {
        return RuleGroup.fromList(all: false, json['any'] as List);
      }
      if (json['field'] is String) return RuleCond.fromJson(json);
    }
    throw FormatException('not a filter rule: ${jsonEncode(json)}');
  }
}

/// `all` (AND) or `any` (OR) of its children.
class RuleGroup extends RuleNode {
  RuleGroup({this.all = true, List<RuleNode>? children, super.negate})
    : children = children ?? [];

  factory RuleGroup.fromList(List<dynamic> items, {required bool all}) =>
      RuleGroup(
        all: all,
        children: [for (final i in items) RuleNode.fromJson(i)],
      );

  bool all;
  final List<RuleNode> children;

  @override
  Object? toJson() => wrap({
    all ? 'all' : 'any': [for (final c in children) c.toJson()],
  });
}

/// One test on one field.
class RuleCond extends RuleNode {
  RuleCond({required this.field, required this.op, this.value, super.negate});

  factory RuleCond.of(RuleField field) =>
      RuleCond(field: field, op: field.ops.first, value: field.defaultValue);

  factory RuleCond.fromJson(Map<String, dynamic> json) {
    final field = RuleField.fromJson(json['field'] as String);
    if (field == null) throw FormatException('unknown field ${json['field']}');
    final op = json['op'] as String? ?? field.ops.first;
    var value = json['value'];
    // Lists of tags edit as one comma-separated text.
    if (value is List) value = value.join(', ');
    return RuleCond(field: field, op: op, value: value);
  }

  RuleField field;
  String op;

  /// A string, bool, int or null; tag lists are kept as "a, b/c" while editing.
  Object? value;

  void setField(RuleField f) {
    field = f;
    op = f.ops.first;
    value = f.defaultValue;
  }

  void setOp(String o) {
    final wasBetween = op == 'between';
    op = o;
    if (o == 'between' && !wasBetween) value = 'today, +7d';
    if (o != 'between' && wasBetween) value = field.defaultValue;
  }

  @override
  Object? toJson() {
    final m = <String, Object?>{'field': field.json, 'op': op};
    if (opNeedsValue(op)) {
      m['value'] = switch ((field, op)) {
        (RuleField.tag, 'has') => _parts(value).firstOrNull ?? '',
        (RuleField.tag, _) => _parts(value),
        (RuleField.due || RuleField.start, 'between') => _parts(
          value,
        ).take(2).toList(),
        _ => value,
      };
    }
    return wrap(m);
  }

  static List<String> _parts(Object? v) => '${v ?? ''}'
      .split(',')
      .map((p) => p.trim())
      .where((p) => p.isNotEmpty)
      .toList();
}

/// A saved filter's editable definition (utre_core::Filter).
class FilterDef {
  FilterDef({
    this.name = '',
    this.icon,
    RuleGroup? rule,
    this.sort = const [],
    this.showCompleted = false,
  }) : rule = rule ?? RuleGroup(children: [RuleCond.of(RuleField.tag)]);

  factory FilterDef.fromJson(String source) {
    final m = jsonDecode(source) as Map<String, dynamic>;
    var rule = RuleNode.fromJson(m['rule']);
    // The builder's root is always a group.
    if (rule is! RuleGroup) rule = RuleGroup(children: [rule]);
    return FilterDef(
      name: m['name'] as String? ?? '',
      icon: m['icon'] as String?,
      rule: rule,
      sort: [for (final s in (m['sort'] as List? ?? const [])) s as String],
      showCompleted: m['show_completed'] as bool? ?? false,
    );
  }

  String name;
  String? icon;
  RuleGroup rule;
  List<String> sort;
  bool showCompleted;

  String ruleJson() => jsonEncode(rule.toJson());

  String toJson() => jsonEncode({
    'name': name.trim(),
    'icon': (icon == null || icon!.trim().isEmpty) ? null : icon!.trim(),
    'rule': rule.toJson(),
    'sort': sort,
    'show_completed': showCompleted,
  });
}
