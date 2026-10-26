## 2026-10-26: GUI G6b: Compressor, EQ and Reverb

G6b completes the Effects tab, on top of G6a (#124): its own sections,
and GUI Builder's `FilterSection::fx()` from #125 in the filter's place.

**Sections** (`sections::effects`)

- `compressor`: `CompressorSection` and its editor (`CompressorEditor`).
  The mode selector shows Multiband, Low Band, High Band or Single Band and
  sets which bands draw. The editor draws each band's input level
  (`kGainMeter` quads) between its upper and lower thresholds, with ratio
  lines and handles. Dragging a threshold or a ratio moves it as upstream's
  `mouseDrag` does; a shift drag moves every threshold, unclamped, and a
  double click resets a ratio. The value popup shows below the editor.
- `equalizer_response`: `EqualizerResponse`, shared by the EQ and the
  reverb. It sets up three of the engine's own `DigitalSvf` models at
  100 kHz, as upstream does (`computeFilterCoefficients`), and draws their
  combined response through the new EQ evaluator, with the frequency lines,
  a filled handle for the selected band and rings for the others. A press
  on a handle selects its band; the drag moves its cutoff across and its
  gain up and down. The cutoff's value popup shows while dragging.
- `equalizer`: `EqualizerSection`. Mode buttons with upstream's high pass,
  notch and low pass icons toggle each band's mode and select it; a band in
  a pass or notch mode greys its gain. The LOW / BAND / HIGH tabs pick the
  knobs shown.
- `reverb`: `ReverbSection`, with its feedback EQ (two shelves, 40%
  headroom, no frequency lines) and the LOW / HIGH tabs over its top.

- The filter: #125's `FilterSection::fx()`, with its own power button
  (the tab skips its own there).

**Shared pieces**

- `controls::filter_response::eq` (`kEqFilterResponseVertexShader`). Its
  constants (notes 8 to 136, dB from -1 to 1) make the shader's output the
  response in dB.
- `Path::stroked` gains curved joints and rounded ends
  (`Paths::createFilterStroke`): a disc of half the thickness at each joint
  and open end, which the non-zero fill unites with the segments into
  JUCE's shape. Bevelled joints and square ends still panic.
- `controls::paths`: `low_pass`, `high_pass` and `notch`.
- `controls::shape_button`: `Shape::Icon` (a path filled to fit) and
  `ShapeColours::icon_on` (`useOnColors`).

**Effects tab**

- A press held on the scroll bar's track beside the thumb keeps paging,
  after 0.4 s and then every 0.04 s (`ScrollBar::timerCallback`).
- The FL Studio checklist's item 6 adds the compressor and EQ drags, and a
  wheel and trackpad speed check with a REAPER note.

**Not yet ported**

- The EQ's spectrogram under its response (`Spectrogram`, fed from the
  engine's equalizer memory). The response draws over the plain widget
  background. The spectrogram needs the memory in telemetry and the widget,
  which G6c's output displays share; it comes in its own PR.
- The Hz alternate display on cutoff knobs
  (`setSliderHasHzAlternateDisplay`), which the filter sections need too.
- `EqualizerResponse::mouseWheelMove` (the wheel over a handle turns its
  band's resonance).

**Tests**

- Layout fixtures: 46 paths for the compressor, EQ, reverb and filter,
  matching upstream at all six sizes.
- `tests/effects.rs`, with a host that applies sets at the next frame:
  - `a_compressor_threshold_drag_is_one_gesture_for_what_it_moves`: only
    the controls a drag changes get a gesture, one each.
  - `an_eq_handle_drag_is_one_gesture_per_control`.
  - `an_eq_mode_button_toggles_in_one_gesture`.
  - `track_presses_page_within_the_column`.
- Unit tests: `eq_stages_multiply_in_db`,
  `curved_joints_and_rounded_ends_stay_within_half_the_thickness`, and the
  compressor's `thresholds_format_like_upstream`,
  `compression_pulls_levels_towards_the_threshold` and
  `inactive_bands_give_their_span_to_the_middle`.
- Side by side with ref-gui (`all_effects_bottom`): the EQ's mode buttons,
  handles and frequency lines and the reverb's response and tabs match.
  The filter's response differs as G6a's live displays do (ref-gui's
  engine hasn't run), and #125's filter has no blend label icons yet.

No audio change.
