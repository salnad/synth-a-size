## 2026-10-27: GUI, the line editors rebuild their wave path when upstream's do

**Changed.** This follows #204's review notes. The animated line editors'
wrap points now take their boosts exactly when upstream rebuilds the wave
path (`setGlPositions` under `reset_positions_`, then `resetWavePath`).

- **Interactions.** Upstream calls `resetPositions` on every interaction
  that changes the editor's state: a hover change in `mouseMove`,
  `drawDown`, `drawDrag`, `drawUp`, the double click,
  `clearActiveMouseActions` and the temporary paint toggle. Edits call it
  too. The port marks the path dirty in a frame whose hover point, power or
  grid section, press, drag, paint stroke, temporary paint or pointer
  position on the editor changes, or whose shape is edited.
- **When the copy happens.** Both editors rebuild the path at the start of
  `render`, before the frame's decay and boost:
  - the first time;
  - after a resize;
  - after a dirty frame;
  - for an LFO, after its shape renders again from elsewhere.
    `LfoSection::reset` and `loadFile` call `resetPositions`.
- **The remap's second copy.** A remap curve's animated `render` also
  checks the render count in `LineEditor::render`, after its boosts, so a
  shape that renders again copies a second time, after the frame's boost.
  An LFO's `render` has no such check.
- **Code:** `wrap_copies` decides; `copy_wrap_boosts` copies.

**Verified.**

- **Unit tests:**
  - `the_path_is_rebuilt_as_upstream_resets_it`: first frame, still frame,
    interaction, resize, an LFO's re-render, a remap's re-render (after)
    and a remap's edit (before and after).
  - `wrap_points_take_boosts_only_when_the_path_is_rebuilt`: the loop's
    edge and the ends of a line that doesn't loop.
- **Checks:** `cargo clippy --workspace --all-targets -D warnings`, the
  `sas-gui` and `sas-plugin` tests, `gui_reference.py layout --check` and
  `ledger.py --check` all pass. The gallery's views catch no playhead at an
  edge, so their images are unchanged.
