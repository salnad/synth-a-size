## 2026-10-27: GUI G4c-3c, the line editors' point entry and the LFO menu's import and export

Taken over from GUI Builder's G4c plan, on #195's `LineTarget`: the menu
items the line editors still lacked
(`vital/src/interface/editor_components/lfo_editor.cpp`'s `mouseDown` and
`respondToCallback`, `line_editor.cpp`'s base menu and text entry,
`editor_sections/lfo_section.cpp`'s `importLfo` and `exportLfo`).

**Port** (`controls/line_editor.rs`, `sections/lfo.rs`)

- **Menus:** over a point, after the LFO's Set Start Point or the base
  menu's start, Remove Point and Enter Point Phase for an inner point,
  then Enter Point Value, then the divider, in both the LFO menu and the
  base menu (a remap curve's). The LFO menu ends with Import LFO and
  Export LFO after the flips; the base menu has neither.
- **Point entry** (`showTextEntry`, `setSliderPositionFromText`): a
  centred monospace field 50 × 30 at the size ratio, at the point and kept
  inside the editor, takes the keyboard with its text selected, showing
  the point's phase or value in JUCE's default number text. Return,
  Escape or losing the keyboard set the point and hide the field; empty
  text only hides it. Upstream's Escape hides the field, and hiding it
  loses the keyboard, which sets the point (`textEditorFocusLost`), so the
  port sets it on Escape too. A phase stays within its neighbours'
  (`getMinX`, `getMaxX`); a value from 0 to 1 sets the height, and a level
  end typed to a new value takes the other end with it. The shape is
  rendered and written as the editor's other edits are; upstream calls no
  listener here, so a caller-owned shape hears nothing.
- **Shared text editor:** it takes `centred`, `hide_prompt` and
  `surrender_focus` as #194 adds them (applied from #194's diff), and a
  `mono` style (`setMonospace`: Droid Sans Mono, a 0.17 top indent). The
  entry takes the keyboard through `focus_select_all`, which raises the
  window focus request for FL Studio.
- **Import LFO** (`importLfo`): a native open dialog titled "Import LFO" in
  the user's LFOs folder for `.vitallfo` files; the choice loads with the
  LFO extension (`loadFile(choice.withFileExtension(...))`), becoming the
  current file and naming the shape.
- **Export LFO** (`exportLfo`): a native save dialog titled "Export LFO" in
  the same folder, suggesting no name; the shape is written with the LFO
  extension and then named after the file, so the file keeps the earlier
  name, as upstream writes `stateToJson` before `setName`.

**Tests**

- `tests/controls.rs`: the LFO menu's items with Import and Export
  (`the_lfo_menu_flips_the_shape` updated);
  `the_lfo_point_menu_offers_the_entries` over the peak and over the
  start; `a_typed_point_value_or_phase_moves_the_point` (a value, a phase,
  a phase clamped to its neighbours'); `a_typed_end_value_moves_both_ends`;
  `escape_sets_the_typed_point_too`.
- `sections::lfo` `an_export_writes_the_shape_before_renaming_it`.

No audio change.
