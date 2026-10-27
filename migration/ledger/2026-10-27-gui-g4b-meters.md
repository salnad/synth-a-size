## 2026-10-27: GUI G4b-3a, the modulated controls' meters

**Changed.** A modulated control now shows where the engine has moved it,
as upstream's `ModulationMeter` (`modulation_meter.cpp`) and
`ModulationManager::renderMeters` do. The new module is
`sas-gui/src/modulation_meter.rs`.

- **What it draws.** An arc around a knob (`kRotaryModulationFragment`), or
  a bar beside a linear slider or at a text slider's side
  (`kLinearModulationFragment`). It runs from the control's own value to
  the engine's, separately for left and right: the left in
  `kModulationMeterLeft`, the right in `kModulationMeterRight` and their
  overlap in `kModulationMeter`. Knob arcs are `kKnobModMeterArcThickness`
  thick.
- **Where** (`getModulationMeterBounds`, then `getMeterBounds`):
  - A knob's arc is centred on it and is `kKnobModMeterArcSize` across,
    scaled by the knob size, plus the arc thickness for the rest, then
    moved down by `kKnobOffset`.
  - A linear slider's bar sits in the margin beside its track, with
    `kLinearWidthPercent`, `kLinearModulationPercent` and the widget
    margin, in upstream's integer arithmetic.
  - A text slider's bar is 2 px at its right, or at its left for the
    sliders that call `setModulationBarRight(false)`: the oscillators'
    transpose and unison voices, and the sample's transpose (new
    `Slider::modulation_bar_right`).
  - `setModulationArea` isn't ported. Upstream sets it only on tempo
    sliders, which aren't modulation destinations, and on keytrack sliders,
    where it comes to their own bounds.
- **Values** (`updateDrawing`): the destination's mono total, plus its poly
  total while any voice plays (`num_voices`), each clamped to the range.
  Knobs map them over ±`kRotaryAngle`. Upstream's `TextLookAndFeel` branch
  is unreachable (`rotary_` excludes text sliders) and isn't ported.
  `ModulationManager::read_meters` reads the modulated destinations' totals
  from the telemetry snapshot once a frame. Without an engine (tests, the
  gallery until #158) no meter draws.
- **When.** Only on a control with a connection that is on
  (`isModulated() && slider->isActive()`).
- **Order.** Upstream renders the meters before every component. Here each
  section panel collects its meters in a layer that paints before its
  controls, over the section's background.
- **Rotated coordinates.** Quads gain `rotated`
  (`OpenGlMultiQuad::setRotatedCoordinates`) for vertical bars and text
  sliders, whose meters run upwards. #160's exact coordinates turn with
  them.

**Verified.**

- **Gallery, no engine:** every view is pixel-identical to `main`.
- **Gallery with #158's live engine**, merged locally, against ref-gui
  with #129's live engine. Counts are differing pixels, first more than 24
  apart, then any difference:

  | View | `main` + #158 | This + #158 |
  |---|---:|---:|
  | `modulations` | 17414 / 98877 | 17403 / 98867 |
  | `modulations_many` | 17842 / 99323 | 17813 / 99305 |

  - Upstream's green meter ticks (oscillator level and pan, and the
    others the views modulate) all match: no meter-coloured pixel is more
    than 24 off. The worst at the meters is 13, on antialiased edges.
  - 3 changed pixels per view moved further from upstream, all
    antialiasing.
  - The linear and text-slider meters aren't in a gallery view; unit tests
    cover their geometry.
- **Tests:**
  - `modulation_meter` unit tests: a knob's arc rect and its left and
    right ranges in angles; clamping to the range; a text slider's 2 px
    bar on the right and on the left, rotated; a linear slider's bar in
    upstream's integer geometry; amount knobs have none.
  - `meters_read_the_engines_totals_adding_poly_while_voices_play`
    (`modulation.rs`): nothing without telemetry, mono only with no voices,
    mono plus poly with voices, and nothing for an unmodulated control.
  - `rotated_coordinates_run_a_linear_meter_upwards` (`tests/gpu.rs`):
    fails with the rotation removed.
- **Checks.** `cargo clippy --workspace --all-targets -D warnings`, and
  `-p sas-gui --features gpu-tests`, pass, as do the `sas-gui` and
  `sas-plugin` tests, `cargo test --release -p sas-gui --features
  gpu-tests`, `gui_reference.py layout --check` and `ledger.py --check`.
