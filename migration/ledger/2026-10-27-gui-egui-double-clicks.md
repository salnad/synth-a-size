## 2026-10-27: The compressor and the selection lists count clicks as JUCE does

The second follow-up to `controls::clicks` (#176, #190). The compressor
editor and the selection lists (the preset browser's and the popup
browser's) took their double clicks from egui's `double_clicked`, whose
rules differ from JUCE's on every count: under 0.3 s between releases
(JUCE: under 0.4 s between presses), 6 points of movement (JUCE: under 4),
a hold of up to 0.8 s (JUCE: 0.3 s), and no check of buttons or modifiers.

**Port** (`controls/clicks.rs`, `sections/effects/compressor.rs`,
`sections/selection_list.rs`)

- `DoubleClicks`: for a control that only needs `mouseDoubleClick`, every
  press on it (any button, with its modifiers) and its moves, counted with
  JUCE's rules at the release (`Component::internalMouseUp`).
- The compressor's `mouseDoubleClick` (a hovered ratio back to 0) and the
  selection list's (the selected file chosen again, which closes the popup
  browser) use it.
- The compressor's drag now follows the pointer from its first move, as
  JUCE's `mouseDrag` does; egui's drag started only past its 6-point click
  distance, so a short drag on a threshold or ratio moved nothing.

**Tests**

- `tests/effects.rs`: a second press that drags a ratio 5 points moves it
  and doesn't reset it; two clicks 0.35 s apart reset it. Both fail on the
  previous code (the drag reset the ratio without moving it; the clicks
  weren't a double click).
- `tests/preset_selector.rs`: a second press held half a second leaves the
  popup browser open; two clicks 0.35 s apart close it. The second fails on
  the previous code; the first guards JUCE's long press (egui's
  release-to-release limit refused that hold too).

No audio change.
