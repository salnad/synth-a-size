## 2026-10-26: GUI G3b (4b), the envelope editor

**Changed.** The envelope sections' editor (`controls/envelope_editor.rs`,
`EnvelopeEditor`), in place of 4a's backdrop.

- Drawing, in upstream's order: the time ruler's labels (`formatTime`,
  fading in between divisions), the grid and sub-grid lines, the curve
  (the right channel's fill, the left's, then the centre line), the point
  rings and the power dots halfway along the attack, decay and release,
  the hover ring and the drag circle, and the rounded corners.
- Editing (`pickHoverPosition`, `mouseDrag`): the nearest point is picked
  as upstream does; the delay, attack, hold and release points move their
  times; the sustain point moves the decay time and the sustain level; a
  vertical drag on a power dot bends it, and a double click straightens
  it. Dragging the decay or release past the window's end widens the
  window. Each drag is one gesture per parameter, and the editor draws its
  own values until the host has them.
- `Colour::overlaid_with` (JUCE's `overlaidWith`, in its integer steps).
- Still to come: the playing voices' positions and the animated curve
  (from the engine's telemetry), zooming with the wheel, and the
  magnifying glass (zoom by drag, reset by double click).

**Verified.**

- Pixels, side by side with ref-gui
  (`media/gui/g3b-envelope-editor-side-by-side.png`): the init envelope's
  curve, rings, power dot, grid and time labels match; the magnifying
  glass is still to come.
- Unit tests: point positions for known times, the hover pick, the time
  labels, and `overlaid_with` against JUCE's arithmetic.
- Interaction tests: dragging the attack point lengthens the attack in one
  gesture; dragging the sustain point sets decay and sustain; a double
  click on the attack power dot straightens it.
- `cargo clippy --workspace --all-targets -D warnings`, `cargo fmt` and the
  `sas-gui` tests pass.
