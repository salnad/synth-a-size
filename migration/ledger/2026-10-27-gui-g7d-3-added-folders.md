## 2026-10-27: GUI G7d (3), folders added to the popup browser

The fourth G7d PR, stacked on G7d-2b (#174): "Add Folder" and the remove
cross in the popup browser's folder list, for wavetables and samples, and
FL checklist item 11.

**The folder list** (`sections/selection_list.rs`), as upstream's
`SelectionList` with an additional roots name:

- Added folders that exist are listed after the user's folders, in the
  order they were added (`filter` over `selections_` then
  `additional_roots_`), and the list ends with an "Add Folder" row drawn
  with a dashed folder (`kAddFolderName`, `createDashedStroke` with 4 on
  and 2 off at the size ratio).
- A press goes as `leftClick` sends it: on a row whose path doesn't exist
  it does nothing, except on Add Folder, which asks for a folder; on the
  star it stars or selects; on the row-high square left of the scroll bar
  (`x_area_`) it removes an added folder and, on any other row, does
  nothing; elsewhere it selects.
- The hovered added folder shows `Paths::thickX` in that square, brighter
  with the pointer on it (`remove_additional_x_`, Icon Button Off and Off
  Hover).
- Upstream's right-click menu offers "Open File Location" on the Add
  Folder row too, for a file that doesn't exist, as on All and Favorites;
  the port offers it only on real files and folders (D29, covering all
  three rows).

**The popup browser** (`sections/popup_browser.rs`):

- Oscillators and the sample take added folders
  (`kAdditionalWavetableFoldersName`, `kAdditionalSampleFoldersName`);
  LFOs don't, as upstream passes no name for them.
- Opening lists the owner's added folders, and All, Favorites and the
  first listing include their files (`loadPresets`, `allSelected`,
  `favoritesSelected`).
- Add Folder opens a folder dialog titled "Add Folder", made at once like
  the others (#164). The chosen folder is refused with upstream's "Error
  Adding Folder" alerts when it holds more than 8,000 files, its folders
  counted (`isAcceptableRoot`, `kMaxRootFiles`), or is already added or one
  of the user's folders ("Folder already added"); otherwise it is listed
  and saved after the folders already in the config.
- The cross removes the folder from the list and the config.
- `is_acceptable_root` follows linked folders as upstream's walk does,
  but refuses a folder that links back to itself or a folder above it,
  which upstream walks round, hanging when the loop holds no files (D28).

**The arrows.** The wavetable and sample arrows step through the added
folders' files too (`getShiftedFile` with the additional folders name).

**Storage.** `EditorModel::additional_folders` and
`set_additional_folders`; the plugin keeps them in the config under
upstream's keys, `wavetable_folders` and `sample_folders`
(`config::additional_folders`, `save_additional_folders`), which every
instance shares.

**Paths.** `Path::dashed` ports `PathStrokeType::createDashedStroke`'s
walk, quirks included (a dash goes on from one open subpath into the
next, and a segment adds its start, not its end); `paths::thick_x` is
`Paths::thickX`.

**Tests**

- `controls::path` `dashes_follow_juces_walk`.
- `config` `additional_folders_round_trip`: other keys are kept.
- `popup_browser` `a_folder_that_loops_back_is_refused` (and a linked
  folder elsewhere is followed and counted), and
  `folders_are_added_and_removed_as_upstreams_are`: an
  accepted folder is listed before Add Folder and saved; a repeat, a user's
  folder and a folder of 8,001 files (across a subfolder) are refused with
  upstream's alerts; the folder's files are under All and it is listed
  again on reopening; removing it updates the list and the config; the LFO
  browser has no Add Folder row.
- `tests/preset_selector.rs`: `an_added_folder_lists_and_its_cross_removes_it`
  (the cross's square on a user's folder does nothing), and
  `the_wavetable_arrows_step_into_added_folders`. The browser test that
  lists the folder rows now ends with Add Folder.

The dialog itself isn't driven in tests (it is native); FL checklist item
11 covers it, the alerts, and the folder surviving a reopen.

No audio change.
