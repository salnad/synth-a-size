## 2026-10-26: GUI shared TempoSelector and upstream's hand-built icons

Shared ahead of G6a (the chorus, delay, flanger and phaser rates) and
G3b/G4 (the LFO and random rates), so there is one copy.

**Changed**

- `controls::tempo::TempoSelector` (`editor_components/tempo_selector.cpp`),
  built on the sync control as a `Slider::selector`:
  - A press opens upstream's menu (Seconds, Tempo, Tempo Dotted, Tempo
    Triplets), plus Keytrack when the control's maximum reaches it, as
    the LFOs' does. The chosen item sets the value, through the Overlay's
    `MenuOwner::Selector`.
  - A right-click opens the slider's own menu (`SynthSlider::mouseDown`).
  - It draws the mode's icon in `kIconSelectorIcon`, with the dotted
    mode's dot.
  - `TempoMode` has `shows_free_slider`, `shows_tempo_slider` and
    `shows_keytrack_sliders`, upstream's `valueChanged` visibility rules,
    for each section to pick its rate sliders.
  - `paint_divider` is `SynthSection::drawTempoDivider`.
- `controls::paths`: upstream's hand-built icons (`Paths` in
  `look_and_feel/paths.h`) in their unit squares: `clock`, `note`,
  `triplet_notes`, `keyboard` (the sample section's keytrack button) and
  `keyboard_bordered` (the keytrack mode). G3b's loop, bounce, shuffle and
  pencil belong here too.
- `controls::path` gets its JUCE credit line (`juce_graphics`,
  GPL-3.0-only) and the compound SPDX expression `GPL-3.0-or-later AND
  GPL-3.0-only`, following #120's rule 1.
- `Panel::slider_input`: `Panel::slider` without the drawing, for controls
  that draw themselves.

**Tests**

`sas-gui` unit tests:

- `controls::tempo`: `modes_pick_their_sliders` (each mode's slider
  visibility) and `keytrack_is_offered_only_where_the_control_has_it`
  (four items for `chorus_sync`, five for `lfo_1_sync`).
- `controls::paths`: `icons_span_their_squares` and
  `the_keyboard_has_three_keys`.

No audio change; nothing on main draws these yet.
