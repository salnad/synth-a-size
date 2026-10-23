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
  init preset. The fixtures stay keyed by path.
- `migration/scripts/gui_reference.py layout` runs ref-gui once per preset and
  size (ref-gui already took `--preset`) and names the preset when a path is
  missing.
- `sas-gui` `tests/layout.rs` skips the `@` lines when it reads the path list.

No paths use the directive yet; G6a's Effects sections are the first.

**Checks**

- `gui_reference.py layout --check` passes on the committed fixtures, so the
  init-preset paths are unchanged.
- A trial `@preset all_effects.vital` block gave real bounds, for example
  `effects/container/CHORUS` at [240, 48, 552, 137] (1400×820) and
  [120, 24, 276, 68] (700×410).
- `test_gui_reference.py`: the directive applies to the paths after it, and a
  missing preset fails.
