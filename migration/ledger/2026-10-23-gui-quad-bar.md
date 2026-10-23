## 2026-10-23: GUI bar quad (BarRenderer's fragment shader)

A small G6 prerequisite, agreed with GUI Builder. The Effects tab's chorus and
delay viewers draw with upstream's `BarRenderer`.

**Changed**

- `primitives`: `QuadKind::Bar` and its WGSL arm, a port of
  `kBarFragmentShader` (`look_and_feel/shaders.cpp`): alpha ×
  min(1, corner × size) towards each edge. `size` is the bar vertex shaders'
  varying, carried in `shader_values[0]` and `[1]` (pixels): the full
  thickness across the bar, and half its length × the display scale along it
  (`BarRenderer::setBarSizes`), so at display scale 1 the fade spans two
  pixels along and one across (UI Reviewer on #108). The BarRenderer port fills
  them. Additive bars use the existing `QuadBatch::additive`. The instance
  layout is unchanged.
- A `gpu-tests` feature builds `sas-gui`'s `tests/gpu.rs` (required-features),
  whose tests render through wgpu and fail without a GPU adapter. CI's GUI
  reference job, which has Mesa's lavapipe, runs
  `cargo test -p sas-gui --features gpu-tests`; the other jobs don't build them.

**Tests**

- `tests/gpu.rs` `bars_fade_over_two_pixels_along_and_one_across` renders a
  horizontal bar 20 × 8 through the real pipeline: full colour inside, about
  0.25 on the end pixels and 0.75 one in, 0.5 on the side pixels, nothing
  outside.
