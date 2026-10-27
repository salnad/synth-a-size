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
- Every rfd dialog is made eagerly, in the frame that asks for it, while the
  editor's window is known to exist: `show_alerts` calls `dialog.show()` and
  `file_dialog::open` calls `pick_file`/`pick_folder`/`save_file` before
  pinning the future, where both used to make the dialog inside a lazy async
  block at the next poll. rfd keeps the parent's handle (on macOS the
  editor's `NSView`), so a host closing the editor between the two frames
  left that poll reaching a freed view. These are the only rfd call sites:
  tuning, Open External Preset, Export Preset and Choose Data Folder all go
  through `file_dialog::open`. Closing the editor (`set_parent(None)`) also
  drops alerts not yet shown, so none is made for a window that is gone.
- Open External Preset skips a choice that doesn't exist, without a message,
  as upstream's `importPreset` does.
- FL Studio checklist item 9 (`decisions/gui-framework.md`): the message
  shows as a sheet, OK dismisses it, and closing the window or removing the
  plugin with it showing neither hangs nor crashes.

**Tests**

- `sas-plugin` `a_preset_that_doesnt_load_gives_upstreams_reason`: a
  missing, a corrupted and a newer preset.
- `tests/preset_selector.rs` `a_preset_that_doesnt_load_says_why_when_picked`:
  a list pick queues the message box; the arrows in an external preset's
  folder don't.
- `file_dialog` `closing_the_editor_drops_queued_alerts`: an alert queued
  before the editor closes is gone after, and a later `show_alerts` starts
  no dialog.

No audio change.
