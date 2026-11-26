import 'package:fake_async/fake_async.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:utre/desk/debounced.dart';

void main() {
  const quiet = Duration(milliseconds: 300);

  test('typing saves once, after the quiet time', () {
    fakeAsync((t) {
      final saved = <String>[];
      final e = DebouncedEdit(initial: 'a');
      Future<void>? save(String v) {
        saved.add(v);
        return null;
      }

      for (final v in ['ab', 'abc', 'abcd']) {
        e.edited(v, save);
        t.elapse(const Duration(milliseconds: 100));
      }
      expect(saved, isEmpty);
      t.elapse(quiet);
      expect(saved, ['abcd']);
      expect(e.dirty, isFalse);
    });
  });

  test('flush saves at once and the timer does not save again', () {
    fakeAsync((t) {
      final saved = <String>[];
      final e = DebouncedEdit(initial: 'a');
      e.edited('b', (v) {
        saved.add(v);
        return null;
      });
      e.flush();
      expect(saved, ['b']);
      t.elapse(quiet * 2);
      expect(saved, ['b']);
    });
  });

  test('dispose flushes a pending edit', () {
    fakeAsync((t) {
      final saved = <String>[];
      final e = DebouncedEdit(initial: 'a');
      e.edited('b', (v) {
        saved.add(v);
        return null;
      });
      e.dispose();
      expect(saved, ['b']);
    });
  });

  test('typing back to the stored value saves nothing', () {
    fakeAsync((t) {
      var saves = 0;
      final e = DebouncedEdit(initial: 'a');
      Future<void>? save(String v) {
        saves++;
        return null;
      }

      e.edited('ab', save);
      e.edited('a', save);
      e.flush();
      t.elapse(quiet);
      expect(saves, 0);
    });
  });

  test('a remote value never replaces unsaved typing', () {
    final e = DebouncedEdit(initial: 'a', delay: null);
    e.edited('mine', (_) => null);
    expect(e.incoming('theirs', 'mine'), isNull);
    expect(e.dirty, isTrue);
  });

  test('a remote value is shown when the user has not typed', () {
    final e = DebouncedEdit(initial: 'a', delay: null);
    expect(e.incoming('theirs', 'a'), 'theirs');
  });

  test(
    'an untouched field does not write the old text back over a remote edit',
    () {
      final saved = <String>[];
      final e = DebouncedEdit(initial: 'a', delay: null);
      // The field was focused but nothing typed; a sync brings 'theirs'; then blur.
      final shown = e.incoming('theirs', 'a');
      e.edited(shown!, (v) {
        saved.add(v);
        return null;
      });
      e.flush();
      expect(saved, isEmpty);
    },
  );

  test('the echo of a trimmed save leaves the typed text alone', () {
    fakeAsync((t) {
      final e = DebouncedEdit(initial: 'x', normalize: (v) => v.trim());
      e.edited('foo ', (_) => null);
      t.elapse(quiet);
      // The store echoes the trimmed title while the user's text still ends in a space.
      expect(e.incoming('foo', 'foo '), isNull);
    });
  });

  test(
    'a flush after the field moved on writes to the record that was edited',
    () {
      fakeAsync((t) {
        final saved = <String>[];
        final e = DebouncedEdit(initial: 'one');
        e.edited('one!', (v) {
          saved.add('task1:$v');
          return null;
        });
        // The pane now shows task 2; its closure is different, the pending one is bound.
        e.flush();
        expect(saved, ['task1:one!']);
      });
    },
  );
}
