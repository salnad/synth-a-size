## 2026-10-26: GUI G3b (3), the filter sections and their responses

**Changed.** The Voice tab's FILTER 1 and 2 and the Effects tab's filter,
which Tabs Builder places (`sections/filter.rs`, `FilterSection::voice` /
`FilterSection::fx`), with their response curves.

- `FilterSection`: both layouts (`positionTopBottom` with the source
  inputs, `positionLeftRight` with the text-component model selector),
  power button and off overlay, the horizontal or sideways heading, the
  controls each model shows (`showModelKnobs`: formant X/Y, transpose,
  peak and spread; the comb's cut), the drive and key track labels, the
  model and style text with its arrows (`prevClicked` / `nextClicked`
  wrap through every model's styles), and the sources it filters: OSC1–3
  and SMP toggle that source's destination (`toggleFilter1/2`), and
  turning on FIL1/FIL2 turns the other filter's input off
  (`filterSerialSelected`).
- The model list is upstream's two-column `DualPopupSelector`
  (`popup::DualMenu`, `Overlay::dual_menu`): models on the left, the
  chosen model's styles on the right, keeping the style when its name
  matches; a choice applies at once and the list stays open.
- `FilterResponse` (`controls/filter_view.rs`): as upstream, the engine's
  own filter of the chosen model (`sas-filters`) is set up at the visual
  sample rate and its coefficients become the response shader's
  uniforms; the curve is drawn for both channels while the filter is on
  (right, then left) and in the disabled colours when off, cached until
  the controls change. Dragging on it moves cutoff and resonance (formant
  X and Y) in one gesture each.
- Response evaluators added to `controls/filter_response.rs` (#119): analog,
  positive and negative flange, diode, dirty, formant and ladder, each a
  line-by-line port of its vertex shader, with hand-derived tests.
- `Slider::popup_prefix` (`setPopupPrefix`): linear and rotary sliders'
  popups read "prefix value" unless the parameter has names; text
  sliders' read only the prefix. The oscillator's morph, detune power and
  frame sliders now carry theirs.
- `controls::button::text_toggle`, `Panel::text_toggle`: a text toggle
  whose state isn't a parameter.
- The gallery renders a view's preset (`views.json`); four authored
  presets turn both filters on with every model between them
  (`gui/presets/filters_*.vital`). ref-gui now draws with animation off,
  so animated views show the controls rather than its silent engine's
  outputs (a filter response at cutoff 0).
- Still to come: the blend arrows' icons (after #116), the response's
  animation from the engine's modulated values, and the mouse wheel on
  the response.

**Verified.**

- Layout: every Voice filter control, label, input and the model
  selector added to the fixtures; the port's rectangles equal upstream's
  at all six sizes. The effects filter's layout equals upstream's
  1400×820 bounds in a unit test.
- Pixels, side by side with ref-gui for all eight models
  (`media/gui/g3b-filter-responses-side-by-side.png`): the curves and the
  sections match apart from the blend icons.
- Interaction tests: a source input reroutes its oscillator, one filter
  feeds the other at a time, the arrows wrap through models, the model
  list picks a model then a style and stays open, and a drag on the
  response moves cutoff and resonance in their gestures.
- `cargo clippy --workspace --all-targets -D warnings`, `cargo fmt` and the
  `sas-gui` tests pass.
