## 2026-10-26: GUI G6c (2): the oscillators' advanced sections

Part 2 of the Advanced tab, on top of part 1 (#142): each oscillator's
advanced section where the Voice tab's oscillator is
(`sections::oscillator_advanced`, upstream's `OscillatorAdvancedSection`).

**OSC n OPTIONS** (`OscillatorOptions`): NOTE TRACK and HI-RES WAVETABLE
text buttons. Active while the oscillator is on; otherwise its tab shadow
goes and the off overlay covers it (`createOffOverlay`, `setActive`).

**OSC n UNISON** (`OscillatorUnison`): the stack selector with upstream's
long names, detune range, unison blend, stereo unison, and the table,
spectral and distortion spreads. Active while the oscillator is on with more
than one voice (`checkActive`). Upstream never lays out its SPECTRAL UNISON
button (0 by 0), so the port doesn't show it.

**Spread display** (`SpreadVisualizer`): with more than two voices, a bar per
voice in three rows (wave frame, spectral morph, distortion), each voice's
value spread across the voices and clamped, from the modulated values in
telemetry (mono plus poly while the processor runs), drawn with
`BarRenderer`.

`Panel::off_overlay` draws a section's off overlay on top, for sections
whose activator lives elsewhere; `Panel::activator` uses it.

**Follow-ups**

- File dialog (review, UI, #142): the editor clears the dialogs' parent
  window when its window closes, so an X11 handle doesn't keep its
  connection open until the next window. FL item 8 adds removing the
  plugin, or closing the project, while the dialog is open (`rfd` can't
  cancel the sheet, and its completion handler is in the plugin binary).
- The reverb's labels are drawn at `size_ratio * 10` points, the font its
  `paintBackground` sets before `drawLabelForComponent`, instead of the
  label height (GUI Builder found them about 9% too large).
  `section::draw_label_with_size` draws a label at a given size.

**Tests**

- Layout fixtures: 36 paths (each oscillator's section, subsections and
  controls) match upstream at all six sizes.
- `tests/advanced.rs`, with a host that records gestures:
  `the_frequency_units_set_the_editor_setting`,
  `an_oversampling_button_sets_the_amount_in_one_gesture`,
  `a_built_in_tuning_loads_named_after_itself` (the 5-limit major third is
  5/4) and `note_track_toggles_in_one_gesture`.
- Side by side with ref-gui (`init_advanced`): the sections, their off
  overlays and labels match. "HI-RES WAVETABLE" overflows where upstream
  curtails it (JUCE's `drawText`, routed to GUI Builder). The macro column
  on the left is the modulation interface's, not part of this tab.

No audio change.
