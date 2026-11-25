import 'package:flutter_test/flutter_test.dart';
import 'package:tick_alt/desk/markdown.dart';

void main() {
  test('parses headings, lists, tasks, code and quotes', () {
    final b = parseMarkdown(
      '# Title\n\nhello\nworld\n\n- a\n  - [ ] b\n1. c\n> q\n> r\n---\n```\nx\n```',
    );
    expect(b[0], isA<MdHeading>());
    expect((b[1] as MdPara).text, 'hello\nworld');
    expect((b[2] as MdItem).marker, '•');
    final task = b[3] as MdItem;
    expect(task.indent, 1);
    expect(task.checked, false);
    expect((b[4] as MdItem).marker, '1.');
    expect((b[5] as MdQuote).text, 'q\nr');
    expect(b[6], isA<MdRule>());
    expect((b[7] as MdCode).text, 'x');
  });

  test('toggles a task checkbox on its line only', () {
    const src = '- [ ] one\n- [x] two';
    expect(toggleCheckbox(src, 0), '- [x] one\n- [x] two');
    expect(toggleCheckbox(src, 1), '- [ ] one\n- [ ] two');
  });
}
