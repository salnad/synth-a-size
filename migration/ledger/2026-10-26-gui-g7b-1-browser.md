## 2026-10-26: GUI G7b (1), the preset browser's frame

The first of three preset browser PRs: the browser opens in place of the
main sections and draws its static parts; the list's rows, search and
favourites come in G7b-2, and the folder list and the row menu in G7b-3.
Built on Harness Builder's #129 (the live-engine `ref-gui` options), whose
harness files this changes too.

**Port** (`sections/preset_browser.rs`, `PresetBrowser` and `PresetList`)

- `PresetBrowserLayout` is `PresetBrowser::resized` and `getInfoRect` /
  `getSearchRect`. `getInfoRect` subtracts the large padding before
  truncating, unlike the rest of `resized`, which is one pixel at some
  sizes. The browser's bounds (`FullLayout::browser`) run from the main
  column to the interface's width, which a letterbox's left offset narrows,
  as upstream's do.
- The background: the info and search panels with their tab shadows and
  borders, the two divider lines, the folder list's background, the text
  editors' backgrounds (`DefaultLookAndFeel::fillTextEditorBackground`),
  and the list's background, title strip, column dividers, star and column
  titles (`PresetList::paintBackground`), with the list's own tab shadow.
- Over it: the preset's name and author, or "Preset name" and "Author" in
  the lightened body colour (`loadPresetInfo`), the search box's prompt in
  its font and indents, and the nine style filters as text buttons. The
  filters are editor-local on/off values (`local::PRESET_STYLE_FILTERS`);
  G7b-2 filters by them.
- Showing it (`FullInterface::setPresetBrowserVisibility`): the browser
  replaces the tab, the modulation column and the voice and portamento
  sections; the header shows the exit button and "PRESET BROWSER"
  (`setTemporaryTab`) instead of the tabs. A left click on the preset name
  toggles it, the menu's new first item "Browse Presets" opens it, and the
  exit button or Escape closes it.
- `EditorModel::preset_author` and `preset_comments`; the plugin reads them
  with the name from one cached copy of the save info.
- `paths::star`, `paths::exit_x` (with `Path::add_star`), and
  `Justify::Right` for the list's "Date" title.
- `user_folders::directories` is `LoadSave::getDirectories`: the kind's
  folder under the data folder and under each folder inside it, where
  imported banks go.

**Harness** (`migration/reference/gui`, `migration/scripts/gui_reference.py`)

- `ref-gui --overlay preset_browser` opens the browser over the tab, and
  `views.json` views may set `"overlay"`; a new `preset_browser` view
  renders it from both.
- `ref-gui` now sets upstream's `DefaultLookAndFeel`, as upstream's
  `SynthEditor` does. Without it, text editors drew in JUCE's default
  colours (a square teal box). Every existing view renders identically with
  it.
- The script runs `ref-gui` with `HOME` and `XDG_DATA_HOME` in an empty
  temporary folder, so no config, favourites or presets on the machine
  change a frame; it keeps `~/.Xauthority` for a desktop display.
  G7b-2 adds a generated fixture data folder there for the list's rows.

**Tests**

- The layout fixtures add the browser, its list, folder list, style
  buttons, name, author, search box and comments, and the header's exit
  button and temporary tab; all six sizes match.
- `tests/preset_selector.rs`:
  `the_name_the_menu_and_the_exit_button_show_and_hide_the_browser` and
  `escape_hides_the_browser`. The menu test now finds "Initialize Preset"
  one row lower.
- `user_folders::directories_include_banks`.
- `test_gui_reference.py`: `OverlayArgsTest`.
- Side by side with ref-gui, the browser matches apart from the folder list's
  rows (G7b-3).

No audio change.
