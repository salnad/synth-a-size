## 2026-10-27: GUI G5e (3a), the wavetable editor's Wave Source overlay

The first half of G5e-3. While a Wave Source or Shepard Tone source keyframe
is selected, the editor shows the Wave Source overlay
(`vital/src/interface/wavetable/overlays/wave_source_overlay.cpp`) as
upstream draws it, with its controls working. Drawing on the waveform and
the bars, their menus and the audible drag come in G5e-3b.

**Port** (`sections/wave_source_overlay.rs`, `sections/wavetable_edit.rs`)

- `OverlayLayout::wave_source`: `setOverlayPosition`'s edit strip below the
  waveform and `WaveSourceOverlay::setEditBounds`: the controls background
  centred in it, the interpolation selector, the two grid sizes, and each
  grid size's incrementer halves.
- `paint_controls_background` (`ControlsBackground::setPositions`): the body
  in a rounded rectangle with a border, the divider lines, a lighter strip
  behind each title, and the titles ("", "GRID X", "GRID Y"). Later overlays
  share it.
- The editors over the displays: the keyframe's own waveform with its fill
  and the snap grid's lines and circles (`WaveSourceEditor`), and its
  harmonics' amplitudes and phases (`BarEditor`s, `updateFrequencyDomain`,
  with a silent harmonic's phase at -1/2) in the overlay's bar colour.
- The editor's own displays fade out by `kObscureAmount` while the overlay
  covers them (`setColors`), and their corners round off after the overlay
  (`renderCorners`).
- Upstream draws the overlay twice each frame: `SynthSection`'s
  `renderOpenGlComponents` renders it as a visible sub-section before the
  displays, and `WavetableEditSection::renderOpenGlComponents` renders the
  current overlay again after them. The additive bars, the waveform's fill
  and the lighter strips show doubled, so the port draws it twice in the
  same order. Only the second drawing handles the controls.
- GL fills a row whose centre lies exactly on an edge with the lower row,
  where the port's rasterizer takes the upper. The grid's horizontal lines
  take GL's row, and the waveform is nudged down a hundredth of a pixel so
  its fill meets the centre line on GL's row.
- The interpolation selector (a `TextSelector` over upstream's five
  interpolation types) sets the source's interpolation style and mode and
  commits (`sliderValueChanged`, `creator::edits::set_wave_source_interpolation`).
  It never reads the source's own setting, as upstream never calls
  `setInterpolationType`, so it starts at "None".
- The grid sizes (0 to 16, 6 and 4 by default, back to them on a double
  click) set the snap grid. Their `IncrementerButtons` paint with JUCE's
  component painting, which `FullInterface` turns off, so upstream never
  shows them, and the always-on-top grid sliders over them take every
  press, so they never step the value either. The port lays them out (for
  the layout paths) and does nothing else with them.
- Each oscillator's editor keeps a Wave Source overlay and a Shepard Tone
  one, each with its own grid sizes and selector, as local controls
  (`local::WAVE_SOURCE_CONTROLS`): not saved or automated, as upstream's
  aren't.
- The overlays follow `frameSelected`: a press on an unselected keyframe,
  creating one, adding a source, a reset and showing the editor select one
  keyframe and show its overlay; deselecting, a selection box
  (`selectFrames` deselects first), removing a component or keyframes and a
  preset load hide it; keyframe drags carry it. After each component list
  edit (`componentsChanged`, `recreateVisibleFrames`), a selection of one
  keyframe is selected again, showing its overlay, and a larger one hides
  it.
- `Format::string_lookup` (`setStringLookup`), so a slider can show and take
  names from a list of the editor's own; `BarRenderer::y_at` and
  `scaled_y_at`; `Interface::local_value` for tests.

**Harness**

- The layout paths add the overlay, its background and title texts, the
  interpolation selector, the grid sizes and the four incrementer halves.

**Tests**

- Layout fixtures: the 12 new paths match at all six sizes.
- `tests/wavetable_editor.rs`: the overlay follows the selection (shown on
  opening, hidden by a click on empty space, shown by a press on the
  keyframe, hidden by a selection box); choosing "Spectral Blend" gives the
  source linear frequency-domain interpolation and commits once; a click on
  an incrementer's half is the grid slider's and leaves the size alone; a
  modifier added after a selection box took one keyframe shows its overlay
  again.
- Unit tests: the selector's values map to upstream's style and mode, and
  other components are left alone; a sine's harmonics; the snap radius.
- Side by side with ref-gui at 1400x820, `wavetable_editor` matches upstream
  in colour and placement; what differs is text rasterisation and a few
  levels of anti-aliasing at the waveform's edges and the snap circles.

No audio change.
