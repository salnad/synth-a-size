## 2026-10-27: GUI G4c-2, editing the Matrix tab's rows

**Changed.** The Matrix tab's rows now edit as upstream's do
(`modulation_matrix.cpp`). This is stacked on #185, the edit bus's
`set_slot_route`.

- **Selectors** (`ModulationSelector`):
  - A press on a row's source opens the source list (`source_popup_items_`:
    "-" then each source's menu name).
  - A press on its destination opens the two-column selector, 420 px wide
    (`destination_popup_items_`):
    - "-";
    - "Global", the destinations under no group's prefix
      (`createMiscSubMenu`);
    - then upstream's groups and dividers (`kDestinationSubMenuPrefixes`,
      Delay listed twice as upstream has it). The oscillator and filter
      groups name their items by `local_description`, the rest by
      `display_name`.
  - The ids are indices into the names, "-" and then the sources or
    destinations in natural order (`source_strings_`,
    `destination_strings_`, `NaturalStringComparator`).
- **Choosing** (`ModulationMatrixRow::sliderValueChanged`):
  - Each row holds its own source and destination choice. One without the
    other connects nothing and shows until the routes next change
    (`updateDisplay`).
  - Once both are chosen, or when a connected row changes either, the
    row's own slot is rewired through `EditorModel::set_slot_route`. That
    is one engine change, and the host hears nothing: no gestures and no
    undo entries (D18).
  - "-" frees the slot.
  - A route to the slot's own amount is refused, and the row shows "-"
    again.
  - The choice selects the row.
- **Row selection** (`rowSelected`):
  - A press on a row's background (`mouseDown`) selects it.
  - Its toggles select it through `buttonClicked`: on a release over the
    toggle, not on the press.
  - Its sliders select it only when their value changes
    (`sliderValueChanged`); a press that doesn't move one leaves the
    selection as it was. `Panel::slider_changed` compares the frame's
    input value with the host's value before it.
  - Its selectors select it on a choice. Re-choosing the item already
    shown does nothing, since `Slider::setValue` notifies only a change: no
    selection and no `set_slot_route`.
  - A state load that changes a row's bipolar, bypass or stereo selects a
    row, as `setAllValues` sends those toggles' `buttonClicked`.
    - It selects the last changed toggle's row in `setAllValues`' order,
      which is its button map's: by name.
    - So `modulation_10_*` comes before `modulation_2_*`, and with slots 2
      and 10 changed, slot 2 is selected.
    - The sliders are set without notification, so they select nothing.
  - **Not copied: ref-gui's second highlight.** ref-gui's
    `modulations_matrix` also highlights row 1. That comes from the
    harness, which loads the preset before attaching the editor, so the
    rows see their toggles change as they are built (see G4c-1's ledger).
    A plugin opening its editor on an already-loaded state doesn't do
    that, so the port doesn't copy it. That one highlight is a known
    difference between that view and ref-gui.
- **Sorting** (`ModulationMatrix::mouseDown`, `sort`):
  - A press on the headings sorts by the column under it, using upstream's
    hit columns (`bipolar_x = source_width + 1.5·x_padding`, ...). Pressing
    the sorted column again reverses it.
  - It is a stable sort of the current order by the column, then connected
    rows first.
  - Rows are re-sorted only then and when the routes change
    (`updateModulations`), not as values move.
  - The sorted column's heading is in the regular face.
- **Bipolar** (`buttonClicked`): the curve follows a bipolar toggle in the
  frame it is clicked (`power_slider_->setBipolar`), not a frame later as
  the host value arrives.
  - It flips the value read before the button handles the mouse, so the
    curve is right whether the host applies the set at once or later.
- **The two-column selector** (`DualPopupSelector`, `PopupList`) gained:
  - left rows chosen by themselves, as "-" is;
  - dividers (a lighten line across the middle of a row, never hovered or
    chosen);
  - no left selection until one is chosen (`showSelections` with nothing
    marked);
  - a right list that scrolls with the wheel and its scroll bar when its
    rows are taller than it. The Mod Matrix group lists 128 destinations,
    each slot's amount and power.
  - The filter type selector uses it as before.

**Verified.**

- **Tests (`tests/controls.rs`):**
  - `a_rows_selectors_rewire_its_own_slot_without_the_host`: the source
    list and destination selector open on a press. Source alone connects
    nothing. The destination connects slot 1. A new source rewires it, and
    "-" frees it. The bus gets one entry each, and the host's gesture log
    stays empty throughout.
  - `a_row_cannot_modulate_its_own_amount`.
  - `rows_are_selected_by_a_press_and_by_a_load_changing_their_toggles`.
  - Four tests from Voice Reviewer's second review. Each fails when its
    fix is reverted:
    - `a_rows_toggle_selects_it_on_a_release_over_it`: not on the press,
      nor on a release off it.
    - `a_load_selects_the_last_changed_row_in_name_order`: slots 2 and 10
      select 2, and slots 9 and 19 select 9.
    - `a_bipolar_click_redraws_the_curve_in_that_frame`: checked in the
      release's own frame, with a host that applies sets at once and with
      one that defers them.
    - `rechoosing_a_selectors_item_does_nothing`.
  - `a_rows_slider_selects_it_only_when_it_moves`: a press on row 2's
    amount, and one on its morph curve, leave row 1 selected. A drag on the
    amount selects row 2. The test fails when the amount's change doesn't
    select the row.
  - `pressing_a_heading_sorts_the_rows`: number descending, back, then
    source ascending.
- **Unit tests:** the stable sort and connected-first order, the rows
  shown, natural order of the names, and the destination list's groups,
  dividers and local names.
- **Gallery:** `init_matrix` and `modulations_matrix` are pixel-identical
  to G4c-1's. The filter tests pass with the extended selector.
- **Checks.** `cargo clippy --workspace --all-targets -D warnings` passes,
  as do the `sas-gui` and `sas-plugin` tests, `gui_reference.py layout
  --check` and `ledger.py --check`.
