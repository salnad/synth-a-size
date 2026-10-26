## 2026-10-26: GUI G3b (4c), the LFO editor

**Changed.** The LFO sections' shape editor (`controls/line_editor.rs`,
upstream's `LineEditor` with `LfoEditor`'s menu and drawing) and the
controls around it.

- Drawing, at rest: the grid (and the painted cell while painting), the
  shape with its loop wrap points (`resetWavePath`, smooth and powers
  applied) in the right, left and centre passes, the point rings and
  power dots, the hover ring and drag circle, and the corners.
- Editing: points drag between their neighbours, snapping near the grid
  (Alt doesn't snap), and level end points move together; power dots bend
  their segment (Shift bends all, Alt alternates), clamped to
  `SynthLfo::kMaxPower`; a double click removes a point, straightens a
  power (Shift: all) or adds a point; paint mode (the paint button, or
  Command held) draws the paint pattern into the grid cell under the
  pointer (`paintLine`). The right-click menu sets the start phase (to a
  point, a power's middle or a grid cell), removes a point, resets a
  power, copies the shape as JSON and flips it.
- The editor keeps its own copy while editing and until the synth has it,
  so it never draws a stale shape.
- `EditorModel::lfo_shape` / `set_lfo_shape`: the shape from the edit
  bus's document and edits through `EditHandle::set_lfo_shape` (the path
  the chat edits already use). A full queue is retried next frame.
- LFO section: the grid size sliders ("X Grid", "Y Grid"), the paint
  pattern selector (`PaintPatternSelector`, drawn as its pattern, choosing
  from the pattern names), the paint and smooth buttons, and the shape's
  name in the strip.
- `local`: controls the editor keeps itself (upstream's sliders without a
  synth parameter: each LFO's grid sizes, paint pattern and paint button),
  behaving as sliders through `Host` but stored per editor, so they aren't
  saved or automated, as upstream's aren't.
- Still to come: the paint and smooth buttons' icons (with the shared
  `controls::paths`, #122), entering a point's phase or value as text,
  pasting, saving, importing and exporting shapes (with the browsers), the
  wheel over the editor, and the playing voices' positions and the
  animated line.

**Verified.**

- Pixels, side by side with ref-gui
  (`media/gui/g3b-lfo-editor-side-by-side.png`): the init triangle, its
  points and power dots, the grid, the grid sizes, the paint pattern and
  the name match; the paint and smooth icons and the tempo note are still
  to come.
- Unit tests: level ends move together, painting fills one cell, powers
  clamp.
- Interaction tests (through the editor, with a model that stores the
  shapes): dragging a point moves it, a double click adds a point,
  painting draws the pattern into a cell, and the menu flips the shape.
- `cargo clippy --workspace --all-targets -D warnings`, `cargo fmt` and the
  `sas-gui` tests pass.
