## 2026-10-26: GUI G3b (4a), the modulation column

**Changed.** The Voice tab's right column (`sections/modulation.rs`,
`ModulationInterface`), shown on every tab as upstream does: envelopes,
LFOs and random LFOs, each a column of source tabs beside the selected
source's section, and the two rows of keyboard sources.

- Tabs (`controls/modulation_button.rs`): `ModulationButton`'s background
  (selected colour, meter well, border, the source's name, the shadow
  where a tab joins its section) and `ModulationTabSelector`'s layout,
  shadow and minimum count (3 envelopes, 4 LFOs, 2 randoms, plus one past
  the last connected source once connections exist). A click on a tab
  shows that source's section.
- Envelope section (`sections/envelope.rs`): the six knobs and labels, and
  the editor's backdrop.
- LFO section (`sections/lfo.rs`): mode selector, the rate (free, tempo or
  keytracked by the sync value, with the keytrack divider), phase, fade or
  smooth time with the smooth-mode menu, delay and stereo; the top row's
  backgrounds, the name strip's arrows and the editor's backdrop.
- Random section (`sections/random.rs`): sync and stereo switches, style,
  rate, and the viewer at rest (backdrop and centre line).
- Shared: `PixelPainter::fill_rect_gradient` (JUCE's linear and radial
  `ColourGradient` fills), `with_clip`, `Colour::with_alpha`;
  `section::paint_tab_shadow`, `place_tempo_controls`, `keytrack_divider`,
  `paint_widget_background`; `Panel::widget_corners`.
- UI Reviewer's follow-ups from #118: a released slider keeps drawing
  its own value until the host shows it (at most 10 frames), so a knob
  doesn't flash its old value after a double-click reset or the pitch
  wheel's spring-back; the FM loop tests run with the host applying sets
  at once and a frame later.
- Still to come: the envelope editor (4b), the LFO editor with its grid,
  paint and smooth controls (4c), the sync selector (Tabs Builder's shared
  `TempoSelector`, swapped in when it lands), the magnifying glass and
  other icons (after #116), the random viewer's animation, source
  connections and dragging (G4), and the LFO's name and browser.

**Verified.**

- Layout: the column, its tab selectors and tabs, the first envelope, LFO
  and random section with every control, and both keyboard rows added to
  the fixtures; the port's rectangles equal upstream's at all six sizes.
- Pixels, side by side with ref-gui
  (`media/gui/g3b-modulation-column-side-by-side.png`): the column matches
  apart from what is still to come. Upstream's random SYNC button shows
  its hover colour in the reference frame (the virtual display's pointer);
  its value is off, as in the port.
- Interaction tests: a tab click shows its source (a drag then moves that
  envelope's knob), the smooth-mode menu, a free-running LFO's frequency
  slider, and the random stereo switch.
- `cargo clippy --workspace --all-targets -D warnings`, `cargo fmt` and the
  `sas-gui` tests pass.
