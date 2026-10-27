## 2026-10-27: The shape buttons, envelope editor and LFO editor count clicks as JUCE does

A follow-up to #176, which added `controls::clicks` (JUCE's
`getNumberOfMultipleClicks`, `isLongPressOrDrag` and
`canBePartOfMultipleClickWith`) for the sliders and the wavetable organizer.
The three other controls with double clicks had their own checks, which
counted presses close in time and place only.

**Port** (`controls/shape_button.rs`, `envelope_editor.rs`, `line_editor.rs`)

- Each records every press with its button and modifiers, tracks how far it
  moves, and takes the count at the release, as `Component::internalMouseUp`
  does. A second press that moved 4 points or more, or was held over 0.3 s,
  is one click; presses chain only with the same buttons and modifiers.
- The shape buttons (the envelope's magnifying glass is the one with a
  double click: `DragMagnifyingGlass::mouseDoubleClick` fits the envelope),
  the envelope editor's power dots (`mouseDoubleClick` straightens the
  power) and the LFO editor (a double click adds or removes a point). All
  three record a right-button press too, which ends a run of clicks.
- The constants' re-export from `slider.rs` goes; `controls::clicks` holds
  them.

**Tests** (`tests/controls.rs`)

- `a_dragged_or_held_second_press_doesnt_straighten_a_power` replaces
  `a_double_click_straightens_after_the_second_presss_drag`, which asserted
  a straightening after a second press that dragged 10 points. JUCE counts
  that press as one click, so upstream never sends `mouseDoubleClick` there:
  the drag bends the power in one gesture and nothing straightens it. A held
  second press doesn't straighten it either. `a_double_click_straightens_a_power`
  still covers the real double click.
- `the_magnifying_glass_resets_only_on_a_real_double_click`: a quick second
  click fits the envelope; a held one, one that drags sideways, or one after
  a right press leaves the zoom as it was.
- `a_dragged_or_held_second_press_adds_no_lfo_point`: a dragged or held
  second press on empty space sets no shape; a quick one adds the point.
- All three fail on the previous checks.

No audio change.
