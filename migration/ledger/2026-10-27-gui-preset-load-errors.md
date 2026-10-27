## 2026-10-27: GUI "Error opening preset" for presets that don't load

The port-wide follow-up from the review of #159: a preset that doesn't load
now says why, as upstream's `AlertWindow::showNativeDialogBox` does, where
the editor ignored the error before.

- `EditorModel::load_preset_file`'s error is upstream's reason
  (`SynthBase::loadFromFile`): "Preset was created with a newer version."
  for a newer preset, "Preset file is corrupted." for anything that doesn't
  read, and nothing for a missing file.
- Where upstream says so, the editor shows a native message box titled
  "Error opening preset" with "There was an error open the preset. " and the
  reason (upstream's wording):
  - a preset picked in the browser's list, or reached by the header's
    arrows moving the list's selection (`SynthPresetSelector::
    newPresetSelected`);
  - Open External Preset (`importPreset`).
  The arrows stepping through an external preset's folder stay silent, as
  upstream's `PresetBrowser::loadFromFile` is.
- `controls::file_dialog::alert` queues the message box; the editor shows
  queued ones after each frame (`show_alerts`, rfd's message dialog
  attached to the editor's window, without waiting), and the dialogs'
  polling drives them.

**Tests**

- `sas-plugin` `a_preset_that_doesnt_load_gives_upstreams_reason`: a
  missing, a corrupted and a newer preset.
- `tests/preset_selector.rs` `a_preset_that_doesnt_load_says_why_when_picked`:
  a list pick queues the message box; the arrows in an external preset's
  folder don't.

No audio change.
