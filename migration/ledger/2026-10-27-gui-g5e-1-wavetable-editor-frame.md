## 2026-10-27: GUI G5e (1), the wavetable editor's frame

The first of seven wavetable editor PRs (G5e). An oscillator's edit button
now opens its wavetable editor in place of the interface, and the editor
draws the oscillator's wavetable as it is. Editing, the overlays and the
menus come in G5e-2 to G5e-7 (the plan is in the PR description).

**Port** (`sections/wavetable_edit.rs`, `WavetableEditSection`)

- `WavetableEditLayout` is `WavetableEditSection::resized`,
  `getFrameEditBounds`, `getTimelineBounds` and
  `WavetableOrganizer::handleWidth`. `getTimelineBounds` subtracts the
  unrounded padding, which is a pixel at some sizes. The section's bounds
  are the letterbox's left edge and width and the window's top, as
  `FullInterface::resized` gives them.
- The background: the two bodies and their tab shadows, the organizer's
  background, strips and position lines, the playhead's ticks, the preset
  selector's background and the displays' widget backgrounds.
- Over it, the frame under the playhead (`updateTimeDomain`,
  `updateFrequencyDomain`): the waveform (line and fill), the amplitudes
  (power and square scale) and the phases, at zoom 8 with upstream's
  disabled colours and rounded corners. Then the gear, the wavetable's name
  and author with the arrows, the menu and exit buttons, the component
  list's rows (names, menu buttons, the "Add Modifier" and "Add Source" UI
  buttons with their plus icons) and the organizer's rows, keyframe diamonds
  and playhead line.
- Showing it (`FullInterface::showWavetableEditSection`): the editor
  replaces the header and every section, over the interface's background
  and bars. The edit button opens it, and the exit button closes it. Each
  oscillator keeps its own section, so its playhead and selection persist
  while it is hidden.
- Selection: showing selects the first source's first keyframe
  (`visibilityChanged`), and a preset load resets it (`FullInterface::reset`)
  by the model's preset generation.
- `WavetablePlayheadInfo` paints with JUCE's component painting, which
  `FullInterface` turns off for its OpenGL context, so upstream never shows
  its number. The port draws nothing there either.
- `EditorModel::wavetable_creator`: a copy of the document's creator, which
  the plugin reads from the edit bus. The section reads it again when the
  content revision moves.
- `BarRenderer::set_scaled_y` with `power_scale` and `square_scale`,
  `paths::gear` and `paths::plus`, and
  `WavetableComponent::keyframe_positions` made public.

**Harness** (`migration/reference/gui`, `migration/scripts/gui_reference.py`)

- `ref-gui --overlay wavetable_editor` opens oscillator 1's editor, as its
  edit button does. A new `wavetable_editor` view renders it from both
  sides; the gallery opens it with `Interface::show_wavetable_editor`.
- The layout paths add the editor, its organizer, component list (the init
  preset's rows), playhead, exit, preset selector, menu and settings.
  `ref-gui` lays them out while the editor is hidden, so they sit before
  the first `@preset` block.

**Tests**

- Layout fixtures: the editor's 17 paths match at all six sizes.
- `tests/wavetable_editor.rs`: the edit button opens the editor and exit
  closes it, writing nothing to the host; showing selects the first
  keyframe and an edit that keeps it keeps the selection; a preset load
  resets the selection, whether the editor shows or not.
- `wavetable_edit` unit tests: a sine's frequency display (half amplitude
  in bin 1 at minus a half turn; silent bins at `-kPi / 2`), the rows with a
  gap after each group, and the "name  -  author" text.
- Plugin: `the_wavetable_creator_is_the_documents`.
- `test_gui_reference.py`: the wavetable editor overlay.
- Side by side with ref-gui at 1400x820, the frame matches except the
  frame-edit area, which upstream covers with the Wave Source overlay
  (G5e-3).

No audio change.
