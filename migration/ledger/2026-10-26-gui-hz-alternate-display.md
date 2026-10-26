## 2026-10-26: GUI, the hertz alternate display on cutoff sliders

**Changed.** `SynthSection::setSliderHasHzAlternateDisplay` for the shared
`Slider`, so cutoff sliders can read in hertz when the "hz_frequency"
setting is on.

- `Slider::hz_alternate()` marks a slider. `Slider::displayed(hz)` is the
  slider as it shows its value (`getDisplayDetails`): with the setting on,
  its details become upstream's `hz_details` (exponential, no post offset,
  " Hz", times `kMidi0Frequency`) with the display's exponential base
  2^(1/12) (`setDisplayExponentialBase`). The drawn text, the popup and
  text entry all follow, since `Panel::slider` and `slider_input` use the
  displayed slider.
- `EditorModel::hz_frequency_display`: the setting
  (`LoadSave::displayHzFrequency`), false by default as upstream's is when
  the config doesn't have it. Tabs Builder wires the setting's switch and
  its config in G6c.
- The Voice filters' and the effects filter's cutoff sliders use it; the
  effects' cutoffs (distortion, flanger, phaser, equalizer, reverb) take it
  up with G6.

**Verified.**

- `cutoffs_read_in_hertz_with_the_setting_on`: a cutoff of note 60 reads
  "Cutoff: 261.6 Hz" with the setting on and no hertz with it off; typing
  440 sets note 69; a slider without the alternate never changes.
- `cargo clippy --workspace --all-targets -D warnings`, `cargo fmt` and the
  `sas-gui` tests pass.
