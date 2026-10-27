## 2026-10-27: GUI G7, the saved window size

The editor opens at the size last saved, as upstream's `SynthEditor`
does with `LoadSave::loadWindowSize`, and saves a new size once it holds,
as `BorderBoundsConstrainer::resizeEnd` does.

- **Config** (`plugin/src/config.rs`): `window_size` reads the config's
  `"window_size"`, the editor's width as a share of 1400, at least 0.25
  (`kMinWindowSize`), 1 when unset; `save_window_size` rewrites it,
  keeping every other key.
- **Opening** (`sas_gui::state_for_window_size`): a new plugin instance's
  editor state starts at 1400 × 820 times the saved size, rounded, no
  smaller than 350 × 205, with the drawer closed. The plugin reads the
  size when the instance is made (`SynthPlugin::with_parts`, from the
  config file MIDI learn uses; in-memory plugins open at 1).
- **Saving** (`WindowSizeSaver`, `App::save_window_size`): the editor's
  share of the window, without the chat's drawer (`drawer::split`), over
  1400, once a new size has held for half a second, so a drag's passing
  sizes and the window it opened at aren't saved, and opening the drawer
  saves nothing. Nothing is saved while the drawer's resize awaits the
  host. `EditorModel::save_window_size` writes it.
- **D30**: an instance reopening its window keeps its own size rather
  than one another instance saved since, as nice-plug keeps one editor
  per instance and the size can't be set before it reopens; and there's
  no clamp to the screen, which isn't known before the window opens.
- FL checklist item for the saved size (`decisions/gui-framework.md`).

**Tests**

- `config` `window_size_round_trip`: unset is 1, other keys are kept, and
  a smaller size reads as 0.25.
- `editor` `the_state_opens_at_the_saved_size`,
  `a_size_is_saved_once_it_holds`, and
  `the_editor_saves_its_width_without_the_drawer` (the drawer's widening
  saves nothing; a resize to 2640 × 1230 with the drawer open saves 1.5).
- `sas-plugin` `a_new_instance_opens_at_the_saved_window_size`.

No audio change.

**G7 acceptance pack.** The FL checklist in `decisions/gui-framework.md`
stays the source; a one-sitting test script generated from it, ordered for
a single run (preparation, the editor opening first, the focus-check
bundle last), is in the project store at `docs/fl-acceptance-checklist.md`.
The screenshot gallery it pairs with is CI run 40444408190's `gui-gallery`
artifact at #179's head 0a12760 (the top of the G7 stack): 19 views,
upstream on top and the port below, kept in the store at
`media/gui/g7-gallery/` with upstream's logo masked (never in git).
