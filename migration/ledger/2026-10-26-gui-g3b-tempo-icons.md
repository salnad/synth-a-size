## 2026-10-26: GUI G3b follow-ups, the tempo selectors, the icons and the magnifying glass

**Changed.**

- The LFO and random sections' sync selector is Tabs Builder's shared
  `TempoSelector` (#122): its icon, its menu on a press, and the slider it
  picks (`TempoMode`). The sections' own `Rate` and
  `section::draw_tempo_divider` give way to `TempoMode` and
  `tempo::paint_divider`.
- `controls::path`: on #131's curved joints and rounded ends (discs at the
  vertices), bevelled joints and square ends as JUCE draws them, and the
  stroke split per subpath so an end can take an arrowhead. `stroked_with_arrowheads`
  (`createStrokeWithArrowheads`: each subpath shortened by the arrowheads'
  lengths, capped with a triangle as wide as the arrowhead tipped at the
  original end) and `centred_arc` (`addCentredArc` as a new subpath).
- `controls::paths`: `magnifyingGlass`, `loop`, `bounce`, `shuffle`,
  `pencil`, `halfSinCurve` (keeping upstream's start at `(start_x,
  end_x)`), `paintBrush`, and the filter blend's `bandPass`, `leftArrow`,
  `rightArrow`, `phaser1`, `phaser3`, `narrowBand` and `wideBand` beside
  #131's `low_pass`, `high_pass` and `notch` (with its
  `create_filter_stroke`); the buttons' icons as `paths::Icon`, drawn
  through #131's `Shape::Icon` (`Shape::from(icon)`).
- Buttons: `SynthButton::icon` (an `OpenGlShapeButton` bound to a control,
  with `useOnColors`), `Shape::Icon` and `ShapeColours::icon_on`. A shape
  button now reports its drag (`mouseDrag`'s delta, in points) and a
  double click at the second click's release (JUCE's rules).
- Icons in place: the sample's keytrack, random phase, loop and bounce
  toggles; the LFO's paint and smooth buttons (on colours while on); the
  oscillator's edit pencil (its action comes with the wavetable editor,
  G5); the filters' blend icons for each model and style
  (`getLeftMorphPath` / `getRightMorphPath`), in the body text colour, in
  both layouts (the effects tab's call to the filter's background now
  passes the host).
- The envelope's magnifying glass (`DragMagnifyingGlass`): a drag zooms
  (`magnifyZoom`, 2^(0.02 · Δy)), a double click fits the release with a
  quarter to spare (`magnifyReset`).
- The envelope editor's double click straightens a power at the second
  click's release, after any drag on that press (JUCE's `mouseDoubleClick`
  after `mouseUp`; Voice Reviewer's note on #126).

**Verified.**

- Pixels, side by side with ref-gui
  (`media/gui/g3b-icons-tempo-side-by-side.png`,
  `media/gui/g3b-filter-blend-icons-side-by-side.png`): the sample's four
  toggles, the magnifying glass, the paint brush and half-sine, both tempo
  notes, and the blend icons of all eight filter models match.
- Path tests: end caps reach half the thickness past the ends (rounded and
  square) or stop at them (butt); an arrowhead caps the shortened end;
  curved joints round the corner a mitre squares; centred arcs start at
  twelve o'clock; #131's curved-joint test stands. (The test that bevelled
  joints and square ends panic is replaced by these, those styles now
  being ported.)
- Interaction tests: a double click whose second press drags bends the
  power in one gesture and straightens it in the next; dragging the
  magnifying glass up zooms in, so the same drag of the attack point sets a
  shorter attack.
- Voice Reviewer's follow-ups on #132 and #133: with a model reporting the
  hertz setting on, a filter cutoff's popup reads "Cutoff: … Hz" through
  the editor; the sample viewer reads the sample once per content revision
  and again when it moves.
- `cargo clippy --workspace --all-targets -D warnings`, `cargo fmt` and the
  `sas-gui` tests pass.
