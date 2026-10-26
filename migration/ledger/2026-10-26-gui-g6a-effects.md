## 2026-10-26: GUI G6a: the Effects tab, with Chorus, Delay, Distortion, Flanger and Phaser

**Changed**

- `sections::effects::EffectsTab` (`EffectsInterface`):
  - The effect order list (`DragDropEffectOrder`, `DraggableEffect`):
    one tile per effect, with its icon and power button. Dragging a tile
    reorders `effect_chain_order`.
  - The column of enabled sections, scrolled by the wheel or the scroll
    bar (`EffectsViewport`, `OpenGlScrollBar`).
  - `EffectsLayout` and `OrderLayout` hold the bounds; the layout
    fixtures check them.
- The Chorus, Delay, Distortion, Flanger and Phaser sections, using
  main's shared widgets:
  - Each has its sideways heading (`section::paint_container`,
    `Heading::Sideways`) and its power button on top
    (`SynthSection::setActivator`).
  - Rates use the shared `TempoSelector` (#122); Delay's style and
    Distortion's type and filter order are `Slider::selector`s with
    upstream's long names.
  - Distortion's filter knobs show disabled while its filter order is
    None.
- Their displays (`sections::effects::viewers`):
  - `ChorusViewer`: a bar per voice at its delay, from the
    `chorus_delays` status outputs.
  - `DelayViewer`: the dry tap and echoes fading by the feedback, per
    stereo style and tempo mode.
  - `DelayFilterViewer`, for the chorus and the delay.
  - `DistortionViewer`: the transfer curve, through
    `Distortion::get_drive_value` and `get_driven_value`.
  - `DistortionFilterResponse`, `FlangerResponse` and `PhaserResponse`:
    the engine's own `DigitalSvf`, `CombFilter` and `PhaserFilter` are set
    up as upstream's viewers set theirs up, then drawn through #119's
    evaluators. The flanger's line spans three widths and slides with
    `flanger_delay_frequency`, and feeds the comb y `(i / 2) % 2`.
  - Each reads a control as upstream's `getValue(output, slider)` does:
    the mono modulation total from telemetry while its processor is
    enabled, else the control's value.
  - Dragging a display moves its controls as upstream's `mouseDrag` does,
    as one host gesture per drag. The values are kept between frames, so
    a host that applies sets a frame late can't skew them (#118).
- `controls::bars::BarRenderer` (`BarRenderer`), drawn with #108's
  `QuadKind::Bar`; bars at non-finite positions are skipped.
- `Colour::float_alpha` and `Colour::with_alpha` (JUCE's `getFloatAlpha`
  and `withAlpha(float)`).
- Side-by-side views: the gallery loads a view's preset, and a view can
  scroll the effects column (`effects_scroll`, `ref-gui --effects-scroll`,
  `Interface::set_effects_scroll`). New views `all_effects` and
  `all_effects_bottom` show all nine sections between them.
- The FL Studio checklist gets item 5, the Effects tab drags, with a
  REAPER note.

Not in G6a: Compressor, EQ and Reverb (G6b). The Effects tab's filter
comes from GUI Builder's `FilterSection::fx()`, placed once it lands.

**Known difference.** The side-by-side frames differ for the displays
that read the engine's outputs (the chorus bars, the flanger's position,
the phaser's cutoff and the distortion filter's modulated values).
`ref-gui`'s engine never processes, so upstream's outputs read 0, while
the gallery has no engine and shows the controls' values. Running the
engine inside `ref-gui` crashes in `vital::Compressor::processWithInput`
(a null input), even with the render harness's preparation. That is
harness work for later. In the plugin, telemetry carries the live values.

**Tests**

- `sas-gui` layout fixtures: 80 Effects tab paths, laid out with
  `all_effects.vital`, at all six sizes.
- `sas-gui` unit tests:
  - `sections::effects::moving_an_effect_shifts_the_ones_between`
    (`moveEffect`).
  - `sections::effects::delay::a_stereo_delay_splits_its_tempo_controls`.
  - `controls::bars::vertical_bars_follow_the_vertex_shader`.
  - The viewers' uniform setup, against upstream's filter setups:
    `phaser_uniforms_follow_the_filter_setup`,
    `flanger_uniforms_clamp_the_comb_feedback` and
    `distortion_uniforms_keep_the_cutoff_above_the_minimum`.
- Side by side with `ref-gui` (local): the Chorus and Delay filter
  responses, the Delay bars, the Distortion curve and the Phaser response
  match. Text and knob edges show the known anti-aliasing difference.

No audio change.
