## 2026-10-26: GUI G4a, the macro and wheel source column

**Changed.** The column left of the main area (`sections/extra_mod.rs`,
`ExtraModSection` with `MacroKnobSection`), drawn on every tab as
upstream does:

- Four macro sections (`SingleMacroSection`, macro skin): the knob
  (`macro_control_N`, popup to the right), its name on the label
  background, and its modulation source button below (the button's
  background without text, `paintMacroSourceBackground`), with the
  section's body, knob shadow, border and tab shadow.
- The pitch and mod wheel sources ("PITCH WHL", "MOD WHL"), a vertical
  tab selector with borders.
- `EditorModel::macro_name` (`SynthBase::getMacroName`, "MACRO n" when
  blank): the plugin reads the edit bus's save info.
- Still to come: dragging a source onto a control, rings and meters
  (G4b); the pencil that edits a macro's name (with the icons' follow-up)
  and the editing itself.

**Verified.**

- Layout: the column, both wheel sources, each macro section, knob, label
  and edit button added to the fixtures; the port's rectangles equal
  upstream's at all six sizes.
- Pixels, side by side with ref-gui
  (`media/gui/g4a-macro-column-side-by-side.png`): the column matches
  apart from the pencils.
- `a_macro_knob_drags_its_control`.
- `cargo clippy --workspace --all-targets -D warnings`, `cargo fmt`,
  `gui_reference.py layout --check` and the `sas-gui` tests pass.
