## 2026-10-23: GUI bar quad (BarRenderer's fragment shader)

A small G6 prerequisite, agreed with GUI Builder. The Effects tab's chorus and
delay viewers draw with upstream's `BarRenderer`.

**Changed**

- `primitives`: `QuadKind::Bar` and its WGSL arm, a port of
  `kBarFragmentShader` (`look_and_feel/shaders.cpp`). The colour fades out over
  the last pixel inside each edge: alpha × min(1, distance in pixels to the
  nearest edge). Additive bars use the existing `QuadBatch::additive`. The
  instance layout is unchanged.
- CI's GUI reference job, which has Mesa's lavapipe, runs `sas-gui`'s ignored
  tests (`cargo test -p sas-gui -- --ignored`): those render through wgpu and
  need a GPU adapter, which the other Linux jobs don't have.

**Tests**

- `primitives::tests::bars_fade_over_their_last_pixel` renders a bar through
  the real pipeline: full colour inside, half alpha on the edge and corner
  pixels (their centres are half a pixel inside), nothing outside.
