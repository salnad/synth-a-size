## 2026-10-27: GUI G5e (4a), the wavetable editor's Phase Shift and Wave Window overlays

The first half of G5e-4. While a Phase Shift or Wave Window keyframe is
selected, the editor shows that modifier's overlay
(`vital/src/interface/wavetable/overlays/phase_modifier_overlay.cpp`,
`wave_window_overlay.cpp`) as upstream draws it, with its controls and
editors working. The Line Source overlay follows in G5e-4b, on the
generalised line editor (#195).

**Port** (`sections/phase_window_overlays.rs`, `sections/wavetable_edit.rs`,
`local.rs`)

- `OverlayKind::Phase` and `OverlayKind::Window` in the overlay dispatch
  (#193). The Phase Shift's overlay obscures no display, as its
  `setTimeDomainBounds` returns false; the Wave Window's obscures the
  waveform.
- `PhaseLayout` and `WindowLayout` (`setEditBounds`): the controls
  background in the edit strip, sized from the strip's height, with the
  style selector, the controls' phase editor and the mix slider, or the
  shape selector and the two position sliders. The title rows and divider
  lines reuse the Wave Source overlay's `paint_controls_background`, through
  the new `edit_strip` and `title_bounds` helpers.
- The Phase Shift's two `PhaseEditor`s, one over the waveform while a
  keyframe is selected and one in the controls: one-pixel ticks from the
  bottom, one per sixteenth of a turn from the phase, halving in height at
  each finer division (`updatePositions`). A drag turns the phase by a whole
  turn per editor width (`updatePhase`), auditioning the playhead's frame
  (`phaseChanged(phase, false)`), and the release commits
  (`phaseChanged(phase, true)`).
- The mix slider auditions as it moves and commits when its drag ends
  (`sliderValueChanged`, `sliderDragEnded`). The style selector (upstream's
  "Harmonic Phase", over the five phase styles) sets the modifier's style
  and commits.
- The Wave Window's `WaveWindowEditor` over the waveform: the window
  curve (200 points, `setPoints`) with the inside and outside fills, and a
  bar at each edge. A press within the grab radius of an edge grabs the
  nearer one (`getHover`); the drag moves it (`changeValues`, each edge kept
  on its side of the other), auditioning, and the release commits. The
  position sliders follow the editor (`windowChanged`), and moving a slider
  moves its edge, auditions, and commits at the end of the drag. The shape
  selector sets the modifier's window shape and commits.
- `frameSelected`: the mix, and the window's positions, load from the
  selected keyframe. Upstream sets the phase style from the modifier with a
  notification, which re-renders an unchanged table when the selector held
  another style; the port sets it quietly, with nothing to commit. Upstream
  never sets the window shape selector from the modifier, so it keeps its
  own value, as in the port.
- The new controls are local (`local::MODIFIER_CONTROLS`: phase style,
  phase mix, window shape, left and right position per oscillator): not
  saved or automated, as upstream's aren't. Continuous local controls no
  longer round to whole numbers; only indexed ones do.
- Every drag auditions only, and commits once on release (D18).
- The section's own waveform display is nudged down a hundredth of a pixel,
  as the Wave Source overlay's waveform is, so its fill meets the centre
  line on GL's row (the Phase Shift overlay leaves the display unfaded,
  which showed the tie). The phase ticks get the same nudge, so a tick top
  on a row boundary leaves that row empty, as upstream does. The shared
  constant is `wavetable_edit::GL_TIE_NUDGE`.

**Harness**

- Views `wavetable_editor_phase` and `wavetable_editor_window`: #196's
  `wavetable_modifiers.vital` with the Phase Shift (0:1) or Wave Window
  (0:2) keyframe selected.
- Layout paths for both overlays under `@wavetable-select`: the overlay,
  its background and three title texts, and the controls.

**Tests**

- Layout fixtures: the 15 new paths match at all six sizes. With one
  mapping broken on purpose, the test fails.
- `tests/wavetable_editor.rs`: a drag over the waveform shifts the phase by
  the dragged fraction of a turn, auditioning the playhead's frame and
  committing once; the controls' phase editor does the same at its own
  width; the mix slider auditions and commits once when its drag ends; the
  style selector sets the phase style and commits once; dragging a window
  edge moves it, the sliders follow, and the release commits once; a press
  between the edges grabs nothing; the shape selector sets the window shape
  and commits once. Nothing reaches the host in any of them.
- Unit tests: the phase ticks follow the phase; the nearer window edge is
  grabbed.
- Side by side with ref-gui at 1400x820: outside text rasterization,
  `wavetable_editor_phase` differs from upstream in 4 pixels by more than
  24 levels (anti-aliasing on the waveform's edge), and
  `wavetable_editor_window` in none. Before the display's nudge, the Phase
  Shift view also differed along the waveform's centre line, and at two
  tick tops. `wavetable_editor` improves slightly with the nudge.
  `wavetable_editor_groups` and `wavetable_editor_select` select a Line
  Source keyframe, whose overlay comes in G5e-4b.
