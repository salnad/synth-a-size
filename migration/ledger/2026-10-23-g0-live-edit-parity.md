## 2026-10-23: G0 live-edit parity, ref-render edit scripts

Second G0 PR, stacked on the edit bus (#94). Live edits are now
parity-tested against the C++ reference: `ref-render` plays a timed edit
script, and the port plays the same script through the plugin's edit bus.

**Harness**

- `ref-render --edits PATH` reads a JSON script of timed edits. Before each
  host block, it applies every edit whose time, placed like a MIDI event at
  `floor(seconds * rate + 0.5)`, is at or before the block's first sample.
  Each edit makes upstream's own editor calls (table in
  `migration/reference/README.md`):
  - `set_control` goes through `valueChangedExternal`, without the GUI
    callback.
  - `connect_modulation` is `ModulationManager::modulationDraggedToComponent`.
    The amount comes from the destination control's position, then
    `SynthGuiInterface::connectModulation` runs (with
    `initModulationValues` for a new slot), then
    `setModulationValues(amount, bipolar, false, false)`.
  - `disconnect_modulation` is `SynthGuiInterface::disconnectModulation`.
  - `line_mapping` calls the slot's line map `jsonToState`, then `render`.
  - `lfo_shape`, `wavetable` and `sample` are `LoadSave::loadLfos`,
    `loadWavetables` and `loadSample` for one target, which load onto the
    engine's existing objects.
  - The metadata records the script and `edits_applied`. An edit timed
    after the end of the render fails it, so a case can't silently lose
    edits (Parity Reviewer). Renders without `--edits` are unchanged,
    metadata included.
- Within one block, `ref-render` rejects a script that sets a
  `modulation_N_*` control and then connects a modulation (Graph Reviewer).
  The port's plugin applies edit-bus edits before the block's host
  parameters, upstream in call order, so the two agree only when a slot's
  controls follow its connection.
- Render cases take an optional `"edits"` corpus path (`.json`).
  `run_corpus.py` passes it to `ref-render`, and `sas-parity` validates it
  (`RenderCase::edits_path`). There's a unit test on each side, with the
  same error text.
- `generate.py` writes `migration/corpus/edits/live_edits.json`. Over a
  held-pad MIDI file it does:
  - a modulation connected (LFO 1 to filter 1 cutoff, with the filter
    turned on) and its amount set;
  - an LFO shape loaded;
  - the slot's remap curve loaded;
  - a bipolar drag of envelope 2 onto oscillator 1's wave frame;
  - a two-keyframe line wavetable loaded while notes sound;
  - an authored bell sample loaded and the sampler turned on;
  - the first connection removed, then made again (a new slot, so its
    curve resets to linear), then remapped.

  These are 17 edits.
- New cases: `render/init_saw__live_edits` (44.1 kHz, block 64) and
  `render/init_saw__live_edits_48k_b512`, where edits fall inside blocks.
- Each case list asserts it equals the render cases of `cases.json` (via
  `sas_parity::cases::load_render_cases`) filtered on `edits`, so a new
  render case can't skip port parity unnoticed (Parity Reviewer). The
  engine's `tests/render.rs` lists the cases without a script, and the
  plugin's `tests/live_edit_parity.rs` the cases with one.

**Port test** (`plugin/tests/live_edit_parity.rs`). The script plays through
`SynthPlugin`:
- modulation routing, curves, LFO shapes, wavetables and the sample go over
  `edit_handle()`, each JSON edit loaded onto a copy of the mirror's current
  object, as `jsonToState` loads onto the engine's;
- a connection uses `SlotValues::dragged_to` at the destination's live host
  parameter value;
- controls, and a connection's `parameter_changes`, are set as host
  parameters.

Each edit is made before the first host block that starts at or after its
time.

**Results**

- Both cases pass at the default render tolerances. The port is
  **bit-identical to the `REF_FAST_MATH=OFF` reference** on both (every
  sample). Against the official reference the residual is −59.80 dB at
  44.1 kHz / 64 and −59.03 dB at 48 kHz / 512, which is that reference's own
  fast-math noise against its no-fast-math build.
- Mutation checks. Each run below fails parity:
  - dropping `set_control`, `connect_modulation`, `disconnect_modulation`,
    `line_mapping`, `lfo_shape`, `wavetable` or `sample` edits (each type
    alone, on the 44.1 kHz case);
  - applying edits one block early (both cases).

  `the_edits_change_the_render` keeps a coarse version in the suite: the
  render without the script is far from the render with it.
- Reference hashes were written from this VM's reference build, whose
  identity matched all 2772 committed hashes (`hashes --check
  --require-known-identity`, 0 problems). CI's cross-machine check confirms
  them. The macOS arm64 hashes come from the advisory job's `macos-hashes`
  artifact (run 39908662632, at 2209282 before the rebase onto main; the
  reference and corpus are unchanged by the rebase).

**Finding: the unison pad amplifies the reference's noise.** The first draft
ran the same script on `unison_pad_fx` (unison, chorus, delay, reverb).
- The port was bit-identical to the no-fast-math reference there too.
- But the official reference against its own no-fast-math build gave
  residual −51.8 dB and LSD 0.1076. That's over the 0.1 limit, with the
  noise accumulating in the effect tails after the filter turns on. The
  unedited `unison_pad_fx__chords` gives −61.9 dB.
- So the case moved to `init_saw` (one oscillator, no effects), which keeps
  every edit type and puts the reference's noise at about −59.5 dB. No
  tolerance was touched.

**Next:** the telemetry snapshot and status-output audit (PR 3).
