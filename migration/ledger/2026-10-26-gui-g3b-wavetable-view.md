## 2026-10-26: GUI G3b, the wavetable view in the oscillator sections

**Changed.** Wavetable Builder's `Wavetable3d` (#127) in place in each
oscillator section, with the API agreed then:

- The background pass paints the view's background (and, in 3D, its
  frames) before the wavetable name's selector, as `paintChildrenBackgrounds`
  paints OpenGL children before the preset selector; it takes the skin's
  view settings (`ViewSettings::from_skin`), the render type from
  `osc_N_view_2d`, the oscillator's on state and the model's wavetable.
- `show` draws the lines and handles the view's drag, which moves the wave
  frame slider, before the section's sliders; it gets this frame's
  telemetry snapshot when the engine publishes one, and the animate switch
  on (upstream's default, until the editor has the setting).
- The dimension button cycles `view_2d` only; the view reads the rest
  itself.
- The gallery's model renders its preset's wavetables
  (`PreparedWavetable::render`) for the view.

**Verified.**

- Pixels, side by side with ref-gui
  (`media/gui/g3b-oscillator-wavetable-view-side-by-side.png`): the three
  oscillators' views match, active and off, apart from the edit pencil
  (with the icons' follow-up).
- `cargo clippy --workspace --all-targets -D warnings`, `cargo fmt`, the
  `sas-gui` tests and `gui_reference.py layout --check` pass.
