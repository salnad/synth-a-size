## 2026-10-26: GUI, unison bars sized in interface pixels

**Changed.** `Quad::vertical_bar` no longer multiplies the bar's half
length (`size.y`) by the display scale. Upstream sizes `BarRenderer`'s bars
in interface pixels (its `display_scale_` is always 1): `size.x =
width_percent · w / 2`, `size.y = |top − bottom| · 0.5 · h / 2`, and the
minimum width is `4 / w`. The port's width and minimum width were already
right; at 2x its end fades were twice upstream's. The oscillator's unison
viewer draws through a pure `unison_bars`, so it can be tested without a
panel. Found by UI Reviewer after #118.

**Verified.**

- `unison_bars_size_in_interface_pixels_at_2x`: for a viewer twice as
  many pixels, the shader's half length doubles with the height (28, not
  the old 56 at display scale 2) and the minimum width stays 4 / width.
- `cargo clippy --workspace --all-targets -D warnings` (and `sas-gui` with
  `gpu-tests`), `cargo fmt` and the `sas-gui` tests pass.
