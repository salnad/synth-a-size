## 2026-10-26: GUI G3b (1), the Voice tab's oscillator and sample sections

**Changed.** The first G3b PR. The Voice tab now shows its synthesis column
(`sections/synthesis.rs`, `SynthesisInterface`): the three oscillator
sections and the sample section with their controls, and the two filter
sections' places, which the filter PR fills.

- Oscillators (`sections/oscillator.rs`, `OscillatorSection`): the power
  button and off overlay; the PITCH, UNISON and PHASE joint controls
  (transpose, tune, voices, detune, phase, random phase) with the
  transpose-quantize dots and the unison viewer's bars; level and pan; the
  spectral and wave morph knobs, whose bipolarity, double-click value and
  active state follow their type; the spectral morph, wave morph and
  destination texts with their menus (dividers and the FM/RM modulator
  suffixes as upstream) and prev/next arrows; the frame slider; the
  distortion phase slider for the wave morphs that use it, which shortens
  the wavetable area; the wavetable name strip's arrows; and the dimension
  button, which steps `osc_N_view_2d` back through 3D, 2D and spectrum.
- Choosing a wave morph type that closes an FM/RM loop between oscillators
  resets the last oscillator in the loop, as
  `SynthesisInterface::distortionTypeChanged` does. The check takes the
  chosen type rather than reading it back: nice-plug applies a set later
  (CLAP queues an event; VST3 waits while a block is processing).
- Read-after-set audit (UI Reviewer): a slider now keeps its own value
  while it is held, as JUCE's `Slider` does. Drags compare against it (not
  the host's possibly stale value), the pitch wheel springs back from it,
  and the slider and its popup draw it, so they no longer lag a frame
  behind the pointer. Menu follow-ups get the chosen value.
- Sample (`sections/sample.rs`, `SampleSection`): power button and off
  overlay, PITCH joint control with the quantize dots, level, pan, the
  destination with its menu and arrows, and the backgrounds of the
  waveform and the buttons.
- Shared: `section.rs` gains `slider_width`, `slider_overlap(_with_space)`,
  `place_joint_controls`, `paint_joint_control` and
  `draw_label_connection`; `Panel` draws always-on-top components (the off
  overlay, then the power button) after the rest, as
  `renderOpenGlComponents` does; `controls::shape_button` ports
  `OpenGlShapeButton` (chevrons, invisible selectors and just-text buttons,
  with the hover fade); `controls::transpose_quantize` ports
  `TransposeQuantizeButton`'s paint; `Quad::vertical_bar` places a
  `BarRenderer` bar; `Layer` draws additive quads; `Slider` gains the
  text-entry width, the text offset override and the double-click value.
- Still to come in G3b: the wavetable and sample names and the sample
  waveform (with Wavetable Builder's data accessors), Wavetable3d in its
  place (Wavetable Builder), the sample's keytrack, loop, random phase and
  bounce icons and the oscillator's edit icon (after #116's paths), the
  transpose-quantize call-out, the filters, the modulation column, and the
  slider's mouse wheel and shift-drag by octaves.

**Verified.**

- Layout: every oscillator and sample component, and both filter places,
  added to the fixtures; the port's rectangles equal upstream's at all six
  sizes. Oscillator 3 is also laid out with a new authored preset
  (`gui/presets/osc_3_sync.vital`, Sync wave morph) so the distortion phase
  slider and the shortened wavetable area are checked.
- Pixels, side by side with ref-gui
  (`media/gui/g3b-oscillators-sample-side-by-side.png`, and the port at
  2x in `g3b-port-oscillators-2x.png`): the sections match apart from what
  is still to come; the off overlay matches upstream's to the pixel.
- The test host now applies sets at the start of the next frame by
  default (as if a block ran in between), and every test passes that way;
  both FM loop tests (arrows and menu) fail with the old read-back.
- Interaction tests: the destination menu and arrows (one gesture, wrap
  round), the wave morph menu's dividers and modulator names, an FM loop
  broken on the last oscillator, morph knobs' double-click values by type,
  the dimension button, and the sample destination's arrows.
- `cargo clippy --workspace --all-targets -D warnings`, `cargo fmt` and the
  `sas-gui` tests pass.
