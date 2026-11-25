// A small Markdown renderer for task notes. Notes stay plain text in the core
// (plan/04); this only changes how they are shown. Supported: headings, bold,
// italic, strikethrough, inline code, fenced code, links, quotes, bullet,
// numbered and task lists (checkboxes toggle the source), and rules.
import 'dart:io';

import 'package:flutter/gestures.dart';
// ignore: deprecated_member_use
import 'package:flutter/material.dart';

import 'theme.dart';

// ---- blocks ----------------------------------------------------------------

sealed class MdBlock {
  const MdBlock();
}

class MdPara extends MdBlock {
  const MdPara(this.text);
  final String text;
}

class MdHeading extends MdBlock {
  const MdHeading(this.level, this.text);
  final int level;
  final String text;
}

class MdCode extends MdBlock {
  const MdCode(this.text);
  final String text;
}

class MdQuote extends MdBlock {
  const MdQuote(this.text);
  final String text;
}

class MdRule extends MdBlock {
  const MdRule();
}

class MdItem extends MdBlock {
  const MdItem({
    required this.indent,
    required this.marker,
    required this.checked,
    required this.line,
    required this.text,
  });

  /// Nesting level (two spaces or one tab each).
  final int indent;

  /// `•` for bullets, `1.` for numbered items.
  final String marker;

  /// null for a plain item, otherwise the state of its `[ ]`/`[x]`.
  final bool? checked;

  /// The source line, so a checkbox tap can rewrite it.
  final int line;
  final String text;
}

final _heading = RegExp(r'^\s{0,3}(#{1,6})\s+(.*?)\s*#*\s*$');
final _rule = RegExp(r'^\s{0,3}([-*_])(\s*\1){2,}\s*$');
final _item = RegExp(r'^(\s*)([-*+]|\d+[.)])\s+(?:\[( |x|X)\]\s+)?(.*)$');
final _quote = RegExp(r'^\s{0,3}>\s?(.*)$');

List<MdBlock> parseMarkdown(String source) {
  final lines = source.replaceAll('\r\n', '\n').split('\n');
  final out = <MdBlock>[];
  final para = <String>[];
  void flush() {
    if (para.isNotEmpty) out.add(MdPara(para.join('\n')));
    para.clear();
  }

  for (var i = 0; i < lines.length; i++) {
    final line = lines[i];
    if (line.trimLeft().startsWith('```')) {
      flush();
      final code = <String>[];
      i++;
      while (i < lines.length && !lines[i].trimLeft().startsWith('```')) {
        code.add(lines[i++]);
      }
      out.add(MdCode(code.join('\n')));
      continue;
    }
    if (line.trim().isEmpty) {
      flush();
      continue;
    }
    final h = _heading.firstMatch(line);
    if (h != null) {
      flush();
      out.add(MdHeading(h[1]!.length, h[2]!));
      continue;
    }
    if (_rule.hasMatch(line)) {
      flush();
      out.add(const MdRule());
      continue;
    }
    final it = _item.firstMatch(line);
    if (it != null) {
      flush();
      final lead = it[1]!.replaceAll('\t', '  ').length ~/ 2;
      final m = it[2]!;
      out.add(
        MdItem(
          indent: lead,
          marker: RegExp(r'\d').hasMatch(m) ? m : '•',
          checked: it[3] == null ? null : it[3] != ' ',
          line: i,
          text: it[4]!,
        ),
      );
      continue;
    }
    final q = _quote.firstMatch(line);
    if (q != null) {
      flush();
      if (out.isNotEmpty && out.last is MdQuote) {
        out[out.length - 1] = MdQuote('${(out.last as MdQuote).text}\n${q[1]}');
      } else {
        out.add(MdQuote(q[1]!));
      }
      continue;
    }
    para.add(line);
  }
  flush();
  return out;
}

