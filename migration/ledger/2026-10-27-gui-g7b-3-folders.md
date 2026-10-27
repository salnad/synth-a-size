## 2026-10-27: GUI G7b (3), the folder list, the row menu and the delete overlay

The last of three preset browser PRs, on G7b-2 (#159): the folder list
beside the search, the preset list's right-click menu with rename and
delete, and the shared pieces they need.

**The folder list** (`sections/selection_list.rs`, `SelectionList`)

- Shared with the popup browser (G7d): All and Favorites first
  (`addFavoritesOption`, All selected), then the selections sorted by full
  path in natural order ignoring case. A folder named after the
  passthrough name (Presets) shows its parent's name, so the browser lists
  "Bank" and the data folder's name.
- Rows: the folder icon (filled while open) indented by its depth, or the
  star for files and Favorites (filled for a favourite), and the name in
  the light face at 55% of the row height (`loadBrowserCache`). The
  selected and hovered rows get additive highlights across `browse_area_`
  (two pixels in, three short of the scroll bar); wheel and scroll bar.
- A click on a row selects it; on the selected folder it opens or closes
  it, listing the folders inside (in natural order; upstream takes the file
  system's). A click on a file's star toggles its favourite. A right-click
  offers "Open File Location".
- In the browser: a folder lists its presets and those in the folders
  inside it (`setCurrentFolder`, which later reloads read), All lists every
  preset, Favorites the favourites (`newSelection`, `allSelected`,
  `favoritesSelected`).

**The row menu** (`PresetList::menuClick`)

- "Open File Location", and for a file that can be written "Rename" and
  "Delete". Open File Location is `File::revealToUser`: Finder with the
  file selected on macOS, the folder opened elsewhere.
- Rename puts a text editor over the row's name with the name selected;
  Return, losing the keyboard, the wheel or Escape (which hides it and so
  takes its keyboard, as upstream's does) renames the file, keeping its
  extension and dropping the characters `FileNameFilter` drops, and the
  list reads its presets again.
- Delete opens the delete overlay.

**The delete overlay** (`sections/delete_section.rs`, `DeleteSection`)

- Over the whole interface in the overlay screen colour, a body-coloured
  box with "Are you sure you want to delete this preset?", the preset's
  name, and Cancel and Delete buttons, at upstream's sizes. Delete removes
  the file and the browser reads its presets again (`fileDeleted`); Cancel
  or a click outside the box closes it. It takes every click while open.

**Shared pieces**

- `controls::ui_button`: `setUiButton(primary)`, the rounded UI button with
  its hover fade and light text at 45% of its height; G7c's save section
  uses it too.
- `UpstreamIcon::Folder` (`folder.svg`), and the SVG path reader now takes
  relative commands, H and V, S, and arcs (as `addCentredArc`, from the SVG
  spec's endpoint form), and numbers that run together (`-1.1.9`), which
  that icon has. The reader follows JUCE's `Drawable::parseSVGPath`, so `path.rs`'s
  credit line now names `juce_gui_basics` too (rule 1).
- `text_editor::focus_select_all` and the editor's lost-focus event;
  `preset_files::files_under`; `file_dialog::reveal`.

**Tests** (`tests/preset_selector.rs`)

- `the_folder_list_chooses_what_the_list_shows`: the rows, a bank's and the
  data folder's presets, opening a folder, Favorites and All.
- `the_row_menu_renames_a_preset` and
  `the_row_menu_deletes_a_preset_after_asking` (a click outside cancels),
  on a copy of the corpus presets.
- Side by side with ref-gui, the folder list's rows match pixel for pixel
  (highlight edges included).

No audio change.
