## 2026-10-26: GUI shared icons: `controls::path` and `controls::icon`

Split out of the G6a branch ahead of it, for G3b's sample and oscillator
icons.

**Changed**

- `controls::path`: the parts of JUCE's `Path` and `PathStrokeType` that
  upstream's icons use. Sub-paths (`move_to`, `line_to`, `cubic_to`,
  `close`), `add_rectangle`, `add_ellipse`, `add_pie_segment`,
  `add_line_segment`, `add_path`, `stroked` (butt caps, mitred joints),
  `bounds` and `transform_to_fit`. `Path::from_svg` reads upstream's icon
  files the way `Paths::fromSvgData` does (taking the drawable's outline),
  following `<use>` references and reading `M`, `L`, `C` and `Z` path
  data. Curves are flattened.
- `controls::icon`: JUCE's `Graphics::fillPath`. A path is rasterised on
  the CPU with the non-zero winding rule, using exact coverage along each
  scanline and 16 scanlines per pixel. Each size and position is cached as
  a white texture (up to 256 entries) and drawn tinted.
  `fill_path_to_fit` keeps the path's proportions.
- `UpstreamIcon`: the effect icons (chorus, compressor, delay, distortion,
  effects filter, equalizer, flanger, phaser, reverb) and the drag-and-drop
  arrows, from `vital/icons/`. The logo files are never used.

Not yet ported: `Path::addCentredArc` and `PathStrokeType::createStrokeWithArrowheads`,
which `Paths::loop`, `Paths::bounce` and `Paths::shuffle` need. They come
with the first icon that uses them.

**Tests**

`sas-gui` unit tests:

- `controls::path`: `upstream_icons_parse`, `svg_paths_resolve_through_use`,
  `strokes_are_wound_one_way`, `curves_count_their_control_points_in_the_bounds`
  and `zero_length_lines_only_pad_the_bounds`.
- `controls::icon`: `a_square_covers_its_pixels_exactly`,
  `non_zero_winding_fills_overlaps_and_reversed_holes`,
  `stroked_icons_are_filled_outlines` and `every_upstream_icon_has_ink`.

No audio change; nothing draws these yet on main.