/// [source] with the task-list checkbox on [line] flipped.
String toggleCheckbox(String source, int line) {
  final lines = source.replaceAll('\r\n', '\n').split('\n');
  if (line < 0 || line >= lines.length) return source;
  lines[line] = lines[line].replaceFirstMapped(
    RegExp(r'\[( |x|X)\]'),
    (m) => m[1] == ' ' ? '[x]' : '[ ]',
  );
  return lines.join('\n');
}

// ---- inline ----------------------------------------------------------------

final _inline = RegExp(
  r'`([^`]+)`' // 1 code
  r'|\*\*(.+?)\*\*' // 2 bold
  r'|__(.+?)__' // 3 bold
  r'|~~(.+?)~~' // 4 strike
  r'|\*(?!\s)(.+?)(?<!\s)\*' // 5 italic
  r'|(?<![A-Za-z0-9])_(?!\s)(.+?)(?<!\s)_(?![A-Za-z0-9])' // 6 italic
  r'|\[([^\]]+)\]\(([^)\s]+)\)' // 7 text, 8 url
  r'|(https?://[^\s<>)]+)', // 9 bare url
);

Future<void> openLink(String url) async {
  final uri = Uri.tryParse(url);
  if (uri == null || !(uri.scheme == 'http' || uri.scheme == 'https')) return;
  try {
    if (Platform.isLinux) {
      await Process.run('xdg-open', [url]);
    } else if (Platform.isMacOS) {
      await Process.run('open', [url]);
    }
  } catch (_) {}
}

/// Inline Markdown as spans. Link recognizers are added to [taps], which the
/// caller disposes.
List<InlineSpan> inlineSpans(
  String text,
  TextStyle base,
  SoftColors s,
  List<GestureRecognizer> taps, {
  void Function(String url)? onLink,
}) {
  final spans = <InlineSpan>[];
  var at = 0;
  for (final m in _inline.allMatches(text)) {
    if (m.start > at) {
      spans.add(TextSpan(text: text.substring(at, m.start), style: base));
    }
    at = m.end;
    List<InlineSpan> inner(String t, TextStyle st) =>
        inlineSpans(t, st, s, taps, onLink: onLink);
    if (m[1] != null) {
      spans.add(
        TextSpan(
          text: m[1],
          style: base.copyWith(
            fontFamily: 'monospace',
            fontSize: (base.fontSize ?? 13) - 0.5,
            backgroundColor: s.line.withValues(alpha: 0.7),
          ),
        ),
      );
    } else if (m[2] != null || m[3] != null) {
      spans.add(
        TextSpan(
          children: inner(
            (m[2] ?? m[3])!,
            base.copyWith(
              fontWeight: FontWeight.w800,
              fontVariations: [const FontVariation('wght', 800)],
            ),
          ),
        ),
      );
    } else if (m[4] != null) {
      spans.add(
        TextSpan(
          children: inner(
            m[4]!,
            base.copyWith(
              decoration: TextDecoration.lineThrough,
              color: s.mute,
            ),
          ),
        ),
      );
    } else if (m[5] != null || m[6] != null) {
      spans.add(
        TextSpan(
          children: inner(
            (m[5] ?? m[6])!,
            base.copyWith(fontStyle: FontStyle.italic),
          ),
        ),
      );
    } else {
      final url = m[8] ?? m[9]!;
      final label = m[7] ?? url;
      final tap = TapGestureRecognizer()
        ..onTap = () => (onLink ?? openLink)(url);
      taps.add(tap);
      spans.add(
        TextSpan(
          text: label,
          recognizer: tap,
          mouseCursor: SystemMouseCursors.click,
          style: base.copyWith(
            color: s.accent,
            decoration: TextDecoration.underline,
            decorationColor: s.accent.withValues(alpha: 0.5),
          ),
        ),
      );
    }
  }
  if (at < text.length) {
    spans.add(TextSpan(text: text.substring(at), style: base));
  }
  return spans;
}

/// Text.rich for inline Markdown; owns and disposes the link recognizers.
class MdText extends StatefulWidget {
  const MdText(this.text, {super.key, required this.style});

