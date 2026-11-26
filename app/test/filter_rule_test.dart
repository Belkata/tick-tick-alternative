import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:utre/desk/filter_rule.dart';

void main() {
  test('the core\'s documented example round-trips', () {
    const source = '''
{"any": [
  {"field": "inbox", "op": "is", "value": true},
  {"all": [
    {"field": "due", "op": "on_or_before", "value": "today"},
    {"not": {"field": "tag", "op": "has", "value": "weekend"}}
  ]}
]}''';
    final node = RuleNode.fromJson(jsonDecode(source));
    expect(node, isA<RuleGroup>());
    expect((node as RuleGroup).all, isFalse);
    final inner = node.children[1] as RuleGroup;
    expect((inner.children[1] as RuleCond).negate, isTrue);
    expect(node.toJson(), jsonDecode(source));
  });

  test('tag lists edit as comma-separated text and save as JSON lists', () {
    final c = RuleCond.fromJson({
      'field': 'tag',
      'op': 'has_any',
      'value': ['home', 'work/urgent'],
    });
    expect(c.value, 'home, work/urgent');
    c.value = 'home,  errands , ';
    expect(c.toJson(), {
      'field': 'tag',
      'op': 'has_any',
      'value': ['home', 'errands'],
    });
    c.setOp('has');
    expect((c.toJson() as Map)['value'], 'home');
  });

  test('value-less operators drop the value; between takes two dates', () {
    final due = RuleCond.of(RuleField.due);
    due.setOp('none');
    expect(due.toJson(), {'field': 'due', 'op': 'none'});
    due.setOp('between');
    expect((due.toJson() as Map)['value'], ['today', '+7d']);
    due.setOp('on');
    expect((due.toJson() as Map)['value'], 'today');
  });

  test('negated groups wrap in not; a bare condition root becomes a group', () {
    final g = RuleGroup(
      all: false,
      negate: true,
      children: [RuleCond.of(RuleField.inbox)],
    );
    expect(g.toJson(), {
      'not': {
        'any': [
          {'field': 'inbox', 'op': 'is', 'value': true},
        ],
      },
    });
    final def = FilterDef.fromJson(
      '{"name":"Hi","rule":{"field":"priority","op":">=","value":2}}',
    );
    expect(def.rule.children.single, isA<RuleCond>());
    expect(jsonDecode(def.toJson())['rule'], {
      'all': [
        {'field': 'priority', 'op': '>=', 'value': 2},
      ],
    });
  });

  test('has_time keeps its JSON name', () {
    final c = RuleCond.of(RuleField.hasTime);
    expect((c.toJson() as Map)['field'], 'has_time');
    expect(
      RuleNode.fromJson({'field': 'has_time', 'op': 'is', 'value': false}),
      isA<RuleCond>(),
    );
  });
}
