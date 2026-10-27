## 2026-10-27: GUI G4c-3b (1), the Matrix rows' amount readouts and the line editors' wheel

**Changed.** This is stacked on #200 (G4c-3a).

- **`ModulationMeterReadouts`:** each connected row shows the slot's amount
  now (`modulation_amount_n`) along the top of its destination box.
  - The bar runs from the box's centre towards the amount's side, as far as
    the amount goes, in `ModulationMeterLeft` and then
    `ModulationMeterRight` for the two channels, added.
  - Geometry (`matrix::readout_bar`): `positionBar(i, centre, y, x -
    centre, 0)` at the bottom edge of the 2-pixel meter strip
    (`getMeterBounds`), widened downwards by `kBarHorizontalVertex`'s
    `width_percent`. `bar_width` is `64 · 2 / height`, so the bar is 2
    pixels thick, or 4 / width of the list's height if that is more.
  - It follows the rows' order and scroll. A clear or zero amount draws
    nothing. It isn't drawn without telemetry, as the other meters aren't.
- **The wheel over a line editor** (`lineEditorScrolled`): over an LFO's
  editor or the remap editor, the wheel moves the paint pattern while
  painting (including the temporary Command toggle), and otherwise the X
  grid.
  - `Slider::wheel_value` ports `SynthSlider::mouseWheelMove`. A clicking
    wheel moves an indexed control one step: up when `deltaY` is positive,
    down otherwise, within its range. Any other wheel takes JUCE's
    `Slider::mouseWheelMove`, which moves 0.15 of the range per unit and at
    least the interval.
  - `Wheel::events` reads egui's raw wheel events. A physical wheel clicks;
    a trackpad and its momentum are smooth, scaled as JUCE's macOS peer
    scales precise deltas (0.5 / 256 per point).
- **New gallery view `modulations_matrix_live`:** the modulations preset on
  the Matrix tab, with the chord progression played for a second and the
  animated views drawn.

**Not yet.**

- **G4c-3b (2):** the remap and LFO editors' animated position and boosts.
- **The wheel's value popup:** `mouseWheelMove` shows the slider's popup
  (`showPopup(true)`). The port hides a slider's popup on any frame it
  isn't hovered, an approximation of JUCE's `mouseExit`, so a popup shown
  from the editor would vanish at once. A follow-up after G4c-3b (2) ports
  JUCE's `mouseExit` for slider popups, then turns on `showPopup(true)` for
  the editor's wheel.
- **The wheel on the port's other sliders:** `SynthSlider::mouseWheelMove`
  is used only here so far.

**Verified.**

- **Against ref-gui's `modulations_matrix_live` (1400 × 820):** the
  readouts on rows 1 to 5 (LFO 1, Env 2 and Velocity on Osc 1's level, pan
  and wave frame) are the same pixels in both.
  - The Random 1 row's bar differs in length, because the random source's
    value at that moment differs between the two renders.
  - Over the list, the live frames differ from each other by 40 more
    pixels (more than 24 off) than the still frames do, all on that row.
- **Unit tests:**
  - `readouts_run_from_the_centre_to_the_amount`: right and left, clamped,
    nothing for zero or clear, and the 4 / width minimum.
  - `the_wheel_steps_an_indexed_control`: up, down, a zero delta going
    down, both ends, and JUCE's smooth move, including at least one step.
- **`the_wheel_over_a_line_editor_moves_its_grid_or_pattern`:** a clicking
  wheel over LFO 1's editor steps its X grid. With paint on, it steps the
  pattern instead. Over the remap editor it steps the matrix's own X grid
  and leaves LFO 1's.
- **Checks:** `cargo clippy --workspace --all-targets -D warnings`, the
  `sas-gui` and `sas-plugin` tests, `gui_reference.py layout --check` and
  `ledger.py --check` all pass.
