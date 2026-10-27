## 2026-10-27: GUI G5e (2), the wavetable editor's timeline edits

The second wavetable editor PR. The timeline under the displays now edits
the oscillator's wavetable: the playhead moves the wave frame, the organizer
edits keyframes, and the component list edits components and groups. Each
edit commits the creator to the model when it is done, which renders the
table, as upstream's `render()` does.

**Port**

- `sas_wavetable::creator::edits`: the component list's and organizer's
  edits as plain creator edits (`addSource`, `addComponent`, `removeGroup`,
  `removeComponent`, `resetComponent`, the group and modifier moves,
  `createKeyframeAtPosition`, `deleteKeyframe`, and a keyframe drag's
  `mouseDrag` and `mouseUp`), with the guards the menus apply.
  `WavetableComponent` gets `reset`, `remove_keyframe`,
  `keyframe_position`, `set_keyframe_position` and `reposition_keyframe`.
  `reset` clears the keyframes and inserts one at position 0, leaving the
  component's settings, as upstream's does.
- `sections/wavetable_edit.rs`:
  - The playhead (`WavetablePlayhead::mouseDown` / `mouseDrag`) sets the
    oscillator's wave frame, as `setWaveFrameSlider`'s slider does: only
    when the value changes, in one host gesture for the drag (D18). A
    keyframe drag moves the playhead with it (`frameDragged`), in the same
    way.
  - The organizer: a press on a keyframe selects it (unless it is
    selected) and a drag moves the selection, each keyframe clamped to the
    table; the release puts them in order (`reposition`) and commits. A
    drag from empty space draws the selection box and selects the
    keyframes in it; a click there deselects. A double-click, which JUCE
    sends after the second click's `mouseUp`, deletes the keyframe under
    the pointer or creates one there and selects it. The popup-menu button
    opens "Create Keyframe", or "Remove Keyframe(s)" over a selection.
    Pointer positions round, as JUCE's `MouseEvent::x` and `y` do, except
    the press position, which truncates as `mouse_down_position_` does.
  - The component list: "Add Source" and each group's "Add Modifier" open
    the type menus, and each row's menu button opens the group menu (Move
    Group Up and Down, Reset Source, Remove Group) or the modifier menu
    (Move Up and Down, Reset, Remove). The add buttons are UI buttons with
    hover and press colours. The list scrolls with its scroll bar and with
    the wheel over the list or the organizer
    (`WavetableOrganizer::mouseWheelMove`), and the organizer's rows scroll
    with it. The add buttons use `controls::ui_button::interact`, the
    shared UI button's mouse and colours, through the list's clip.
  - The selection follows the edits, as upstream's keyframe pointers do:
    a new source or a reset component selects its keyframe, removals drop
    their keyframes, and moves carry the selection with their groups and
    components.
  - After a commit, the next frame reads the rendered creator back. Upstream
    edits the creator it last rendered, and a keyframe created with none
    left (a reset) copies the frame rendered last.
- `EditorModel::set_wavetable_creator`, which the plugin sends through the
  edit bus (`set_wavetable`).
- `controls::clicks`: JUCE's multiple-click rules
  (`MouseInputSource::getNumberOfMultipleClicks`, `isLongPressOrDrag`), a
  `juce_gui_basics` GPL-3.0-only translation. A press that moved 4 points
  or was held over 0.3 s is one click, and presses chain only with the same
  buttons and modifiers, within the double-click timeout (twice it for the
  third). The sliders now count clicks with it too, so their double-click
  also needs the same modifiers, as JUCE's does; the organizer counts with
  it at the release. `ui_button::interact` splits the shared UI button's
  mouse and colours from its drawing, for callers that draw it themselves.
- `ScrollBar::quad`, so the list draws its scroll bar on top of its own
  rows. `paint` is unchanged.

**Parity** (`migration/reference/src/modules/wavetable/edit.cpp`, module
`wavetable_edit`)

- Upstream's creator takes the same edit sequences as the editor makes
  them, then `render()`. It emits the saved JSON and the render arrays of
  `wavetable_render` (`render_arrays.h`, a copy so `render.cpp` keeps its
  hashes). There are eight cases: adds, moves and removes, resets, keyframe
  creation and deletion, a drag across keyframes and past the end, a drag
  past the start, and edits on new components.
- `params.seed` sets the generator counter before the edits, since a new
  Audio File Source draws its random seed from it; the port test gives its
  thread the same counter.
- The port test (`render_parity.rs`'s `wavetable_edit`) renders the starting
  creator first, as upstream's `jsonToState` and `init` do, then applies the
  edits through `creator::edits`. The saved JSON matches exactly, except in
  the four cases with computed keyframe waves: the init saw, and a keyframe
  interpolated between two. Those compare `wave_data` within the JSON bound,
  as `creator_json`'s preset cases do. The render arrays use the default
  module tolerances. The drag fixtures interpolate in time, because the
  frequency interpolation of unlike spectra is the ill-conditioned content
  the render corpus keeps off the default tolerances.

**Harness**

- A `wavetable_groups` GUI preset (`generate.py`): oscillator 1's table in
  three groups of line sources and modifiers, so the component list
  scrolls. The `wavetable_editor_groups` view renders it from both sides,
  and the layout paths add its rows and the later groups' add buttons. The
  layout test reads each preset's rows from its creator.

**Tests**

- `tests/wavetable_editor.rs`:
  - a playhead drag is one wave frame gesture, and a click that doesn't
    change the value sends nothing;
  - a keyframe drag moves the keyframe and commits once on release;
  - double-clicks create and delete keyframes;
  - a click then a quick drag of the keyframe moves it without deleting
    it, as do a long second press and one with other modifiers;
  - the organizer's menu creates and removes keyframes;
  - a selection box selects the keyframes in it;
  - the component list adds a source and a modifier, moves a group (the
    selection follows) and removes a modifier.
- `controls::clicks`'s unit tests: chains in time and place, drags and long
  presses, and buttons and modifiers.
- Layout fixtures: the editor's paths for both presets match at all six
  sizes.

The display's frame doesn't change during a keyframe drag, as upstream's
doesn't (it renders on release). Drawing in the displays comes with the
overlays (G5e-3), with the edit bus's one-frame commit for the audible drag
(its own PR).
