# Golden (screenshot) tests

Each test renders a screen with fixed data and a fixed clock (`goldenNow` in `fake_db.dart`:
Wed 2026-11-25 10:30) and compares it with a PNG in `goldens/`, in light and dark.
No database, daemon or network is involved: `FakeDb` stands in for the core, and the real
fonts (Manrope, Material Icons) are loaded from the app's own assets (`loadAppFonts`).

| File | What |
|---|---|
| `calendar_golden_test.dart` | Day / Week / Month on desktop, Day and 2 Days on a phone; current-time line, done, overlap, all-day, Google events |
| `task_card_golden_test.dart` | The card in every state, desktop and phone |
| `date_picker_golden_test.dart` | Desktop dialog and phone bottom sheet |
| `devices_golden_test.dart` | Devices page with three fake peers, and with none |
| `shell_golden_test.dart` | The whole shell at desktop width (detail open) and phone width |

## Running

```sh
cd app
flutter test --tags golden                  # compare
flutter test --update-goldens --tags golden # regenerate (Linux only)
flutter test --exclude-tags golden          # everything else (what the "Flutter" CI job runs)
```

Goldens are **generated and compared on Linux only**: on macOS and elsewhere the tests skip,
because text rasterisation differs. The comparison allows 0.2 % of pixels to differ, which
absorbs anti-aliasing noise between Linux machines but not a moved or recoloured widget.

## Updating after a UI change

1. Run `flutter test --update-goldens --tags golden` on Linux and look at the changed PNGs
   (`git diff --stat`, then open them). Commit them with the UI change.
2. If CI fails but your machine passes, download the `golden-failures` artifact of the run:
   it holds `<name>_masterImage.png`, `_testImage.png`, `_isolatedDiff.png` and `_maskedDiff.png`.
   If the CI render is the right one, copy each `_testImage.png` over `goldens/<name>.png`.

## Adding one

Use `goldenTheme('name', (tester, brightness, file) async { ... })` from `golden_utils.dart`
(it runs the body in light and dark), `setSurface` for the window size (pixel ratio 1 keeps
PNGs small), and `themed(...)` for a Soft-themed `MaterialApp`. Anything that reads the clock
should take `goldenNow` (`DeskController(now: ...)`); a screen that still calls
`DateTime.now()` needs fixture dates built from the real today (see `sampleTasks`).

Glyphs missing from Manrope show as boxes: the test renderer has no system fallback font,
so use Material icons rather than characters such as `▾` in the UI (the phone view picker did).
