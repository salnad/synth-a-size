## 2026-10-23: GUI layout fixtures laid out with a preset

First G6 PR. Upstream lays out only what a preset shows: the Effects tab's
`EffectsInterface::setEffectPositions` sizes enabled effects only, so with the
init preset every effect section has empty bounds and the G6 sections had no
upstream geometry to check against.

**Changed**

- `migration/corpus/generate.py` authors `migration/corpus/gui/presets/all_effects.vital`:
  the corpus base preset with all nine effects on, in the default order
  (rule 3: generated, nothing copied). `--check` covers it.
- `migration/corpus/gui/layout_paths.txt` takes `@preset <file>` lines: the
  paths after one are laid out with that preset loaded, the rest with the
  init preset. The fixtures stay keyed by path; each path may be listed only
  once (a duplicate fails rather than taking the last preset's bounds).
- `migration/scripts/gui_reference.py layout` runs ref-gui once per preset and
  size (ref-gui already took `--preset`) and writes a `"preset"` field into
  each entry laid out with one. Init entries are unchanged byte for byte.
  Presets, in `@preset` lines and in `views.json`, must be files in
  `migration/corpus/gui/presets/`.
- `sas-gui` `tests/layout.rs` skips the `@` lines when it reads the path list,
  and reads each entry's preset: an entry laid out with a preset the test
  doesn't know fails (G6a adds `all_effects.vital`), so the port's check can't
  silently assume a preset's state.
- CI's GUI reference job already runs `gui_reference.py layout --check` with
  a freshly built ref-gui, so G6a's first `@preset` fixtures are regenerated
  and compared there.

No paths use the directive yet; G6a's Effects sections are the first.

**Checks**

- `gui_reference.py layout --check` passes on the committed fixtures, so the
  init-preset paths are unchanged.
- A trial `@preset all_effects.vital` block gave real bounds, for example
  `effects/container/CHORUS` at [240, 48, 552, 137] (1400×820) and
  [120, 24, 276, 68] (700×410).
- `test_gui_reference.py`: the directive applies to the paths after it; a
  missing preset, a preset outside `migration/corpus/gui/presets/` (including
  through `..`) and a path listed twice each fail.
- With a trial `@preset` block, the entry carries the preset and the layout
  test fails naming it (UI Reviewer on #107).
