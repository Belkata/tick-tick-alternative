import 'package:flutter_test/flutter_test.dart';
import 'package:utre/desk/ago.dart';

void main() {
  final now = DateTime(2026, 11, 25, 12);
  String at(Duration d) =>
      agoText(now.subtract(d).millisecondsSinceEpoch, now: now);

  test('relative times read naturally', () {
    expect(at(const Duration(seconds: 20)), 'just now');
    expect(at(const Duration(minutes: 5)), '5 min ago');
    expect(at(const Duration(hours: 3)), '3 h ago');
    expect(at(const Duration(hours: 30)), '1 day ago');
    expect(at(const Duration(days: 2, hours: 1)), '2 days ago');
  });
}
