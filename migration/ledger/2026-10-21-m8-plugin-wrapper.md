## 2026-10-21: M8 PR 1, arm64 macOS VST3 wrapper without a GUI (`sas-plugin`, SAS-15)

**Changed.** `sas-plugin` goes from the M0 stub to a working wrapper around
an engine interface, porting the behaviour of `vital/src/plugin/`
(`synth_plugin.cpp`, `value_bridge.h`) on nice-plug 0.4.2:

- *Parameters* (`params.rs`): one host parameter per engine control of the
  `sas-preset` registry, in registry index order, as upstream's `SynthPlugin`
  adds a `ValueBridge` only where `controls_.count(name)` is non-zero. That is
  772 of the registry's 794 entries, `bypass` included; the other 22 are the
  legacy entries in
  `sas_preset::engine::NON_CONTROL_PARAMETERS` (`sub_*`, the per-filter
  `*_input` switches, `compressor_low_band_unused`), which only exist for the
  preset upgrade chain and can't be loaded or saved. The stable ID is the
  registry name (the preset `settings` key), so IDs never depend on order;
  the VST3 ID is nice-plug's hash of it. A test checks that the 772 hashes are
  unique and clear of nice-plug's MIDI CC parameter range. `bypass` is the
  host bypass parameter (`BoolParam::make_bypass`), as upstream's
  `getBypassParameter`. Indexed controls with fewer than 300 steps
  (`ValueBridge::isDiscrete`) are stepped `IntParam`s, so hosts see
  `stepCount = span` as JUCE reports from `getNumSteps`. The other indexed
  controls (`effect_chain_order`, the `*_transpose_quantize` bitmasks) are
  continuous for the host, as upstream. Both round host edits like
  `ValueBridge::convertToEngineValue`. Everything else is a `FloatParam` over
  `[min, max]`. Parameters hold the engine value itself, with groups per
  section (Oscillator 1, Filter FX, Modulation 12, ...) and host text from
  `ValueBridge::getText` (option names, display scale, units).
- *State* (`state.rs`): the host state is the preset JSON
  (`PresetState::to_json`) plus the `tuning` object, as
  `SynthPlugin::getStateInformation`. Restoring loads it on top of the current
  state through `PresetState::load_json`, so it gets the version upgrade
  chain. The tuning is kept as JSON until the engine's tuning state is
  ported, and a state without one keeps the current tuning, as upstream.
  `Plugin::filter_state` makes the preset JSON authoritative: it replaces
  nice-plug's saved parameter values with the preset's control values before
  the wrapper applies them. A state whose preset can't load (corrupted, or a
  newer version) is dropped whole, so nothing changes. Preset files:
  `plugin_state_from_file` (`SynthBase::loadFromFile`, named after the file)
  and `save_preset_file` (`SynthBase::saveToFile`: forces the extension,
  creates the folder, names the preset after the file, no tuning).
- *Engine interface* (`engine.rs`): an `Engine` trait with the calls
  `SynthPlugin` makes on `SynthBase` / `SoundEngine` / `MidiManager`
  (`setSampleRate`, `jsonToState`, `valueChangedExternal`, `setBpm`,
  `correctToTime`, `processMidiMessage`, `processAudio`) and a `SilentEngine`
  stub. The shipped type is `SynthASize = SynthPlugin<SilentEngine>`.
- *Audio callback* (`processor.rs`, `SynthPlugin::processBlock`): bypass
  outputs silence and drops MIDI (`processBlockBypassed`, no inputs). A newly
  restored preset goes to the engine; changed parameters follow, through a
  lock-free change set filled by the parameter callbacks. Then the host tempo
  and, while playing, the song position (`ppqPosition / bps`) are read, and
  the block renders in chunks of at most `MAX_BUFFER_SIZE` (128), each preceded
  by `correctToTime` and the MIDI inside it at offsets relative to the chunk.
  The clock advances by `num_samples / sample_rate`. nice-plug splits host
  blocks at parameter changes (`SAMPLE_ACCURATE_AUTOMATION`), so automation is
  sample accurate.
- *MIDI/MPE*: `MidiConfig::MidiCCs` delivers notes, poly and channel pressure,
  pitch bend and CCs on all 16 channels, which is what upstream's
  `MidiManager` reads for MPE (per-channel bend, pressure and CC 74). Events
  reach the engine as 3-byte MIDI messages (`NoteEvent::as_midi`), quantized
  to 7 bits (pitch bend 14) like the `juce::MidiMessage` upstream gets from
  JUCE's VST3 wrapper.
- *Bundle* (`port/crates/plugin/scripts/bundle.sh`): on macOS it builds
  `aarch64-apple-darwin` only, writes the `.vst3` bundle (Info.plist with
  bundle ID `io.github.salnad.synth-a-size.vst3`, minimum macOS 11.0), checks
  `lipo -archs` is exactly `arm64`, and signs it ad hoc with
  `codesign --verify --strict`. On Linux it makes an x86_64 bundle used only
  to run a validator during development; it is never shipped.
