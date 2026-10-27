## 2026-10-27: ref-gui selects a wavetable editor keyframe

Groundwork for the wavetable editor's component overlays (G5e-4 and
G5e-5): their layout fixtures and gallery views need a keyframe of a
chosen component selected, which upstream's editor only does on a click.

**Reference** (`migration/reference/gui/src/gui_main.cpp`)

- `--wavetable-select group:component[:keyframe]` (zero-based, keyframe 0
  by default), with `--overlay wavetable_editor`, selects that keyframe of
  oscillator 1's wavetable in the editor's organizer through
  `WavetableOrganizer::selectFrame`, as a click on it does: the keyframe is
  selected and the edit section shows its component's overlay.
  `selectFrame` is private; an explicit template instantiation, which may
  name private members, hands its address to the tool, so `/vital/` is
  unchanged. Without the overlay, with a malformed value, or for a group,
  component or keyframe that doesn't exist, the tool fails with a message.

**Script and corpus**

- `gui_reference.py`: a view's `"wavetable_select"` passes the option
  (it needs `"overlay": "wavetable_editor"`). In `layout_paths.txt`,
  `@wavetable-select group:component[:keyframe]` lays the following paths
  out with that keyframe selected, until `@wavetable-select none`; their
  fixture entries name it (`"wavetable_select"`), as they name a preset
  and an overlay. No fixture uses it yet: `layout --check` passes
  unchanged.
- `generate.py`: `wavetable_modifiers.vital`, oscillator 1's wavetable as
  one group of a line source and every modifier with an overlay (Phase
  Shift, Wave Window, Frequency Filter, Slew Limiter, Wave Folder, Wave
  Warp), each set away from its defaults, at components 1 to 6.
- `views.json`: `wavetable_editor_select`, the groups preset with group 1's
  second keyframe selected.

**Port**

- `Interface::select_wavetable_keyframe` and
  `WavetableEditSection::request_selection`: the keyframe is selected
  (`select_frame`, so the overlays hear of it) the next time the section
  shows and has read the creator, in place of the default first keyframe;
  one that doesn't exist leaves the default.
- The gallery applies a view's `"wavetable_select"`. The layout test reads
  entries with `"wavetable_select"` as it reads the rest; the overlays that
  add such fixtures route them.

**Tests**

- `tests/wavetable_editor.rs`
  `a_requested_keyframe_is_selected_when_the_editor_shows`: a second
  keyframe asked for before showing is selected and heard by the
  overlays, with nothing committed; a missing one leaves the first.
- By hand: `ref-gui --layout` with the modifiers preset and
  `--wavetable-select 0:3` lists `oscillator 1/FREQUENCY FILTER` visible
  and the Wave Source overlay hidden; the error cases fail with their
  messages; a temporary `@wavetable-select 0:3` path regenerated with its
  entry naming the selection.

No audio change.
