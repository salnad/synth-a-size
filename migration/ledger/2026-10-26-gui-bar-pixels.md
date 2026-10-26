## 2026-10-26: GUI follow-up: bars measured in pixels at every display scale

Review (UI, #124). Upstream's layout units already include the display
scale, and it never sets `BarRenderer::display_scale_`, which stays 1. So
its bar vertex shaders get the component's size in pixels:
`size.x = width_percent·w/2`, `size.y = |y − bottom|·0.5·h/2`, and
`min_width = 4/w`. #124's `BarRenderer` divided by the display scale, so
at 2x the delay's echoes and the chorus's taps drew twice as wide as
upstream's.

**Changed**

- `BarRenderer::quads` takes only the component's bounds (pixels) and
  uses them as upstream does; `paint` no longer passes the pixels per
  point.
- The minimum width is 4 / width for horizontal bars too, as upstream's
  `4.0f / getWidth()` (review, UI): the port took it from the height,
  which their thickness is measured against.

**Tests**

- `controls::bars::bars_scale_with_the_component_at_2x`: a component
  twice the pixels (display scale 2) gives bars exactly twice as wide,
  with twice the shader size. A bar held to the minimum width is the same
  number of pixels at both sizes.
- `controls::bars::horizontal_bars_take_the_minimum_from_the_width`.
- `controls::bars::vertical_bars_follow_the_vertex_shader` is unchanged.

No audio change.
