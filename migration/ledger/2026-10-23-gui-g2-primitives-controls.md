## 2026-10-23: GUI G2, shader quads and the first controls

**Changed.** `sas-gui` draws upstream's shader-drawn quads and has its first
working controls, shown in the Voice and Portamento sections and wired to
the host parameters.

- Quads: `primitives/quad.wgsl` ports `kPassthroughVertexShader` and every
  fragment shader `OpenGlMultiQuad` uses (colour, fade square, circle, ring,
  rounded corner, rounded rectangle and border, diamond, rotary slider,
  horizontal and vertical slider, rotary and linear modulation, modulation
  knob) line by line. `QuadBatch` draws any number of quads in one instanced
  draw through an egui-wgpu paint callback, with upstream's uniform rules
  (alpha folded into the colour where a shader has no `alpha_mult`,
  transparent alternate colours replaced by the main colour at zero alpha,
  `setRounding` doubling) and normal or additive blending.
- Vendored egui-baseview: paint callbacks need the target's format and
  sample count, which egui-wgpu does not pass. `port/vendor/egui-baseview`
  is egui-baseview 0.9.0 with a patch that publishes them (`PATCHES.md`),
  used through `[patch.crates-io]` and excluded from the workspace.
  Recorded in `decisions/gui-framework.md` and the bundle's third-party
  notices.
- Controls: `SynthSlider` as rotary knobs (body, shadow arc, quad), text
  sliders (`TextLookAndFeel`), curve sliders (`CurveLookAndFeel`) and linear
  sliders; text toggle buttons (`OpenGlButtonComponent`'s text style with
  its hover fade). Gestures follow upstream and JUCE: press begins the host
  gesture, `RotaryHorizontalVerticalDrag` over 200 pixels per turn divided
  by the slider's sensitivity, command for a tenth of the speed, indexed
  values snapped like `Slider::constrainedValue`, double click to the
  default, alt-click or the menu's "Enter Value" for a text entry (text
  selected, value parsed by `SynthSlider::getValueFromText`), right click
  for the menu, buttons toggle on release.
- Popups: the value bubble (`PopupDisplay`), the selection list
  (`SinglePopupSelector`, also the stereo mode selector) and the text entry,
  placed as `FullInterface` places them. Values print with
  `SynthSlider::formatValue`, which differs from the host's text.
- Host access: `EditorModel::param` returns each control's `ParamPtr`, and
  every change goes through nice-plug's `GuiContext` (begin, set, end).
- Sections: `VoiceSection` and `PortamentoSection` (layout, background,
  controls), on the shared `SynthSection` helpers in `section.rs`.

**Verified.**

- Layout: 11 more upstream components in `migration/corpus/gui/layout_paths.txt`
  (every Voice and Portamento control); fixtures regenerated with ref-gui.
  The port's rectangles equal upstream's at all six sizes.
- Pixels: the bottom row of `init_voice` side by side with upstream's
  (ref-gui under Xvfb). Knobs, arcs, text components, labels, the curve and
  the buttons match, apart from text size (next point).
- Fonts: JUCE's FreeType fonts (Linux, where ref-gui renders) make the em
  size `points * unitsPerEm / ascender`; its CoreText fonts (macOS) make it
  `points`. The port follows macOS, so the Linux reference's text is 1.3%
  (Lato), 3.3% (Montserrat) or 7.7% (Droid Sans Mono) larger. Documented in
  `fonts.rs`; not a divergence on the target platform.
- Interaction (`tests/controls.rs`, real nice-plug parameters and a
  recording host context): a knob drag is one begin/set/end gesture with
  the expected value, command slows it tenfold, a text slider at
  sensitivity 0.2 snaps to whole voices, double click returns to the
  default, the popup text follows the drag and hides on exit, a button
  toggles on release in one gesture, the right-click menu sets the default,
  the stereo mode selector sets the mode, and alt-click accepts typed text.
- Unit tests: value formatting and parsing against upstream's rules (and
  JUCE's number printing), drag arithmetic, interval snapping, JUCE colour
  operations, quad instance data, and the WGSL validated by naga.
- `cargo fmt`, `clippy -D warnings`, workspace build and the `sas-gui` and
  `sas-plugin` tests pass locally.

**Not done (tracked).**

- Mouse wheel: JUCE reverses the wheel when macOS natural scrolling is on,
  so a knob follows the physical wheel. baseview does not report that, so
  porting it now would turn knobs the wrong way for most Mac users. Needs
  the flag added to the vendored adapter.
- MIDI learn menu items: the engine has no MIDI learn yet.
- Modulation arcs and the modulation menu items arrive with G4; the quad
  shaders for them are already ported.
- Upstream eases knob thickness on hover (`kThicknessDecay`) and fades quad
  alpha; the port switches at once.
- Line, fill, bar and image renderers moved to G3, where the first views
  that draw them (oscilloscope, envelopes, filters) arrive. The Voice and
  Portamento sections moved from G3 into G2 as the controls' proving
  ground.
