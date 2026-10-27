## 2026-10-27: GUI, slider popups hide on mouseExit; the line editors' wheel shows its popup

**Changed.** This is stacked on G4c-3b (2).

- **When a slider's popup hides.** A slider's value popup now hides on the
  slider's `mouseExit`, as `SynthSlider::mouseExit` calls
  `hidePopupDisplay(true)`, instead of on any frame the slider isn't
  hovered.
  - Each slider remembers whether the pointer was over it or dragging it
    last frame (`isMouseOverOrDragging`). Leaving is the exit, so a drag
    still exits only once it is released, as JUCE sends it.
  - The exit hides whichever popup shows, as upstream's shared popup
    display does (`Overlay::hide_value_popup`). The one exception is a
    popup another slider showed in the same frame, since JUCE sends the old
    slider's exit before the new one's enter.
  - The compressor, equalizer and wavetable views keep their own popup
    handling.
- **The wheel's popup.** The wheel over an LFO's editor or the remap editor
  now shows the moved slider's popup (`SynthSlider::mouseWheelMove`'s
  `showPopup(true)`): the X grid's, or the paint pattern's while painting.
  It stays until a slider is left.
  `Panel::show_value_popup` is the shared `showPopup(true)`.

This closes the "Not yet" that G4c-3b (1)'s ledger left for the wheel's
value popup.

**Verified.**

- **`the_wheels_popup_stays_until_a_slider_is_left`:** the popup shows on
  the wheel and stays while the pointer moves over the editor. It hides
  when the pointer enters and leaves the grid slider, and again after
  another wheel when the pointer enters and leaves the delay knob. The
  test fails with the old rule (hide whenever the slider isn't hovered).
- **Existing tests:** `the_value_popup_follows_the_drag_and_hides_on_exit`
  and the rest of the `sas-gui` tests pass unchanged.
- **Checks:** `cargo clippy --workspace --all-targets -D warnings`, the
  `sas-gui` and `sas-plugin` tests, `gui_reference.py layout --check` and
  `ledger.py --check` all pass.
