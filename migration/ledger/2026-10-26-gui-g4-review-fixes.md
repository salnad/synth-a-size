## 2026-10-26: GUI, Voice Reviewer's fixes for the wavetable view and the macro column

**Changed.** Three fixes from Voice Reviewer's approvals of #136 and #139.

- **The wavetable view no longer draws inside the telemetry read.**
  `OscillatorSection::show_view` used to run `Wavetable3d::show` inside
  `telemetry.read`, holding the snapshot's lock while the view drew and
  took input. `Wavetable3d::read_telemetry` now copies what the view reads
  (whether voices are on, and the totals of its four controls) into a
  `ViewTelemetry` inside the read, as `Live::read` does. `show` takes that
  copy.
- **`rotary_option_bounds` truncates as `placeRotaryOption` does.**
  Upstream subtracts the integer half width from the float offset and
  truncates afterwards. Truncating the offset first gives a different
  answer when the result is negative, as with the oscillator's
  rotary-option override at size ratio 0.75.
- **The macro source buttons have their own fixture keys.** Upstream names
  a macro's knob and its source button both "macro_control_N", and
  `gui_reference.py layout` kept only the first, so the source buttons'
  bounds were never checked. Names repeated in the same place are now
  numbered `#2`, `#3`, ... in tree order; every existing key is unchanged.
  `layout_paths.txt` lists the four `macro_control_N#2` paths, and the
  layout test checks them against `MacroLayout::source`.

**Verified.**

- `rotary_option_offsets_truncate_after_subtracting_the_half_width`: the
  oscillator's option at size ratio 0.75 sits 8 pixels above the knob's
  centre, not 9, and the macro's at ratio 1 is unchanged.
- `LayoutForTest` (in `test_gui_reference.py`): a repeated name in a
  layout dump becomes `#2`, with its own bounds and visibility.
- The regenerated fixtures add only the four source buttons at each size.
  The layout test passes, and fails if the `#2` paths are pointed at the
  knob.
- `cargo clippy --workspace --all-targets -D warnings`, `cargo fmt`, the
  `sas-gui` tests, `test_gui_reference.py` and `gui_reference.py layout
  --check` pass.
