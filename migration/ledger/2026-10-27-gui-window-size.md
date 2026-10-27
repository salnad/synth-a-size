## 2026-10-27: GUI G7, the saved window size

Each editor window opens at the size last saved, as upstream's
`SynthEditor` does with `LoadSave::loadWindowSize` every time its window
opens, and a new size is saved once it holds, as
`BorderBoundsConstrainer::resizeEnd` does.

- **Config** (`plugin/src/config.rs`): `window_size` reads the config's
  `"window_size"`, the editor's width as a share of 1400, at least 0.25
  (`kMinWindowSize`), 1 when unset; `save_window_size` rewrites it,
  keeping every other key. `EditorModel::window_size` and
  `save_window_size` reach it (`None` for a model without a config).
- **Vendored nice-plug-egui** (`port/vendor/nice-plug-egui`, 0.6.0, through
  `[patch.crates-io]`): nice-plug builds a plugin's editor once, and
  nice-plug-egui opens each window at a size only the window could change.
  One patch, in its `PATCHES.md`: `EguiEditorState::set_size`. The
  published crate has no licence file; `LICENSE` is nih-plug's ISC licence,
  which nice-plug forks (nice-plug's own repository couldn't be reached).
  `THIRD_PARTY_NOTICES.md` and `decisions/gui-framework.md` describe it.
- **Opening** (`Editor::reload_window_size`): while no window is open, the
  editor sets the next window's size from the saved one, 1400 × 820 times
  it, rounded, no smaller than 350 × 205 (`window_logical_size`), and wider
  by the chat's drawer when the drawer is open (G7e reopens it docked). It
  runs when the host asks the size before opening a window and when the
  window opens.
- **Saving** (`WindowSizeSaver`, `App::save_window_size`): the editor's
  share of the window, without the drawer (`drawer::split`), over 1400,
  once a new size has held for half a second, so a drag's passing sizes and
  the size a window opened at aren't saved, and opening the drawer saves
  nothing. Nothing is saved while the drawer's resize awaits the host.
- **D30**: upstream keeps the size within the screen; baseview reports no
  screen, so the port doesn't.
- FL checklist item for the saved size (`decisions/gui-framework.md`).

**Tests**

- `config` `window_size_round_trip`: unset is 1, other keys are kept, and
  a smaller size reads as 0.25.
- `editor` `each_window_opens_at_the_saved_size` (wider by the drawer when
  it's open; untouched without a config), `the_state_opens_at_the_saved_size`,
  `a_size_is_saved_once_it_holds`, and
  `the_editor_saves_its_width_without_the_drawer` (the drawer's widening
  saves nothing; a resize to 2640 × 1230 with the drawer open saves 1.5).
- `sas-plugin` `the_window_size_is_kept_in_the_config`.

No audio change.

**G7 acceptance pack.** The FL checklist in `decisions/gui-framework.md`
stays the source; a one-sitting test script generated from it, ordered for
a single run (preparation, the editor opening first, the focus-check
bundle last), is in the project store at `docs/fl-acceptance-checklist.md`.
The screenshot gallery it pairs with is CI run 40444408190's `gui-gallery`
artifact at #179's head 0a12760 (the top of the G7 stack): 19 views,
upstream on top and the port below, kept in the store at
`media/gui/g7-gallery/` with upstream's logo masked (never in git).
