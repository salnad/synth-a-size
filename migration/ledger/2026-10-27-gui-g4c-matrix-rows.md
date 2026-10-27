## 2026-10-27: GUI G4c-1, the Matrix tab's list of connections

**Changed.** The Matrix tab, which drew nothing, now shows upstream's
`ModulationMatrix` list (`modulation_matrix.cpp`) in the new
`sections/matrix.rs`. Editing comes in G4c-2, and the remap section's
controls in G4c-3.

- **Layout** (`resized`, `setRowPositions`, `ModulationMatrixRow::resized`):
  - The list is a 12-row viewport of 34 px rows (scaled), each with 1 px of
    padding, under a title-width heading.
  - The remap section's body sits below it, `kPadding` away.
  - Each row places, by upstream's percentages and integer arithmetic: the
    bypass toggle (its number), the source, bipolar, stereo and morph
    (power curve) controls, the amount bar, and the destination.
  - New layout fixture paths: the container, rows 0, 1 and 63, row 0's
    seven controls, and row 63's amount.
- **Painting** (`paintBackgroundShadow`, `paintBackground`,
  `paintScrollableBackground`, the rows' `paintBackground`):
  - The two bodies with their tab shadows and borders.
  - The headings, each with a lighten-screen line before it. The sorted
    column's name (the number) is in the regular face, the rest light, at
    0.4 of the title width.
  - The list's background colour under the heading.
  - Each row: the body colour, the text component backgrounds behind the
    source and destination, and the curve slider's widget-background box
    (`SynthSlider::drawShadow`).
  - The selected row's lighten-screen highlight (row 1, as
    `parentHierarchyChanged` selects it), and the off overlay on a row
    without a connection.
- **Rows shown** (`sort` with the default number column, then
  `compareConnected`; `checkNumModulationsShown`): connected slots first,
  then the rest, each by slot, and one row more than connected.
- **Selectors.** The source and destination show
  `getMenuSourceDisplayName`. They are `OpenGlSlider`s, not `SynthSlider`s,
  so `TextLookAndFeel` draws them at half their height, with no offset and
  never dimmed.
- **Controls.**
  - The bypass toggle shows its number, or "X" while bypassed
    (`BypassButton`). Stereo reads "L/R". Both have no background (new
    `ButtonStyle::JustText`).
  - The bipolar toggle uses the new `Icon::Bipolar` (`Paths::bipolar`,
    whose second outline upstream leaves empty).
  - The power curve follows the slot's bipolar.
  - The amount bar is bipolar, and reads in the destination's units for a
    linear or indexed destination (`updateDisplayValue`).
  - All are the slot's host controls, through the panel as elsewhere.
- **Scrolling.** The wheel over the list and the scroll bar (`scroll_bar_`)
  scroll the rows, clipped to the viewport.

**Not yet.**

- Selecting rows: a click, and on a state load the last row whose toggle
  changed (`buttonClicked` → `rowSelected`).
  - In ref-gui's `modulations_matrix` frame, row 6 (bypassed in the
    preset) is lit as well as row 1. Loading set its toggle with a
    notification before the map editors existed, so `rowSelected` moved
    only the highlight. `parentHierarchyChanged` then selected row 1 again
    without clearing row 6.
  - That ordering belongs to the harness's load-before-attach. G4c-2 takes
    the selection rules.
- The source and destination menus, sorting by heading, and the remap
  section's controls and readouts (G4c-2, G4c-3).

**Verified.**

- **Gallery against ref-gui:**
  - `init_matrix`: pixels more than 24 off in the list area go from 22,696
    (nothing drawn) to 1,313. They are the "-" glyphs one pixel low, the
    port's usual text difference.
  - New view `modulations_matrix` (the `modulations` preset on the Matrix
    tab): all seven rows and the empty eighth match upstream in place,
    text, toggles, curves and bars, except row 6's highlight above.
  - The remap section is empty until G4c-3, so most of each frame's
    remaining difference is there.
  - Every other view is pixel-identical to `main`.
- **Tests:**
  - `a_row_lays_out_as_upstreams`: ref-gui's bounds for row 0's controls.
  - `connected_rows_come_first_with_one_empty_row_after`: the order and
    the row count.
  - `a_matrix_rows_controls_are_its_slots`: bypass and amount are one host
    gesture each.
  - The layout fixtures for the new paths. Widening the bypass toggle by a
    pixel fails them.
- **Checks.** `cargo clippy --workspace --all-targets -D warnings` passes,
  as do the `sas-gui` and `sas-plugin` tests, `gui_reference.py layout
  --check` and `ledger.py --check`.
