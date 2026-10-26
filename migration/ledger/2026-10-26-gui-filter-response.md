## 2026-10-26: GUI shared filter response evaluators (`controls::filter_response`)

Split out ahead of G6a so that G3b's filter section can build on it. G3b
adds the analog, diode, dirty, formant and ladder evaluators here; G6b adds
the EQ evaluator.

**Changed**

- `controls::filter_response`: upstream's filter response vertex shaders
  (`look_and_feel/shaders.cpp`), evaluated on the CPU. Upstream runs them
  with transform feedback to turn a line's x positions into heights.
- One evaluator per shader, each taking `FilterResponseUniforms` (the
  shaders' `FILTER_RESPONSE_UNIFORMS`: `midi_cutoff`, `resonance`, `drive`,
  `mix`, `db24`, `stage0` to `stage4`). Each returns the line's y at one
  position, in the component's device coordinates.
  - `digital`: `kDigitalFilterResponseVertexShader`.
  - `comb`: `kCombFilterResponseVertexShader`, taking the line's y as well,
    which picks how the response rounds off past its sixth cycle.
  - `phaser`: `kPhaserFilterResponseVertexShader`.
- `RESPONSE_TOOLS` and `FILTER_RESPONSE_CONSTANTS` are ported with them,
  keeping the GLSL's f32 operation order and constants.
- Callers write the points into a `LineRenderer` with `set_x_at` and
  `set_y_at`, and keep their own fill, boost and colours.

**Tests**

`sas-gui` unit tests, each against values hand-evaluated from the shader
source:

- `cutoff_ratios_span_the_midi_range`.
- `digital_low_pass_at_its_cutoff`: +6.02 dB at resonance 0.5, and 0 dB
  dry at mix 0.
- `comb_peaks_and_dips_with_feedback`: with its filters open and feedback
  0.5, +3.52 dB at the cutoff, −6.02 dB an octave below, and flat without
  feedback.
- `phaser_notch_and_peak_at_the_cutoff`: 0 dB at the cutoff, and silence
  with the phase inverted.

The side-by-side check with ref-gui comes with G6a's viewers, which draw
these curves, rendered in the gallery's all-effects views.

No audio change; nothing draws these yet on main.
