## 2026-10-27: GUI G5e (4b), the wavetable editor's Line Source overlay

The second half of G5e-4. While a Line Source keyframe is selected, the
editor shows the Line Source overlay
(`vital/src/interface/wavetable/overlays/wave_line_source_overlay.cpp`) as
upstream draws it: the base `LineEditor` over the waveform, editing the
keyframe's own shape, with the pull power and grid sizes below it. It is
built on #195's generalised line editor (merged into this branch at
55bdc0e until #195 lands on main).

**Port** (`sections/line_source_overlay.rs`, `sections/wavetable_edit.rs`,
`controls/line_editor.rs`, `local.rs`)

- `OverlayKind::LineSource`, covering the waveform (`setTimeDomainBounds`
  returns true).
- `LineTarget::LineSource(oscillator)`: the base editor. It loops, has the
  plain menu without file loading (`setAllowFileLoading(false)`), a line 4
  pixels wide, a fill to the centre (`setFillCenter(0)`), and draws one line
  in `kWidgetPrimary1` with the `kWidgetSecondary1` fill, as the base
  `LineEditor::render` does (`LineTarget::stereo` is false); the LFO and
  remap editors keep their two channels and centre line. It has no shape in
  the synth: the section passes the keyframe's own shape, which the editor
  edits in place (#195's caller-owned shape).
- `LineEditor::paint_only`: the editor's second drawing each frame, after
  the displays, without handling input. Its first drawing is `show`'s,
  before them, in the overlay's input step.
- `LineSourceLayout` (`setEditBounds`): the pull power and the two grid
  sizes, each twice the strip's height wide, with the grid sizes'
  incrementer halves (unpainted and covered, as the Wave Source's are), and
  the background's three divider lines.
- Point additions and removals on the selected keyframe are made to every
  other keyframe of the source (`pointAdded` adds a middle point at the same
  index, or before the last point; `pointsAdded`, `pointRemoved`,
  `pointsRemoved`), and the source's `num_points` follows.
- What commits (D18):
  - A point or power drag auditions the playhead's frame while it moves
    (`pointChanged` and `powersChanged` with `mouse_up` false) and commits
    on release. A right-click's release over a point or power commits too,
    as the base `mouseUp` calls `drawUp` after a popup press (#195).
  - An added or removed point (a double click, the menu) commits at once.
  - A paint stroke (Command held) auditions as it paints and commits once,
    when the stroke ends. Upstream commits each painted cell
    (`pointsAdded` with `notifyChanged(true)`); its release reports nothing.
  - The pull power auditions while it drags and commits when the drag ends
    (`sliderValueChanged`, `sliderDragEnded`).
  - A double click's or the menu's power reset changes the shape with no
    listener call. Upstream commits the double click's release just before
    the reset, so its table plays the old power until the next commit. The
    port sees both in one frame and commits the shape with the reset, so
    the saved shape and the table agree (D35; the menu's Reset Power, which
    upstream also leaves uncommitted, commits the same way).
- The grid sizes set the editor's grid. Upstream also re-renders the
  playhead's frame for them (`notifyChanged(false)`), which changes
  nothing, so the port doesn't. Each wheel event over the editor steps the
  horizontal grid (`lineEditorScrolled`): up for a scroll up, down
  otherwise.
- `frameSelected`: the pull power loads from the selected keyframe.
- The new controls are local (`local::LINE_SOURCE_CONTROLS`: pull power,
  0 to 5, and grid sizes, 6 and 4 by default, per oscillator).
- `Format::juce_decimal_places`: a slider with no parameter behind it
  prints with JUCE's own `Slider::getTextFromValue`
  (`has_parameter_assignment_` false). The pull power's range has no
  interval, so it shows seven decimal places ("0.0000000").
- `paint_controls_background` leaves out a divider line at or past the
  background's right edge, as the background's own bounds clip it (the Line
  Source's third line).
- The base editor's fill meets its centre on a row boundary, and GL fills
  the lower row there. The base editor's fill is nudged down a hundredth of
  a pixel, as the wavetable editor's displays are. Measured against the
  reference frames, the LFO and remap views are unchanged with the nudge
  left off for them; applied to them as well, it added about 775 small
  differences to each and removed none.

**Harness**

- Layout paths for the Line Source overlay under `@wavetable-select 0:0`
  (#196's `wavetable_modifiers.vital`): the overlay, its background and
  three title texts, the pull power, the grid sizes and their four
  incrementer halves.

**Tests**

- Layout fixtures: the 12 new paths match at all six sizes.
- `tests/wavetable_editor.rs`: dragging a point auditions and commits once,
  leaving the other keyframe's shape alone; a double click adds a point
  that the other keyframe gains as a middle point, and a double click on it
  removes it from both, each committing; a paint stroke auditions and
  commits once when it ends, keeping both keyframes' counts in step; the
  pull power auditions and commits once when its drag ends; the wheel steps
  the horizontal grid; a right-click on a point commits. Nothing reaches
  the host in any of them.
- The overlay dispatch test now maps Line Sources, Phase Shifts and Wave
  Windows to their overlays and checks the displays each covers.
- Unit tests: point changes mirror into another keyframe; the Line Source
  target's settings; JUCE's text for a slider with no parameter.
- Side by side with ref-gui at 1400x820: in `wavetable_editor_groups`,
  nothing outside text rasterization differs from upstream by more than 24
  levels (6,330 pixels before the nudge and text format, 4,939 after, all
  text). In `wavetable_editor_select`, the editor's area differs only in
  the waveform display's last column (113 pixels), where upstream's faded
  waveform line reaches the edge and the port's doesn't. That is in the
  display, not the overlay, and is the same without the overlay.

**Correction to G5e-4a**

- Its ledger says `wavetable_editor_window` differs from upstream in no
  pixel by more than 24 levels outside text. It differs in 3 anti-aliased
  pixels at the waveform's top edge (Voice Reviewer).
