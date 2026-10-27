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
- **The corner resizer** (`resizable_corner.rs`): upstream's hosted
  editor shows JUCE's `ResizableCornerComponent` (`SynthEditor`'s
  `setResizable(true, true)`), 18 × 18 at its bottom right, always on top,
  drawn by `LookAndFeel_V2::drawCornerResizer` (four light and dark grey
  diagonals) and hit only on and below a line a quarter above its diagonal
  (`hitTest`). The port draws it at the bottom right of the editor's part of
  the window. A drag asks the host for the editor's original size plus the
  drag, constrained as `ComponentBoundsConstrainer::checkBounds` constrains
  a bottom-right stretch (the minimum size, then upstream's ratio on the
  side the old ratio picks), with the drawer's share when it's docked.
  Letting go saves the editor's width over 1400 as the window has it, as
  `resizeEnd` reads the editor's width: a host that refused the last size
  (a CLAP host puts the view back at once) left the window as it was, and
  that is saved, not the size asked for. When the last size went out with
  the release, the save waits a frame for the host's answer (UI Reviewer,
  #181). A press without a drag saves too, as upstream's `mouseUp` does.
  Sizes round as JUCE's `roundToInt` does, halves to even.
- **Saving** happens only there: sizes the host sets (VST3's `onSize`, a
  docker's 1900 × 400) are never saved, as upstream's aren't (UI Reviewer,
  #181, against an earlier save of every size that held).
- **D30**: upstream keeps the size within the screen; baseview reports no
  screen, so the port doesn't.
- FL checklist item for the saved size (`decisions/gui-framework.md`).

**Tests**

- `config` `window_size_round_trip`: unset is 1, other keys are kept, and
  a smaller size reads as 0.25.
- `editor` `each_window_opens_at_the_saved_size` (wider by the drawer when
  it's open; untouched without a config), `the_state_opens_at_the_saved_size`,
  and `the_corner_saves_the_size_and_the_host_doesnt` (frames in a 1900 ×
  400 docker save nothing; a drag of the corner by 140 × 82 asks for
  1540 × 902, and with the window following, 1.1 is saved on release).
- `editor` `a_refused_corner_size_isnt_saved`: the window doesn't follow
  the drag, and its own 1.0 is saved; with the last move in the release's
  frame, the save comes a frame later.
- `resizable_corner` `a_corner_drag_keeps_the_ratio_and_the_minimum` (with
  heights of exactly a half rounding to even) and
  `the_corner_takes_presses_below_its_diagonal`.
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
