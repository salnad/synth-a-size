## 2026-10-26: GUI G7a: user folders, preset files and the header's preset selector

The first G7 PR: where the user's files live, loading and saving presets
from the editor, and the header's preset arrows, menu and save button.

**User folders** (`sas_preset::user_folders`, shared)

- One definition for the editor's browsers and the chat's preset tools:
  the root defaults to `~/Music/synth-a-size`, with `Presets`,
  `Wavetables`, `Samples` and `LFOs` inside it (upstream's folder names;
  the port has no factory folders beside a `User` one).
- The config's `"data_directory"` moves the root, as upstream's
  `LoadSave::getDataDirectory` does, when it names an existing folder.
  The single default root, the "Choose Data Folder..." item and the
  pre-filled export name are D22 in `migration/DIVERGENCES.md`.
- The file extensions (preset, wavetable, LFO, bank) live here too; the
  plugin's `PRESET_EXTENSION` is this one.
- `files_in` sorts as `LoadSave::FileSorterAscending` does (lower-cased
  full paths, natural order), and `step` is `PresetBrowser::jumpToPreset`.
- `sas-llm-agent`'s `default_preset_dir` is now the Presets folder under
  this root (the same path as before).

**Editor model** (`EditorModel`, implemented by the plugin)

- `data_directory` / `set_data_directory` (the config's
  `"data_directory"`, kept beside the other keys), and `user_folder(model,
  kind)` for the folder of a kind.
- `load_preset_file` (`SynthBase::loadFromFile`, through the host's
  `set_state` as the chat's loads go), `save_preset_file` (`saveToFile`:
  writes the preset named after the file, renames the synth's preset and
  makes it the active file), `init_preset` (`loadInitPreset`), and
  `active_preset_file`.

**Header** (`sections::preset_selector`, `SynthPresetSelector`)

- The previous and next arrows step through the active preset's folder,
  wrapping round; with no active preset they start from the user preset
  folder (upstream shifts its browser's list).
- The menu button (on mouse down) and a right-click on the name open the
  menu: Open External Preset, Export Preset, Initialize Preset, Load
  Tuning File, Clear Tuning (when a tuning is loaded) and Choose Data
  Folder. Browse and Save Preset come with the browser and save overlays
  (G7b, G7c), and bank import and export with G7g. Upstream's login and
  skin items are not ported (rule 4, I17).
- The save button is drawn (`Paths::save`); it opens the save overlay in
  G7c. The menu and save icons are `paths::menu(width)` and
  `paths::save(width)`, snapped to the button's pixels as upstream's are.
- `controls::file_dialog` gains folder picks; the tuning dialog's answer is
  now taken every frame, so the header's "Load Tuning File" works on any
  tab.
- Not yet: upstream's `*` before a modified preset's name, which needs the
  synth to track changes since a load.

**Tests**

- `sas_preset::user_folders`: natural order, stepping round a folder, and a
  missing configured root falling back to the default.
- `sas-plugin` `config::the_data_directory_round_trips`.
- `tests/preset_selector.rs`, with a synth that records preset calls:
  `the_arrows_step_through_the_active_presets_folder`,
  `without_an_active_preset_the_arrows_start_from_the_user_folder` and
  `the_menu_initializes_the_preset`.
- Side by side with ref-gui, the header's arrows and its save and menu icons
  match.

No audio change.
