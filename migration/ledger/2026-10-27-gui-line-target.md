## 2026-10-27: GUI, the line editor for any shape (LineTarget)

**Changed.** `LineEditor` now edits a shape named by a `LineTarget`
instead of always an LFO's. Every per-target difference lives in
`impl LineTarget`, so a target is added in one place.

- **The targets:**
  - `Lfo(i)` is `LfoEditor`.
  - `Remap(slot)` is `LineMapEditor`, a Matrix row's remap curve. G4c-3a
    places it in the Matrix tab.
  - Wavetable Builder will add a Line Source keyframe target for G5e-4.
- **`impl LineTarget`** holds:
  - `looping` (`loop_`). An LFO's line loops: its level ends move together
    (`movePoint`) and its path wraps (`resetWavePath`). A remap curve's
    line doesn't: its ends move apart, and its wrap points repeat the ends.
  - `menu`: `LfoEditor::mouseDown` or the base `LineEditor::mouseDown`.
  - `file_loading` (`allow_file_loading_`): the base menu's Copy, Paste,
    Save to LFOs and Initialize. A Line Source turns it off
    (`setAllowFileLoading(false)`).
  - `line_width` and `fill_center`. An LFO uses the skin's for both. A
    remap curve uses the skin's width and fills from the bottom
    (`LineMapEditor`'s -1). A Line Source uses 0 and 4.
  - `init_shape`: an LFO's triangle, a remap curve's line (`initLinear`).
  - `read` and `write`: the shape through the editor model.
  - `points_edited`: the frame's point changes, as upstream's
    `LineEditor::Listener` hears them. That is `pointAdded`,
    `pointRemoved`, `pointsAdded` and `pointsRemoved` from painting, and
    `fileLoaded` for Paste and Initialize, as `PointEdit`s. It is a no-op
    for LFOs and remap curves. A Line Source keyframe mirrors additions and
    removals into its other keyframes here (`addMiddlePoint`,
    `removePoint`). `LineEditorEvent::edits` returns the same list to the
    section.
- **A caller-owned shape:** `show(.., Some(&mut shape))` edits the
  caller's shape in place and skips the target's `read` and `write`, for a
  wavetable Line Source whose shape lives in the wavetable section's
  working creator. `None` keeps the LFO and remap behaviour exactly as it
  was.
- **The listener calls:** `LineEditorEvent::changes` lists the frame's
  `LineEditor::Listener` calls in upstream's order, so the shape's owner
  can audition mid-drag and commit on `mouse_up` (D18).
  `LineEditorEvent::changed` says whether the frame changed the shape.
  - `PointChanged { index, mouse_up: false }` comes from `movePoint`
    (`line_editor.cpp:232`), and `PowersChanged { mouse_up: false }` from
    `movePower` (:267).
  - `mouse_up: true` comes from `drawUp` on a release when not painting:
    the active point's `pointChanged` or `powersChanged(true)` (:620-637).
    It fires even after a press with no move.
  - A right-click's release does the same for the base editor's targets
    (a remap curve, a Line Source), since the base `mouseUp`
    (:619-623) has no popup check. `LfoEditor::mouseUp` skips popup
    releases (`lfo_editor.cpp:105-107`), so an LFO reports nothing.
  - The flips send `PointChanged { index: 0, mouse_up: true }` (:304-315).
  - `Points(PointEdit)` covers `pointAdded`, `pointRemoved`,
    `pointsAdded`, `pointsRemoved` and `fileLoaded`, which have no
    `mouse_up` flag.
  - **No call at all:** a paint stroke's release (`mouseUp` calls `drawUp`
    only when not painting), and a power reset by double click or by the
    menu. The owner decides when to commit after those.
  - **Order:** a double click's release reports `drawUp` before the double
    click's own change, as `mouseUp` comes before `mouseDoubleClick`.
- **The base menu (remap curves):** Remove Point (not on the ends), a
  divider after a point's items, Reset Power, and under `file_loading`:
  Copy, Paste, Save to LFOs and Initialize. Then Flip Horizontal and Flip
  Vertical. It has no Set Start Point.
- **`EditorModel::line_mapping` and `set_line_mapping`:** a slot's remap
  curve, through the edit bus's existing `set_line_mapping`, applied off
  the audio thread's allocation path as LFO shapes are. `PluginModel`
  implements both.
- **Fixed: the LFO menu no longer offers Initialize.** It came in with
  #173, but upstream's `LfoEditor::mouseDown` (`lfo_editor.cpp:55-98`)
  has none. Initialize is only in the base menu (`line_editor.cpp:489-495`,
  `kInit` at 361). The coordinator confirmed this against the source with
  Voice Reviewer.

**The LFO menu against upstream's.** Upstream's full list is Set Start
Point, Remove Point, Enter Point Phase, Enter Point Value, Reset Power,
Copy, Paste, Save to LFOs, Flip Horizontal, Flip Vertical, Import LFO and
Export LFO.

- **The port has:** Set Start Point, Remove Point, Reset Power, Copy, Paste
  (when the clipboard holds a shape), Save to LFOs, and both flips.
- **Missing:**
  - Enter Point Phase and Enter Point Value, the point's text entry
    (`getTextEditorComponent`). This is also missing from the base menu.
  - Import LFO and Export LFO (file dialogs).
- **Planned:** both, in G4c-3c, the approved third part of the Matrix
  remap work. That covers both editors.

**Verified.**

- **Unit tests:**
  - `targets_differ_as_upstreams_editors_do`.
  - `a_line_that_does_not_loop_keeps_its_ends_apart`: `movePoint` and the
    path's ends, against the looping path.
  - `point_changes_are_recorded`: a double click's addition and removal,
    and painting's `pointsAdded`.
- **Stand-in editor tests:** a `Remap` target in a small harness, with a
  caller-owned shape (`tests/controls.rs`):
  - `a_caller_owned_shape_reports_upstreams_listener_calls`:
    - a double click off the line gives `pointAdded`;
    - a click on a point gives its release;
    - a point drag gives `pointChanged(1, false)` frames, then
      `pointChanged(1, true)`;
    - a power drag gives `powersChanged(false)` frames, then `(true)`;
    - a double click on a point gives two releases, then `pointRemoved`;
    - the target's shape is never written.
  - `a_paint_strokes_release_calls_nothing`: the stroke's calls are all
    point additions and removals, and its release is silent.
  - `the_line_menus_listener_calls`: the flips give `pointChanged(0,
    true)`, Reset Power changes the shape silently, Remove Point gives
    `pointRemoved`, and Initialize gives `fileLoaded`.
  - `without_a_callers_shape_the_target_is_written`: the model gets one
    `set_line_mapping`.
  - `a_right_clicks_release_is_reported_except_on_an_lfo`: on a remap
    curve, a right-click over a point gives `pointChanged(1, true)` and
    over a power `powersChanged(true)`. On an LFO it gives nothing. The
    test fails both with no report and with a report for the LFO too.
- **Plugin:** `editor_model_lines`: a slot's curve and an LFO's shape read
  back through `PluginModel` as set, and other slots keep the init line.
- **Controls tests:** `the_lfo_menu_flips_the_shape` now expects upstream's
  LFO menu, with no Initialize. `the_lfo_menu_saves_the_shape` drops the
  Initialize step.
- **Checks:** `cargo clippy --workspace --all-targets -D warnings`, the
  `sas-gui` and `sas-plugin` tests, `gui_reference.py layout --check` and
  `ledger.py --check` all pass.
