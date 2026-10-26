## 2026-10-26: GUI, text too wide for its box: curtailed, ellipsed or fitted as JUCE does

**Changed.** Before this, the port drew every line of text with egui's
`painter.text`, which lets a line too wide for its box overflow it. JUCE
never does. `Graphics::drawText` keeps the glyphs up to the first that
passes the box by more than a pixel (`addCurtailedLineOfText`). It also
ends the line with full stops unless `useEllipsesIfTooBig` is false (it
defaults to true). `Graphics::drawFittedText`, which every
`PlainTextComponent` uses, squeezes the trimmed line horizontally, by as
little as 0.7 of its width (`addFittedText`, `fitLineIntoSpace`), and only
then ends it with full stops.

- `text::LineFit` ports that layout from the glyphs' right edges:
  `Overflow::Curtail`, `Ellipsis` and `Fit`, with `insertEllipsis` and the
  minimum scale. It also gives `justifyGlyphs`' extent, which leaves out
  whitespace when centring.
- `PixelPainter::text_line(text, rect, style, justify, overflow)` draws a
  line through it. A squeezed line is its galley scaled horizontally from
  its left (`stretchRangeOfGlyphs`). The edges are where each next glyph
  starts and, for the last, the line's width. egui snaps each glyph's
  position to a pixel but not the line's width, so a line that fits sits
  exactly where it did before.
- `PixelPainter::text_centred` and `text_left` are `drawText(..., false)`,
  which slider text, section labels and headings, the LFO's "-" and the
  sample's "PITCH" use upstream. `Layer::text_centred` curtails, and the
  new `Layer::fitted_text` fits. The new `Panel::curtailed_text` draws the
  macro names, which `MacroLabel::paint` draws with `drawText(..., false)`.
- The call sites whose upstream mode isn't plain curtailing:
  - **Ellipsis** (`drawText`'s default): the tab selector, the modulation
    buttons' names, the effect tiles' names (`DragDropEffectOrder`) and
    popup list rows.
  - **Fit** (`PlainTextComponent`): `Panel::text_centred`, used by preset
    strip names, type texts, the filter's DRIVE and KEY TRK labels, and
    the stereo and smooth mode texts. Also button and shape-button text
    (`OpenGlButtonComponent`'s text), envelope times, the value popup's
    text, the header's preset name, and the logo-area product name (the
    port's stand-in for upstream's logo image).

**Verified.**

- `tests/text.rs`:
  - Synthetic edges pin the curtail rule and its pixel of grace.
  - The ellipsis backs off until three full stops fit. A line of three
    glyphs or fewer, or with fewer than three kept, is only curtailed.
  - The squeeze, the 0.7 floor and `fitLineIntoSpace`'s half pixel, then
    the ellipsis over squeezed glyphs.
  - Centring leaves out whitespace.
  - Through egui at display scale 2, "0.000 cents" (the voice tune's
    reading) in a box 4 pixels too narrow is drawn as "0.000 cent". With
    ellipses it is "0.000 cen" followed by "...", the stops after the
    kept glyphs. Fitted, it is squeezed to the box from its left.
- Gallery against ref-gui (image in the PR):
  - The comb filter's "Comb : Low High Flange+" was overflowing past its
    arrows. It now squeezes into its box like upstream's: the view's
    difference from the reference drops 1%.
  - The logo-area name fits its square.
  - Every other view's text is pixel-identical to before, apart from one
    label.
- The reverb's "PRE HIGH CUT" (a `drawLabelForComponent`, so curtailed)
  reads "PRE HIGH CU" here: the port drew the reverb's labels about 9%
  larger than the reference, which Tabs Builder fixes in #144.
- `cargo clippy --workspace --all-targets -D warnings`, `cargo fmt`, the
  `sas-gui` tests and `gui_reference.py layout --check` pass.

**Not done.** `drawFittedText` with more than one line (`splitLines`)
isn't ported. No `PlainTextComponent` in the port's views draws more than
one line.
