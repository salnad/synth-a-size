## 2026-10-27: GUI G5e (7a), the wavetable editor's name, arrows and gear

Taken over from Wavetable Builder's G5e plan: the wavetable editor's title
row, part one. The name's popup browser and arrows, and the gear menu over
the amplitudes. The title menu (save, import, export, resynthesis) is
G5e-7b.

- **The creator's last file** (`EditorModel::last_wavetable_file`,
  `set_last_wavetable_file`): upstream's `WavetableEditSection::loadFile`
  remembers each oscillator's last file in its creator
  (`setFileLoaded`), not in the preset. `popup_browser::load_wavetable_file`,
  which the oscillator's browser and arrows and now the editor's load
  through, sets it when a file loads; the plugin keeps it per oscillator
  while the instance lives.
- **The name** (`textMouseDown`): a press opens the popup browser for the
  oscillator's wavetables, 600 × 400 at the size ratio below the name from
  its left, over the editor (`BrowserOwner::WavetableEditor`), with the
  user's and the added folders and the last file selected. A file chosen
  loads as the oscillator's browser loads one. The interface draws the
  popup browser over the editor too.
- **The arrows** (`prevClicked`, `nextClicked`): the wavetable before or
  after the creator's last file among the user's and the added folders'
  (`getShiftedFile`), the first with none.
- **The gear** (`frequency_amplitude_settings_`): upstream's bar editor
  menu, Power Scale and Amplitude Scale, a divider, and Zoom 1x to 16x,
  setting the section's `power_scale` and `zoom` (`barViewerCallback`),
  which the frequency displays and the overlays read.
- **D31**: upstream's factory wavetables aren't there (approval pending).

**Tests** (`tests/wavetable_editor.rs`)

- `the_gear_menu_sets_the_scale_and_the_zoom`: from the power scale at 8x,
  Amplitude Scale, Zoom 2x, Zoom 16x and Power Scale, with nothing written
  to the host.
- `the_editor_arrows_step_from_the_last_file`: next loads the first of the
  user's and the added folders' wavetables, then the following one;
  previous steps back; the last file follows.
- `the_editor_name_opens_the_browser`: the browser lists the added folder
  and every wavetable, and a file chosen loads.

No audio change.
