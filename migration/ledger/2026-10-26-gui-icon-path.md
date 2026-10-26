## 2026-10-26: GUI shared icons: `controls::path` and `controls::icon`

Split out of the G6a branch ahead of it, for G3b's sample and oscillator
icons.

**Changed**

- `controls::path`: the parts of JUCE's `Path` and `PathStrokeType` that
  upstream's icons use. Sub-paths (`move_to`, `line_to`, `cubic_to`,
  `close`), `add_rectangle`, `add_ellipse`, `add_pie_segment`,
  `add_line_segment`, `add_path`, `stroked`, `bounds` and
  `transform_to_fit`. `stroked` takes `JointStyle` and `EndCapStyle` like
  `PathStrokeType`, and panics on the styles not ported yet. Only mitred
  joints with butt ends are ported, which are `PathStrokeType(thickness)`'s
  defaults and the SVG parser's. After `close()`, `line_to` and
  `cubic_to` go on from the closed sub-path's start, as JUCE's flattening
  does. The arc adds nothing without a positive radius. `Path::from_svg` reads upstream's icon
  files the way `Paths::fromSvgData` does (taking the drawable's outline),
  following `<use>` references and reading `M`, `L`, `C` and `Z` path
  data. Curves are flattened. A stroke counts as visible when its width is
  positive and its `stroke-opacity` times the element's `opacity` is
  positive; otherwise the path itself is used.
- `controls::icon`: JUCE's `Graphics::fillPath`. A path is rasterised on
  the CPU with the non-zero winding rule, using exact coverage along each
  scanline and 16 scanlines per pixel. Each size and position is cached as
  a white texture (up to 256 entries) and drawn tinted.
  `fill_path_to_fit` keeps the path's proportions.
- `UpstreamIcon`: the effect icons (chorus, compressor, delay, distortion,
  effects filter, equalizer, flanger, phaser, reverb) and the drag-and-drop
  arrows, from `vital/icons/`. The logo files are never used.

Not yet ported, each coming with the first icon that uses it:

- curved joints and rounded end caps (and bevelled joints and square
  caps). Most of the hand-built paths in `look_and_feel/paths.h` use
  curved joints with rounded caps: the preset selector's previous and next
  arrows, loop, bounce and shuffle.
- `PathStrokeType::createStrokeWithArrowheads`, which loop, bounce and
  shuffle use.
- `Path::addStar`.
- `Path::addCentredArc` as a public call (it exists privately for the pie
  segment).

Review (UI, #116): the stroke style is now explicit, after a caller of
`stroked(thickness)` would have silently got square joints where upstream
draws round ones.

**Tests**

`sas-gui` unit tests:

- `controls::path`: `upstream_icons_parse`, `svg_paths_resolve_through_use`,
  `strokes_are_wound_one_way`, `curves_count_their_control_points_in_the_bounds`
  and `zero_length_lines_only_pad_the_bounds`.
  Review additions: `unported_stroke_styles_refuse`,
  `lines_after_close_go_on_from_the_subpath_start`, `arcs_need_a_radius`
  and `hidden_strokes_leave_the_path`.
- `controls::icon`: `a_square_covers_its_pixels_exactly`,
  `non_zero_winding_fills_overlaps_and_reversed_holes`,
  `stroked_icons_are_filled_outlines` and `every_upstream_icon_has_ink`.

No audio change; nothing draws these yet on main.
