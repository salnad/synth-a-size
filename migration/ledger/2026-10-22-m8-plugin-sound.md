## 2026-10-22: M8 PR 3, the plugin plays the M7 engine; `sas-render`; VST3 sound check

**Changed.**

- *The plugin makes sound.* `SynthASize` is now
  `SynthPlugin<SoundEngineHost>`, the M7 `Synth` behind the `Engine` trait
  (#75). This is the first build that makes sound in FL Studio: the
  `synth-a-size-vst3-arm64-macos` artifact of the `Plugin` workflow.
- *Upstream's startup and load order* (`vital/src/plugin/synth_plugin.cpp`,
  `synth_base.cpp`). The engine's output depends on it, as measured below.
  - `SynthPlugin::new` no longer draws from the engine's random seed counter.
    Upstream's `SynthPlugin` never calls `loadInitPreset`: the engine starts
    as constructed, and its white-noise sample takes the only init seed,
    inside `SoundEngine`'s constructor. An extra draw shifted every later
    random draw.
  - The init document now counts as already loaded (`document_generation`
    starts at 0), so nothing is loaded at startup.
  - `Plugin::activate` no longer prepares the engine right away.
    `BlockProcessor::activate` notes the document generation, and the next
    block replays upstream's order, where `setStateInformation` loads
    synchronously. A state restored before activation (a host opening a
    project) loads, then the engine is prepared (`prepareToPlay`), as in the
    reference's `ref-render`. A state restored after activation loads into
    the prepared engine.
  - Preset loads and parameter changes reach the engine at the start of the
    next block even while bypassed, as upstream applies them while bypassed.
    Bypassed blocks still render silence and drop MIDI.
- *Automation is applied once per block, as upstream* (Chain Reviewer, a
  divergence inherited from #59). `SAMPLE_ACCURATE_AUTOMATION` is now
  `false`. With `true`, nice-plug split host blocks at every automation
  point, which changes the engine's chunking and the values inside the block.
  Upstream's JUCE VST3 wrapper applies only the last point of each parameter
  queue, once, before `processBlock` (`juce_VST3_Wrapper.cpp`,
  `processParameterChanges`). nice-plug now applies every point before
  `process`, leaving the last value. MIDI CCs, which VST3 delivers as
  parameters, keep their sample offsets, as in JUCE. Test:
  `automation_applies_once_per_block_like_upstream`.
- *`sas-render`* (`port/crates/plugin/src/bin/sas-render.rs`, logic in
  `src/render.rs`). An offline render CLI with a subset of `ref-render`'s
  options and its defaults: `--init`, `--no-midi`, `--sample-rate` (44100),
  `--block-size` (64), `--length`, `--tail` (2.0), `--bpm` (120) and
  `--seed`. It has no `--warmup-samples`, `--format` (always f32), `--meta`
  or `--dump-state`. It seeds the
  counter, builds the synth, loads the preset, renders `last event + tail`
  through `sas_engine::synth::render` and writes a 32-bit float stereo WAV.
- *VST3 sound check* (`port/crates/plugin/vst3-smoke/`). A minimal host on
  the VST3 SDK's hosting helpers: it loads the bundle, plays middle C for
  about 1 s and fails if the held peak is below `--min-peak`. The validator
  checks API conformance, not audio. The same CMake project builds the SDK's
  `validator`. Both are CI-only tools; the SDK is MIT since 3.8.0 and is never
  linked into the plugin.
- *CI* (`.github/workflows/plugin.yml`):
  - The macOS arm64 job builds both tools (cached on the SDK tag and the tool
    sources) and runs the validator, then the sound check, on the shipped
    bundle.
  - A new `vst3-linux-dev` job runs the same validator and sound check on the
    x86_64 Linux dev bundle (never shipped), plus a `sas-render` render. It
    gates where the macOS job, non-gating by owner policy, cannot.

**Measured.**

- `the_plugin_plays_the_engines_render`: `init_saw` restored as host state,
  `chord_progression.mid` played through the plugin's audio callback in
  128-sample host blocks at 48 kHz. It is **bit-identical** to
  `sas_engine::synth::render` of the same input, with peak 0.846.
  - Before the seed fix, the two differed from sample 0, with a max abs
    difference of 1.12.
  - Calling `set_tuning`, and the bpm/switch order, change nothing; the extra
    seed draw alone reproduces the difference.
- `cli_renders_match_the_reference`: all four `render/*` cases the engine
  covers, rendered through `render_offline`, compared through the WAV bytes
  it writes:
  - `init_saw__single_note`;
  - `init_saw__chords`;
  - `init_saw__single_note_48k_b128`;
  - `unison_pad_fx__chords`.

  Each has the reference's exact frame count, from `ref-render`'s length
  rule, and passes its case tolerances.
- VST3 sound check on the Linux dev bundle: silence before the note, peak
  0.359 while held, 0.318 after release. On the silent pre-#75 build it
  reported 0 and failed, as intended.
- Steinberg validator (`-e`, SDK 3.8.0) on the same bundle: 537 passed,
  0 failed.
  - It logs "The bypass parameter was not correctly set" in its
    bypass-persistence test, which still passes. This also appeared on #59's
    silent build.
  - Cause: nice-plug ignores `IEditController::setParamNormalized` while
    processing, because the host delivers the change through `process()`.
    The test sets bypass on the controller mid-processing and reads it back.
  - Hosts send parameter changes through `process()`, so bypass in FL Studio
    is unaffected.
- 40 `sas-plugin` tests; `cargo clippy --workspace --all-targets -D warnings`
  is clean.

**Known gap (follow-up, needs a `sas-engine` accessor).** The plugin can't
reach the engine's own init white-noise sample, so the init document's sample
uses seed 0. An untouched init state therefore saves different white noise
than the engine plays, until a preset is loaded. This only matters with the
sample oscillator on and no preset loaded. The fix is for `Synth` to expose
its constructed sample (`SampleState`), so the init document can carry the
engine's own noise.

**Still to do.**
- Modulation routing comes with M7 slice B; presets with modulations render
  without them until then.
- FL Studio check on an Apple Silicon Mac (owner).