  final String text;
  final TextStyle style;

  @override
  State<MdText> createState() => _MdTextState();
}

class _MdTextState extends State<MdText> {
  final _taps = <GestureRecognizer>[];

  void _disposeTaps() {
    for (final t in _taps) {
      t.dispose();
    }
    _taps.clear();
  }

  @override
  void dispose() {
    _disposeTaps();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    _disposeTaps();
    return Text.rich(
      TextSpan(
        children: inlineSpans(widget.text, widget.style, context.soft, _taps),
      ),
    );
  }
}

// ---- the view --------------------------------------------------------------

/// Renders [source]. Ticking a task-list box calls [onChanged] with the new
/// source; without it the boxes are read-only.
class MarkdownView extends StatelessWidget {
  const MarkdownView({super.key, required this.source, this.onChanged});

  final String source;
  final ValueChanged<String>? onChanged;

  @override
  Widget build(BuildContext context) {
    final s = context.soft;
    final body = manrope(
      13.5,
      FontWeight.w500,
      color: s.fg,
    ).copyWith(height: 1.5);
    final blocks = parseMarkdown(source);
    final children = <Widget>[];
    for (var i = 0; i < blocks.length; i++) {
      final b = blocks[i];
      final prev = i > 0 ? blocks[i - 1] : null;
      // Tight spacing inside a list, looser between other blocks.
      final gap = i == 0 ? 0.0 : (b is MdItem && prev is MdItem ? 3.0 : 10.0);
      children.add(
        Padding(
          padding: EdgeInsets.only(top: gap),
          child: switch (b) {
            MdHeading() => MdText(
              b.text,
              style: manrope(
                switch (b.level) {
                  1 => 19,
                  2 => 16.5,
                  3 => 14.5,
                  _ => 13.5,
                },
                FontWeight.w800,
                color: b.level >= 5 ? s.mute : s.fg,
              ).copyWith(height: 1.3),
            ),
            MdPara() => MdText(b.text, style: body),
            MdRule() => Divider(height: 8, color: s.line),
            MdCode() => Container(
              width: double.infinity,
              padding: const EdgeInsets.all(10),
              decoration: BoxDecoration(
                color: s.line.withValues(alpha: 0.5),
                borderRadius: BorderRadius.circular(8),
              ),
              child: SelectableText(
                b.text,
                style: body.copyWith(
                  fontFamily: 'monospace',
                  fontSize: 12.5,
                  height: 1.4,
                ),
              ),
            ),
            MdQuote() => Container(
              padding: const EdgeInsets.only(left: 10),
              decoration: BoxDecoration(
                border: Border(left: BorderSide(color: s.accent, width: 3)),
              ),
              child: MdText(b.text, style: body.copyWith(color: s.mute)),
            ),
            MdItem() => _item(context, b, body),
          },
        ),
      );
    }
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: children,
    );
  }

  Widget _item(BuildContext context, MdItem it, TextStyle body) {
    final s = context.soft;
    final done = it.checked ?? false;
    final Widget lead;
    if (it.checked != null) {
      lead = SizedBox(
        width: 22,
        child: Align(
          alignment: Alignment.centerLeft,
          child: PriorityCheck(
            priority: 0,
            done: done,
            size: 14,
            onTap: onChanged == null
                ? () {}
                : () => onChanged!(toggleCheckbox(source, it.line)),
          ),
        ),
      );
    } else {
      lead = SizedBox(
        width: it.marker == '•' ? 16 : 24,
        child: Text(
          it.marker,
          style: body.copyWith(color: s.mute, fontWeight: FontWeight.w700),
        ),
      );
    }
    return Padding(
      padding: EdgeInsets.only(left: 6.0 + it.indent * 16),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          lead,
          Expanded(
            child: MdText(
              it.text,
              style: done
                  ? body.copyWith(
                      color: s.mute,
                      decoration: TextDecoration.lineThrough,
                    )
                  : body,
            ),
          ),
        ],
      ),
    );
  }
}
