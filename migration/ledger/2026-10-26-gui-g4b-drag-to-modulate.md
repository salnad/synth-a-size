## 2026-10-26: GUI G4b-1, dragging a modulation source onto a control

**Changed.** This ports the drag-to-modulate part of upstream's
`ModulationManager` and `ModulationButton`'s mouse, in
`port/crates/gui/src/modulation.rs`. It builds on #149's
`EditHandle::set_connection_values`.

- **`EditorModel`:**
  - `modulations()` gives the connected slots, as `Route`s. The plugin
    reads them from the document mirror, cached by edit revision.
  - `connect_modulation(source, destination, SlotValues)`,
    `set_connection_values(slot, values)` and `disconnect_modulation` go
    through the edit bus.
- **Source buttons.**
  - The modulation column's tabs, the macro source buttons and the pitch
    and mod wheels report their mouse to the manager.
  - A press selects the source, which gets upstream's outline
    (`drawCurrentModulator`). A click still switches the tab
    (`modulationClicked`). Dragging out of the button starts the map
    (`startModulationMap`), which needs a free slot (`hasFreeConnection`).
  - A right-click on a connected source opens "Disconnect from …" and
    "Disconnect all". Upstream lists the connections in the order they were
    made, the port in slot order.
  - The column's tabs now show one past the last connected source, as
    upstream's do; the stub said nothing was connected.
- **Destinations.** `Panel::slider` registers each modulatable slider of a
  section that is on, with its section's clip.
  - While a source is dragged, those not yet connected to it light up in
    `kLightenScreen`, fading in and out by 0.2 a frame.
  - Knobs show a circle the size of their modulation meter
    (`getFillBounds`); text and curve sliders their whole bounds; linear
    sliders a band.
  - A ring follows the pointer until it is over a destination
    (`drawDraggingModulation`).
- **Connecting.**
  - Over a destination, the source connects with upstream's amount
    (`SlotValues::dragged_to`). Moving off disconnects it. A modifier key
    makes it bipolar, which reaches the engine as new slot values, not a
    reconnection. The connection left at the release stays.
  - **Host writes (D18, deferred).** Upstream tells the host nothing here.
    The port writes the surviving connection's slot parameters at the
    release, one gesture each (begin, set, end), with its final values.
  - **Gesture count.** A typical drag that creates a connection produces
    five parameter gestures, all at the release: power, amount, bipolar,
    stereo and bypass. A drag that ends off every destination, or on a
    control the source already modulates, produces none. Passing over
    other controls on the way adds none.
  - **Why deferring is safe** (Voice Reviewer's conditions):
    - (a) The plugin forwards only host parameters that changed
      (`params.changes`), so the stale host values can't overwrite what the
      bus set mid-drag.
    - (b) Saved controls come from the host parameters (`write_controls`),
      which agree with the mirror and the engine for every connected slot
      once the release writes them.
  - **Closing the editor mid-drag** commits the pending connection to the
    host from the window's close hook (`Interface::pending_connection`).

**Verified.**

- `a_drag_over_one_knob_onto_another_writes_only_the_last_to_the_host`:
  - Pressing the LFO 1 tab and passing over macro 1 connects it (amount 1,
    unipolar), and leaving disconnects it, with no host call.
  - Releasing on macro 2 connects it and then makes exactly five gestures
    for slot 1, which ends at amount 1, unipolar.
- `a_modifier_mid_hover_makes_the_connection_bipolar`: Shift while over
  macro 1 sets the slot's values to amount 0.25, bipolar, in the engine
  with no host call. The release makes the same five gestures with those
  values.
- `a_drag_released_off_the_controls_leaves_nothing`,
  `no_drag_starts_without_a_free_slot` and
  `a_connected_sources_menu_disconnects_it`: the menu items, and that a
  choice disconnects without a host call.
- A render of the port mid-drag (image in the PR) shows the lit
  destinations, the ring and the selected tab's outline. ref-gui can't
  render a drag in progress.
- `cargo clippy --workspace --all-targets -D warnings`, `cargo fmt` and the
  `sas-gui` tests pass.

**Not yet.** These come with G4b-2 and G4b-3: the amount knobs, including a
drag onto one (`modulationDraggedToHoverSlider`), the drag-and-drop icon on
a hovered source, the source meters, and extra modulation targets (the
wave frame slider's view).
