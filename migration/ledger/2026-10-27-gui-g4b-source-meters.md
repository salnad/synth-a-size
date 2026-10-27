## 2026-10-27: GUI G4b-3b, the source meters and the editing amount overlay

**Changed.** Two more parts of upstream's `ModulationManager`.

- **Source meters** (`renderSourceMeters`, `updateSmoothModValues`). A bar
  on each drawn source button's meter strip (`ModulationButton::getMeterBounds`)
  runs between the source's value and a smoothed copy of it, widened by
  `kModSourceMinRadius`.
  - Two passes, right then left, in `kWidgetPrimary2` then
    `kWidgetPrimary1`. A colour brighter than 0.5 blends additively.
  - The copy snaps to the value when the source's readout comes back from
    the clear value. It then follows by `kModSmoothDecay · seconds · 60` a
    frame, using the editor's clock.
  - No bar draws for a clear value, or for a source that is neither
    selected nor connected.
  - The bar is `BarRenderer`'s: `positionBar` with zero bar width, widened
    by the minimum `width_percent` (4 / width) on the right, with its
    shader sizes.
  - Readouts come from the telemetry snapshot's status outputs. Without an
    engine nothing draws.
  - As upstream, every source's readout is read, snapped and smoothed
    each frame, whether or not its button is drawn, so a button that
    appears after a tab switch starts from the caught-up copy.
- **Editing amount overlay** (`showModulationAmountOverlay`,
  `setAmountQuadVertices`, `setModulationAmountQuad`). The range a
  connection sweeps, drawn on its destination's meter in `kModulationMeter`.
  - **Shape.** On a knob it is an arc 2 pixels thick, 2 pixels beyond the
    meter, kept within ±`kRotaryAngle`. On a vertical bar or text slider
    it is a one pixel line up the meter's left edge, and on a horizontal
    bar one along its top.
  - **When.** While an amount knob is hovered or dragged (`hoverStarted`,
    `sliderValueChanged`). The knobs draw after it, so it follows them from
    the next frame. It also shows while a drag-to-modulate holds a
    temporary connection (`modulationDraggedToComponent`,
    `setTemporaryModulationBipolar`), with the values the drag gave it,
    which the host hasn't heard yet (D18).
  - **Where.** Only on a destination drawn this frame.
  - `modulation_meter::place` now gives a control's meter placement, which
    `Destination` keeps for this.

**Verified.**

- **Gallery.** Without an engine, every view is pixel-identical to `main`.
  With #158's live engine merged locally, the `modulations` views are
  unchanged on both sides: they render without notes, so the per-voice
  sources' readouts are clear and neither side draws a source meter.
- **With notes held.** I added a local view, not committed: the
  `modulations` preset with `held_pad.mid` for 1.3 s. Upstream and the port
  draw the source meters on Env 1, Env 2 and LFO 1 at exactly the same
  pixels. The only other differences in that strip are the RANDOM labels'
  one-pixel text offset.
- **Tests:**
  - `hovering_an_amount_knob_shows_its_range_on_the_destination`: Macro 1's
    arc for a bipolar quarter, kept within the arc; gone once the pointer
    leaves.
  - `a_drag_shows_its_temporary_connections_range`: the overlay uses the
    drag's amount while the host's is still 0; gone after release.
  - `the_editing_overlay_draws_the_amounts_range_on_the_meter`: the knob,
    bipolar, text and linear shapes and values.
  - `source_meters_run_between_the_value_and_its_smoothed_copy`: the
    snapped first frame, lane order and colours, no bar for an unconnected
    source, the smoothing step and the span it draws, and a clear value
    hiding it.
  - `a_source_not_drawn_is_still_smoothed`: a hidden LFO 1 snaps, then
    follows its value for three frames; when its button shows, the bar
    spans the value and the caught-up copy.
- **Checks.** `cargo clippy --workspace --all-targets -D warnings` passes,
  as do the `sas-gui` and `sas-plugin` tests, `gui_reference.py layout
  --check` and `ledger.py --check`.
