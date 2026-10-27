## 2026-10-27: GUI G4c-3a, the Matrix tab's remap editor and header

**Changed.** The Matrix tab's remap section, below the list, now works as
upstream's (`ModulationMatrix`). It is stacked on #195 (`LineTarget`).

- **The curve editor** (`map_editors_`):
  - It edits the selected row's remap curve (`LineTarget::Remap(slot)`),
    through `EditorModel::set_line_mapping`.
  - It fills the section below the header row, inset by the widget margin
    (`setRowPositions`'s `mapping_bounds`).
  - It is drawn in the disabled colours while the selected row has no
    connection (`setActive(connected())`): the fills in
    `WidgetSecondaryDisabled`, and the line, centre line and points in
    `WidgetPrimaryDisabled` (`LineMapEditor::render`, `renderPoints`).
    `LineSettings::active` is new for this; the LFO editors pass true.
- **The header** (`resized`): "MOD REMAP n" for the selected row, in the
  title face at 0.45 of the title width, in the heading colour; paint and
  its pattern; the X and Y grid sizes; smooth; and the LFO file strip.
  - Paint, pattern and grid sizes are local controls the 64 editors share,
    as upstream's sliders are: `modulation_matrix_grid_size_x` (8),
    `_grid_size_y` (1), `_paint` (off) and `_paint_pattern`. The pattern
    starts at 0, "Step": `ModulationMatrix` never sets it, unlike
    `LfoSection`'s `kDown`.
  - Smooth sets the selected curve's smooth flag.
  - The strip shows the selected curve's name, where upstream keeps the
    last loaded file's name across rows (D37). Its arrows step through the user's
    LFO files (`prevClicked`, `nextClicked`, `getShiftedFile`). A press on
    its name opens the popup browser for them, 500 × 250 at the strip's
    right edge and top (`textMouseDown`), with no added folders. A chosen
    file loads into the selected curve, named after the file (`loadFile`).
- **Shared with the LFO section:** the LFO section's editor and header code
  is now `lfo::show_line_controls`, used by both. `popup_browser::load_lfo_file`
  became `load_line_file` for any target, and `BrowserOwner::Remap(slot)`
  is new.
- **Background:** painted in `paintBackground`'s order: the editor's
  widget background, the strip's, paint with its pattern, the grid sizes,
  and the "-" between them.

**Not yet.**

- **G4c-3b (mine):**
  - the remap editor's animated input position and boosts
    (`LineMapEditor::render` with `animate`, `modulation_source_n`), shared
    with the LFO editor's animation;
  - the rows' `ModulationMeterReadouts`;
  - the wheel over the editor (`lineEditorScrolled`: the paint pattern
    while painting, otherwise the X grid), for both the remap and the LFO
    editors, which lack it too.
- **#201 (Tabs Builder's G4c-3c):** Enter Point Phase and Value, and Import
  LFO and Export LFO.

**Verified.**

- **Layout:** the fixtures now list the remap section's parts
  (`layout_paths.txt`, regenerated from ref-gui, additions only):
  `remap_name`, `paint`, `paint_pattern`, `grid_size_x`, `grid_size_y`,
  `smooth`, the `preset_selector` with its text and arrows, and the editor
  `modulation_source_1`. `tests/layout.rs` matches all of them at every
  size. It fails if the name's width is off by one pixel.
- **Pixels against ref-gui (1400 × 820):** in the remap section, the
  pixels more than 24 off went from about 166,000 (an empty section) to
  663, in both `init_matrix` and `modulations_matrix`. All of them are in
  the header's text row ("MOD REMAP 1", the grid numbers, "Linear"), within
  a pixel of upstream's glyphs. `init_matrix`'s unconnected row 1 shows the
  disabled colours as upstream does.
- **Tests (`tests/controls.rs`):**
  - `the_remap_editor_edits_the_selected_rows_curve`: with row 2 selected,
    a double click in the editor adds a point to slot 2's curve, and only
    slot 2's.
  - `the_remap_strip_loads_lfo_files`: the arrows load Bank Shape, Ramp 2
    and Bank Shape into the curve, leaving LFO 1 alone, and a press on the
    name opens the browser on the user's LFOs.
  - The LFO editor's tests pass unchanged on the shared code.
- **Checks:** `cargo clippy --workspace --all-targets -D warnings`, the
  `sas-gui` and `sas-plugin` tests, `gui_reference.py layout --check` and
  `ledger.py --check` all pass.
