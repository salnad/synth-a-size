## 2026-10-27: GUI G5e, the wavetable editor's overlay dispatch

Groundwork for G5e-4 to G5e-6, which add the Line Source, Phase Shift,
Wave Window, modifier and Audio File Source overlays. G5e-3 showed only the
Wave Source overlay, with its dispatch written for it alone.

**Port** (`sections/wavetable_edit.rs`)

- `OverlayKind`: the component overlays (`WavetableOverlayFactory`) by the
  type of the component whose keyframe they edit, with `of` (the overlay
  for a component, if it has one yet) and `obscures` (the displays it
  covers, as `setTimeDomainBounds`, `setFrequencyAmplitudeBounds` and
  `setPhaseBounds` report). The Wave Source overlay is its one kind so far,
  for Wave Sources and Shepard Tone sources.
- The section's `current_overlay`, `overlay_input`, `paint_overlay` and
  `show_overlay` dispatch on it: the input before anything draws, then the
  two drawings around the displays and the corners, as before. An overlay
  is added as a variant with an arm in each.
- `Obscured`: the displays fade one by one (`setColors`'
  `obscure_time_domain_`, `obscure_freq_amplitude_` and
  `obscure_freq_phase_`), not all three together.

**Tests**

- A unit test maps the component types to their overlays and the Wave
  Source overlay to the displays it covers. The editor and layout tests
  pass unchanged, and the gallery's `wavetable_editor` view is
  byte-identical.

No audio change.
