## 2026-10-27: GUI G5e (5), the wavetable editor's Frequency Filter, Slew Limiter, Wave Folder and Wave Warp overlays

Taken over from Wavetable Builder's G5e plan, on its overlay dispatch and
G5e-4a's shared plumbing: the overlays for the four modifiers whose
controls are sliders and toggles
(`vital/src/interface/wavetable/overlays/frequency_filter_overlay.cpp`,
`slew_limiter_overlay.cpp`, `wave_fold_overlay.cpp`,
`wave_warp_overlay.cpp`).

**Port** (`sections/filter_slew_fold_warp_overlays.rs`,
`sections/wavetable_edit.rs`)

- **Dispatch:** `OverlayKind::FrequencyFilter`, `SlewLimiter`,
  `WaveFolder` and `WaveWarp`, beside Phase and Window, with an arm in
  `overlay_input`, `paint_overlay` and `show_overlay`. Only the Frequency
  Filter's covers a display, the amplitudes
  (`setFrequencyAmplitudeBounds`); the others keep the base's setters.
- **Layouts** (`setEditBounds`, on `edit_strip` and `title_bounds`):
  - Frequency Filter: four sections of 4 strip heights; the style,
    cutoff and shape below the titles, NORMALIZE from the shape's right
    edge without padding, a sixth of the height in; titles STYLE, CUTOFF,
    SHAPE.
  - Slew Limiter: two limits of 4 heights; upstream lays the up limit out
    first, under the "DOWN LIMIT" title, and the port keeps that.
  - Wave Folder: one slider 4 heights wide, a float: the background
    truncates it and centres with integer division, the slider centres in
    floats; title MULTIPLY.
  - Wave Warp: X Asymmetric (3.5 heights), X warp (5, inset half a height
    each side), Y Asymmetric, Y warp; four lines; titles "", X WARP, "",
    Y WARP.
- **Controls** (`frameSelected`, `sliderValueChanged`, `sliderDragEnded`,
  `buttonClicked`): local controls, one set per oscillator
  (`SLIDER_MODIFIER_CONTROLS`), set from the keyframe when it's selected.
  The sliders have upstream's ranges and double-click values (cutoff 0 to
  10, 4; shape 0 to 1, 0.5; limits 0 to 1, 0; fold 1 to 32, 1; warps
  -20 to 20, 0), and, being named outside upstream's parameters, show
  JUCE's own slider text with seven decimals. A slider change writes the
  keyframe and auditions; the commit comes once no button is held. The
  filter style (Low Pass, Band Pass, High Pass, Comb) and the toggles
  (NORMALIZE, X and Y Asymmetric) write the modifier and commit at once.
  Nothing is written to the host.
- **D34**: the style selector shows the modifier's style once a keyframe
  is selected, where upstream's keeps the text it was last drawn with
  (approved by the coordinator, 2026-10-27).
- Shared with #194, identically: `Format::unassigned_decimal_places`
  and `just_text_toggle`.

**Fixtures and views**

- `layout_paths.txt`: each overlay with its keyframe selected
  (`@wavetable-select 0:3` to `0:6` in `wavetable_modifiers.vital`): the
  overlay, its background, its title texts and its controls; the layout
  test routes them to `SliderModifierLayout`, and every size matches.
- `views.json`: `wavetable_editor_frequency_filter`, `_slew_limiter`,
  `_wave_folder` and `_wave_warp`.

- `migration/reference/gui/README.md`: a note that the reference's text
  is larger than the gallery's (7.7% monospace, 3.3% Montserrat, 1.3%
  Lato) from FreeType's em sizing, pointing to `fonts.rs`; the port
  follows macOS (coordinator, 2026-10-27).

**Tests**

- `the_slider_modifiers_map_to_their_overlays`: the mapping, and only the
  Frequency Filter covering a display (the amplitudes).
- `tests/wavetable_editor.rs`: the overlays load their keyframes; the
  cutoff auditions while dragged and commits once; the filter style
  commits without an audition; NORMALIZE and X Asymmetric set their
  modifiers and commit once; the left slew slider is the up limit; the
  fold and warp sliders commit once; nothing reaches the host.

No audio change.
