## 2026-10-27: GUI G7d (1), the popup browser for an oscillator's wavetables

The first G7d PR: the popup browser (`PopupBrowser`,
`vital/src/interface/editor_sections/popup_browser.cpp`), opened from an
oscillator's wavetable name, and that name's arrows. Samples, LFOs and Save
LFO come in G7d-2; the added folders ("Add Folder") in G7d-3.

**The browser** (`port/crates/gui/src/sections/popup_browser.rs`)

- `PopupBrowserLayout` is `PopupBrowser::resized`: the folder list and the
  file list side by side under a top strip, their dividers, the search box
  and the exit button. The browser is drawn with its skin section's body,
  border and dividers.
- `open` is `FullInterface::popupBrowser`.
  - The browser's own `visibilityChanged` lists the files of the folder
    selected last time.
  - `loadPresets` then sets the folders: the kind's folder under the user
    root and under each bank. For All or Favorites, it lists every file of
    the owner's types under them. Like upstream, a remembered Favorites
    selection lists every file until it's clicked again.
  - The owner's current file is selected.
- The lists are G7b-3's `SelectionList`:
  - a folder lists its files (`newSelection`);
  - All lists every file and Favorites the favourite ones (`allSelected`,
    `favoritesSelected`);
  - a file loads into the owner (`owner_->loadFile`);
  - a file's star sets its favourite.
- The search keeps the files whose names hold every word, ignoring case
  (`filterPresets`), and Escape in it clears it. Otherwise Escape closes
  the browser, and the arrow keys select the previous or next file,
  loading it (`keyPressed`).
- Double-clicking the selected file closes the browser
  (`doubleClickedSelected`), as does the exit button.
- The closing areas surround the owner's section, under the browser. A
  press there outside the browser closes it and reaches nothing else.
  Presses in the owner's section reach the owner, so the oscillator's
  arrows work with the browser open (`PopupClosingArea`).
- **D26:** the search box always shows, where upstream shows "Download
  content" until its factory content is installed, and there is no store
  button.

**The oscillator**

- A press on the wavetable's name opens the browser, 600 × 400 at the size
  ratio, from the unison voices' left and level with the name
  (`textMouseDown`).
- The arrows load the wavetable before or after the current file among
  every `.vitaltable`, `.wav` and `.flac` under the user's wavetable
  folders, sorted by full path and wrapping round, or the first with no
  current file (`prevClicked`, `nextClicked`, `getShiftedFile`).
- `load_file` (`loadFile`): a `.vitaltable` loads with its JSON's name set
  to the file's (`jsonToState`, then `setName`), and other files load as
  audio, spliced (`loadAudioAsWavetable`, `kWavetableSplice`). The file
  becomes the current file (`current_file_`).

**Shared pieces**

- `SelectionList` gains:
  - a plain list without All and Favorites, as the file list;
  - search filtering (`filter`);
  - `set_selected`, `select_next` and `select_previous`;
  - `reset_scroll_position`;
  - a double-click event.
- `SelectionList` senses drags as well as clicks, so a drag on a row
  reaches nothing beneath; the popup browser's lists cover their owner's
  section, and a drag there moved oscillator 1's morph knobs (UI Reviewer,
  #172).
- A `.vitaltable` that doesn't parse leaves the oscillator unchanged; D21
  now covers loading from a file as well as pasting (UI Reviewer, #172).
- The corpus guard's empty-folder check has its closing quote, and the step
  runs clean under `bash -e` and fails with an empty folder planted
  (Harness Builder, #172).
- The chat's toggle hides while the browser is open
  (`Interface::logo_shown`, from #177): its closing areas cover our logo,
  and a click there closes the browser, as upstream's does, instead of
  toggling the chat (UI Reviewer).
- `user_folders::files_of_type` (`getAllFilesOfTypeInDirectories`: every
  file of the given types under the folders, recursively, matched ignoring
  case as on macOS) and `shifted_file` (`getShiftedFile`).

**Harness**

- `ref-gui --overlay popup_browser` opens the browser for oscillator 1's
  wavetables, with `textMouseDown`'s bounds and `FullInterface::
  popupBrowser`, which only reads the data folder. `views.json` has a
  `popup_browser` view, and the gallery opens it through
  `Interface::request_wavetable_browser`.
- `layout_paths.txt` takes an `@overlay <name>` line (and `@overlay none`):
  the paths after it are laid out with that overlay open, and their
  fixture entries name it. The popup browser's parts are checked that way
  at every size, its search box against the download button's bounds.
- `migration/corpus/generate.py` adds four authored wavetables to the
  corpus data folder: two in Wavetables, one in a folder inside it and one
  in the bank.
- The GUI reference job checks that rendering the views and running the
  gpu tests wrote nothing into `migration/corpus`, empty folders included,
  which git doesn't show (Harness Builder's suggestion on #165, and its
  empty-folder check on #172).

**Tests**

- `popup_browser` unit tests: the layout and the closing areas.
- `user_folders` `files_of_a_type_step_round_every_folder`.
- `tests/preset_selector.rs`:
  - the name opens the browser with the folders and every wavetable, and a
    file loads, `.vitaltable` and audio;
  - the arrows step through the user's wavetables, wrapping;
  - a folder lists its files, the search filters, and a double click
    closes;
  - Escape and a press outside close the browser, and the oscillator's
    arrows work with it open;
  - `with_the_popup_browser_open_a_click_at_the_logo_closes_it_not_the_drawer`
    (in `tests/controls.rs`; it fails with the browser left out of
    `logo_shown`);
  - a drag on a list row over the oscillator reaches none of its controls
    (it fails with click-only sensing).
- `tests/layout.rs`: the popup browser's parts at every size.
- `test_gui_reference.py`: `@overlay` lines, and the `popup_browser`
  overlay.

No audio change.
