## 2026-10-27: GUI, the random LFOs' viewer animates

**Changed.** The random LFO sections' viewer now scrolls the random value,
as `RandomViewer::render` does (`random_section.cpp`). This is stacked on
#205.

- **While animating.** The viewer animates while its source modulates
  something (`isModSourceEnabled`) and the engine's telemetry is there.
  Otherwise it draws nothing over its background, as upstream draws only
  its corners.
- **Each frame:**
  - both lines move one point to the right;
  - their two newest points take the value now (`random_n`, left and right
    channels) at `(1 − value) · height`;
  - the newest three points are hidden (a boost of −1, with a boost amount
    of 1) where the line lies on the bottom edge, as it does at first, off
    the bottom (`setYAt(i, 10000)`).
- **Once per frame.** The lines move once per egui frame, as upstream moves
  them once per render.
- **Drawing:** the right channel's line, then the left's, each filled from
  the skin's fill centre with the skin's line width, in `WidgetPrimary2`
  and `WidgetSecondary2`, then `WidgetPrimary1` and `WidgetSecondary1`
  (faded by `WidgetFillFade`). The viewer's corners go over them.
- **`kDecayMult` is unused upstream.** The task named it, but the
  constructor computes a decaying boost that it never applies. Nothing is
  ported for it.

**Verified against ref-gui (1400 × 820, `modulations_live` and
`modulations_matrix_live`).**

- **What matches:** after 30 frames the line covers the same width as
  upstream's (half the viewer), so the per-frame scroll and the frame count
  match.
- **What differs:** the fill's height. The port's `random_1` value at that
  moment is higher than upstream's, the same difference as the Random 1
  row's readout bar on the Matrix tab. That is the engine's random LFO
  value, not the viewer. With it, both views are 252 pixels further off
  (more than 24) than with no viewer drawn.
- **Unit test:** `the_viewer_scrolls_and_hides_the_bottom_edge`.
- **Checks:** `cargo clippy --workspace --all-targets -D warnings`, the
  `sas-gui` and `sas-plugin` tests, `gui_reference.py layout --check` and
  `ledger.py --check` all pass.
