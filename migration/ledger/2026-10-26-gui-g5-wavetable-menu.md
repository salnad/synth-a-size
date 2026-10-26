## 2026-10-26: GUI G5 (2), the wavetable view's right-click menu

**Changed.** `Wavetable3d` gets upstream's right-click menu (`mouseDown`
with the popup-menu button, `respondToMenuCallback`). The menu opens at
the pointer, with the items in upstream's order:

- **Save to Wavetables**: shown only once the editor can save
  (`Wavetable3d::save_available`), because upstream opens `FullInterface`'s
  save section, which is G7's. Choosing it raises a save request the
  caller takes with `take_save_request()`. Hook contract, for G7: when it
  returns true, save oscillator N's wavetable (`EditorModel::wavetable_json`)
  as a `.vitaltable` in the user's Wavetables folder, as
  `FullInterface::saveWavetable` does (the save section with file type
  "Wavetable", extension `vitaltable`, the folder created if missing).
  Then set `save_available` on the views.
- **Copy**: the oscillator's wavetable JSON onto the clipboard
  (`getWavetableJson(index_).dump()`), through egui's `copy_text`.
- **Paste**: shown only when the clipboard holds wavetable JSON
  (`hasMatchingSystemClipboard`: `WavetableCreator::isValidJson`). It loads
  that JSON into the oscillator. egui reads the clipboard only on a paste
  key, so the menu reads it through copypasta, already in the tree through
  egui-baseview.
- **Initialize**: the default wavetable (`loadDefaultWavetable`:
  `WavetableCreator::init`).

Not here:
- **Text to Wavetable** is absent: it needs sign-in (D20). A local
  alternative is parked as I20.
- **Resynthesize Preset to Wavetable** comes as its own PR.

The menu's choice goes through the overlay (`MenuOwner::Wavetable`,
`Overlay::wavetable_action`), and the view carries it out on its next
frame. A right-click starts no drag and sends the host nothing.

`EditorModel` gains three methods, implemented by the plugin on the edit
bus (`PluginModel`):
- `wavetable_json(osc)`: the creator's `to_json().dump()`.
- `load_wavetable_json(osc, json)`: parses, checks `is_valid_json`, reads
  the creator (a line shape becomes a single line source, as in
  `jsonToState`), and sets it through `EditHandle::set_wavetable`. It
  returns false and changes nothing when the text isn't wavetable JSON or
  can't be read. Upstream leaves the creator cleared and partly loaded when
  valid-looking JSON fails to read, with no re-render; the port leaves the
  oscillator unchanged (D21).
- `load_default_wavetable(osc)`: `WavetableCreator::init` on the
  oscillator's creator, then `set_wavetable`.

All three run on the GUI thread; the audio thread only receives the
prepared table through the edit bus, as for every wavetable edit.

**Verified.**

- sas-plugin, `wavetables_copy_paste_and_initialize`:
  - copying oscillator 1's JSON and pasting it into oscillator 2 gives
    the same JSON and name, and moves the revision;
  - non-JSON, JSON without groups, an oscillator past the last, and JSON
    that passes the validity check but can't be read (`"groups": [7]`)
    change nothing, not even the revision;
  - Initialize gives "Init" on that oscillator only.
- sas-gui, `tests/controls.rs`:
  - the menu lists Copy and Initialize, adds Paste only for wavetable
    JSON on the clipboard, and adds Save only once saving is available;
  - a right-click edits nothing;
  - choosing Paste or Initialize reaches the model with the clipboard's
    text or the oscillator;
  - Copy asks the model for the oscillator's JSON, and the press on the
    menu doesn't reach the view under it;
  - Save raises one save request.
- `cargo clippy --workspace --all-targets -D warnings`, `cargo fmt`, and
  the sas-gui and sas-plugin tests pass.

**Host checks.** None new. The clipboard is the system's, so copying in
one plugin instance and pasting in another works in FL Studio and in
REAPER alike.