- *CI*: a new workflow, `.github/workflows/plugin.yml` (so `ci.yml` and
  `reference-arm.yml` are untouched). On macos-15 it bundles and signs the
  arm64 VST3, checks the `GetPluginFactory` / `bundleEntry` / `bundleExit`
  exports, builds Steinberg's VST3 validator from VST3 SDK v3.8.0 (MIT since
  3.8; cached, CI-only, not shipped), runs it in extensive mode, and uploads
  the bundle. nice-plug has no validator of its own. The plugin's Rust tests
  run with the workspace on Linux (`ci.yml`) and on macOS arm64
  (`reference-arm.yml`).

**Measured.** The Steinberg validator (`validator -e`, VST3 SDK v3.8.0) on
the Linux dev bundle: 537 tests passed, 0 failed, covering parameter scan,
units, bus layouts, state, 32- and 64-bit processing and automation. The
macOS arm64 run is in this PR's `Plugin` workflow. There are 32 Rust tests.
Every authored corpus preset (`migration/corpus/presets`, `preset_tests`)
loads through the host state path with JSON identical to `sas-preset`'s own
`load_file`, and every `error_*` preset fails with `sas-preset`'s error. A
host state round trip (through serde JSON, as nice-plug stores it) is exact
for every control, modulation slot, wavetable, LFO and the tuning, including
out-of-range and between-step values.

**Decisions and upstream behaviour kept.**

- *The preset's own value stands while the parameter still represents it.*
  A preset's −0.0 (`voice_tune` in `controls_and_metadata.vital`) reads back
  from the float parameter as 0.0, because nice-plug ignores a set that
  compares equal to the current value. A stepped parameter holds the rounded
  value of a non-integer preset value (e.g. `osc_1_transpose` 0.25 shows
  step 0), and upstream stores the value as given. The preset's value is
  saved and kept in the engine while two things hold: the control hasn't been
  set since the restore (a per-control "touched" bit, set by every parameter
  callback and cleared when a preset is restored), and the parameter equals
  the preset's value (for stepped ones, its rounding). A control moved away
  and back therefore saves and plays the parameter's value, as upstream
  would. After a preset load, the engine only gets `set_control` for
  controls that no longer stand for the preset. Tests:
  `negative_zero_survives_the_host_path` (capture → serde → restore),
  `host_state_round_trips_exactly`,
  `a_stepped_parameter_leaves_the_presets_own_value_in_the_engine` and the
  two `a_control_moved_away_and_back_*` tests (from Chain Reviewer's delta
  review).
- *Every save/load cycle delays the sample by 4 frames*: `Sample::stateToJson`
  encodes the stored buffer including its `kBufferSamples` zero padding, as
  `sas-preset` mirrors. The round-trip test asserts that shift, not equality.
- *Host text entry departs from upstream, approved by the owner* (2026-10-21,
  `2026-10-21-owner-approval-plugin-text-entry.md`). Upstream's
  `ValueBridge::getValueForText` returns `unskew(text / display_multiply)`, an
  engine-domain value, and JUCE's VST3 wrapper passes that straight to the
  host as the *normalized* value (`juce_VST3_Wrapper.cpp:529`,
  `outValueNormalized = param.getValueForText(...)`). `setValue` then maps it
  with `convertToEngineValue` and never clamps. So typing "72" for Filter 1
  Cutoff gives normalized 72, which sets the engine cutoff to
  72 × 128 + 8 = 9224. Typing an option name gives 0, because
  `getFloatValue` returns 0. nice-plug cannot reproduce this: text input
  becomes a plain value and then `preview_normalized`, which clamps to
  [0, 1], so out-of-range engine values are impossible. The closest
  reproduction would make every typed number above 1 select the maximum.
  The port instead inverts its own display text: it parses the number,
  removes `post_offset`, divides by `display_multiply` and unskews, and it
  also accepts option names. This affects display and text entry only, not
  parity.
- *VST3 note expressions* other than pressure have no MIDI equivalent and are
  dropped, as JUCE's wrapper has none. Note events addressed to every
  channel or key (CLAP only) are dropped too.
- *Process status* is `KeepAlive`, matching upstream's tail of 0 with
  `silenceInProducesSilenceOut() == false`. Latency is 0.
- CLAP export stays because it costs nothing; it is not tested or shipped.

**Follow-ups / blocked.**

- *Real engine wiring*: implement `Engine` on the M7 `SoundEngine` once it
  lands (voices are in #45) and swap the `SynthASize` alias. Upstream does
  `load_preset` with processing paused, and so does nice-plug, which restores
  state under `permit_alloc`. A heavy engine load should be prepared off the
  audio thread and swapped in.
- *Tuning*: replace the JSON passthrough with the engine's tuning state.
- *Offline render CLI* (port plan, Wave 3): renders through `BlockProcessor`
  with `SynthParams::restore_state`. It needs the real engine to be useful.
- *FL Studio check on an Apple Silicon Mac* (M8 exit criterion): needs the
  owner. Use the `synth-a-size-vst3-arm64-macos` artifact.
