## 2026-10-26: GUI G5 (1), the wavetable view (`Wavetable3d`)

**Changed.** `controls::wavetable_3d::Wavetable3d` ports upstream's
oscillator wavetable view as a standalone component. G3b's oscillator
section places it: it passes the bounds, its wave frame slider and that
slider's bounds, the oscillator's wavetable (`EditorModel::wavetable`,
#117), this frame's telemetry snapshot and the animate switch. The view
reads everything else itself from the host parameters: `osc_N_view_2d`
(the render type, which G3b's dimension button cycles), `osc_N_on`
(active), and the frame, spectral morph and distortion controls.

- **The three render types** (`renderWave`, `renderSpectrum`):
  - **2D:** the playing frame as a filled line a quarter of the view high.
  - **3D:** every frame of the table as a stack (`paint3dBackground`: 257
    strokes in the accent colours, every 16th in the first, each with a
    gradient into the body), with the playing frame's line at its place in
    the stack and ring end caps (`drawPosition`). The stack is built once
    and kept until the table, the bounds, the activity or the colours
    change.
  - **Spectrum:** the frame's harmonics on a log frequency scale in dB,
    with 3 dB per octave of tilt, mirrored about the centre
    (`loadSpectrumData`).
- **The playing frame** goes through the oscillator's own display
  functions (`run_spectral_morph`, `adjust_phase`, `get_phase_window`,
  `interpolate`, `set_spectral_morph_values_for`,
  `set_distortion_values_for`), as `loadIntoTimeDomain` does. While
  animating, the controls' modulated values come from the telemetry's
  modulation totals: mono, plus poly while voices play
  (`getOutputsTotal`). Lines are rebuilt only when those values, the types,
  the bounds or the table change (`updateRenderValues`).
- **Dragging the view moves the frame slider** (`mouseDown`, `mouseDrag`):
  the view's height spans the slider's range, the value is clamped at
  every step, and the slider's popup shows at the slider and hides when
  the pointer leaves the view. The drag keeps its own value, as
  upstream's `current_value_` does, rather than reading the parameter back
  (the host applies sets a frame later).
  - Divergence: upstream sets the slider without a host gesture. The port
    wraps the drag in one gesture, so FL Studio records one automation
    pass and one undo step.
- **Painter:** `PixelPainter::gradient_polyline` (a stroke with a linear
  gradient, as a non-radial `ColourGradient`) and `Colour::with_alpha`.

Not in this PR:
- The right-click menu (save, copy, paste, initialize, resynthesize), the
  audio-file drop (on #121's adapter patch) and the wavetable editor come
  next in G5. Text to Wavetable stays parked (GUI-only, needs sign-in).
- **Mouse wheel:** upstream forwards the wheel to the frame slider. It
  stays out, like the shared `Slider`'s wheel (G2), because baseview
  doesn't report macOS natural scrolling.

**Verified.**

- Unit tests (`controls::wavetable_3d`):
  - the projection equals `setDimensionValues`;
  - the render types follow `view_2d`;
  - with no morph or distortion, the 2D line is a known two-harmonic frame
    to within 0.01 px. It reads two samples on, from the oscillator's
    interpolation of the padded morph buffer, as upstream's view does;
  - a 3D line starts at its frame's place in the stack and rises by the
    wave range;
  - the spectrum mirrors about the centre, the fundamental stands well
    above the floor, the empty bins sit on it, and points past the width
    park off the right edge.
- Interaction tests (`tests/controls.rs`, deferred host sets):
  - a 20-pixel drag on a 100-pixel view moves the frame 51.2 in one
    begin/set/end gesture, with the popup at the frame slider;
  - a drag past the top clamps to the last frame and comes back from the
    clamp;
  - leaving the view hides the popup.
- `cargo clippy --workspace --all-targets -D warnings`, `cargo fmt` and the
  sas-gui tests pass.
- Side-by-side screenshots against upstream come once G3b places the view
  in the oscillator section, in the GUI gallery.

**Host checks.** None new. The drag's single gesture shows in FL Studio
as one automation pass (also in REAPER, with its automation in Touch or
Latch mode).
