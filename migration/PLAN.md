# Port plan

Each milestone ends when its exit criteria pass in CI. Parity means the
port's output meets the tolerances in `migration/corpus/cases.json` when
compared with `audio-diff` against the C++ reference at the same sample
rate, block size, seed and length. Rules: [AGENTS.md](AGENTS.md). Progress:
[ledger/](ledger/README.md) (history before 2026-10-21: [LEDGER.md](LEDGER.md)).

Crate map (`port/crates/<dir>`, package `sas-<dir>`):

| Crate | Milestone | Upstream |
|---|---|---|
| `simd` | M1 | `synthesis/framework/poly_values.h`, `poly_utils.h`, `futils.h` |
| `dsp-core` | M1 | `synthesis/framework/*` (Processor, ProcessorRouter, Value, Feedback, operators), `lookups/lookup_table.h`, `utilities/*` |
| `preset` | M2 | `common/synth_parameters.cpp`, `common/load_save.cpp`, parts of `common/synth_base.cpp` |
| `modulation` | M3 | `synthesis/modulators/*`, `modules/{envelope,lfo,random_lfo}_module.cpp`, `modules/modulation_connection_processor.cpp`, `common/line_generator.cpp` |
| `wavetable` | M4 | `synthesis/lookups/{wavetable,wave_frame}.cpp`, `common/wavetable/*` |
| `oscillators` | M4 | `synthesis/producers/*`, `modules/{oscillator,sample,producers}_module.cpp` |
| `filters` | M5 | `synthesis/filters/*`, `modules/{filter,filters,formant,comb}_module.cpp` |
| `effects` | M6 | `synthesis/effects/*`, effect modules, `modules/reorderable_effect_chain.cpp` |
| `engine` | M7 | `synth_engine/sound_engine.cpp`, `modules/synth_voice_handler.cpp`, `framework/voice_handler.cpp`, `common/midi_manager.cpp`, `common/tuning.cpp` |
| `plugin` | M8 | `src/plugin/*` (behaviour only; the wrapper is nice-plug) |
| `llm-control` | M9 | new |
| `gui` | M10 | `src/interface/*` (behaviour and layout reference only) |

## M0: harness and scaffolding (this PR)

- `/vital` vendored at a pinned SHA and verified in CI.
- `ref-render` (preset + MIDI → WAV, deterministic) and `ref-module`
  (single processor → NPY) built from the vendored sources.
- `audio-diff` comparison tool with tolerances and exit codes.
- An authored golden corpus and a runner.
- Rust workspace with stub crates; the nice-plug plugin stub exports
  VST3/CLAP.
- CI: reference build, corpus render with a determinism check, Rust
  build/test/lint.

Exit: CI green; renders uploaded as an artifact.

## M1: SIMD `poly_float` and the processor graph

- `simd`: `PolyFloat`, `PolyInt` and `PolyMask` (4 × f32/u32, SSE2 on
  x86_64, NEON on aarch64, with a scalar fallback used to check the other
  two), plus the full operator set from `poly_values.h`/`poly_utils.h`
  with the same NaN, rounding and clamping behaviour. Port `futils.h`
  exactly, including its own approximations (`exp2`, `log2`, `tanh`,
  `sin`, ...) rather than calling libm.
- `dsp-core`: `Output`/`Input`/`Processor`/`ProcessorRouter`, control-rate
  vs audio-rate outputs, triggers (mask/value/offset), `Value`,
  `cr::Value`, `Feedback`, `SmoothValue`, operators, lookup tables,
  oversampling plumbing and sub-block splitting at 128.
- Tests: unit tests for every SIMD op against scalar math, and reference
  checks for operator processors via new `ref-module` entries.

Exit: SIMD and operator module captures within tolerance; graph ordering
matches upstream on a small routed graph.

## M2: parameter registry and preset round-trip

- `preset`: every parameter from `synth_parameters.cpp` (name, min, max,
  default, value scale, display info, version added), the modulation
  slot params, and string tables.
- Load/save of the upstream preset JSON: default filling, the
  `updateFromOldVersion` upgrade chain, wavetables, LFO shapes, sample,
  modulations with line mappings, macro names and metadata.
- Tests: for every corpus preset, `ref-render --dump-state` JSON equals
  the port's load→save JSON (after canonical float formatting); round-trip
  is idempotent; parameter count and names match upstream exactly.

Exit: state JSON parity on all corpus presets plus the init preset.

## M3: modulators and the modulation matrix

- Envelopes, LFOs (all sync modes, smoothing, fade, delay, stereo),
  random LFOs (all styles), macros, mod wheel, pitch wheel, velocity,
  aftertouch, note and note-in-octave sources.
- `ModulationConnectionProcessor`: amount, bipolar, power, stereo,
  bypass, line mapping; mono vs poly destinations.
- Decide on an exact `mt19937` + `uniform_real_distribution<float>`
  replica for random features (and record the decision).
- Tests: `ref-module` captures for envelope/LFO/random LFO and
  modulation-connection processors.

Exit: module captures within tolerance; random features either exact or
covered by a recorded phase-blind comparison.

## M4: wavetables and oscillators

- Wavetable creator: Wave/Line/Audio-file sources, all modifiers,
  keyframe interpolation, normalisation, DC removal, FFT to frequency
  domain, and rendering into 257 frames with the same FFT size and
  scaling.
- Oscillator: wavetable lookup and interpolation, unison (voices, detune,
  power, range, spread, blend, stack styles), spectral morph modes,
  distortion modes, phase and random phase, the sample player and the
  sub/stack routing.
- Tests: wavetable frame captures (new `ref-module` entries) and
  single-oscillator renders.

Exit: oscillator-only patches at parity across sample rates 44.1/48/96
kHz.

## M5: filters

- All filter models and styles: analog (Sallen-Key), dirty, ladder,
  digital SVF, diode, formant, comb, phaser. Keytrack, drive, blend,
  dual-filter routing, filter FX.
- Tests: the `filter` `ref-module` captures for every model × style,
  including oversampled runs.

Exit: every model × style capture within tolerance.

## M6: effects

- Chorus, compressor (multiband), delay (all styles), distortion (all
  types, with its filter), EQ, filter FX, flanger, phaser, reverb, the
  effect chain order and dry/wet behaviour.
- Tests: per-effect captures and an all-effects corpus patch.

Exit: effect captures and the FX corpus cases within tolerance.

## M7: voices, MIDI, MPE, full-patch parity

- Voice allocation and stealing (`voice_priority`, `voice_override`),
  polyphony, legato, portamento, sustain/sostenuto/soft pedals, pitch
  bend range, MPE (per-note pitch, slide, lift, pressure), tuning files,
  `correctToTime` and tempo sync, and oversampling settings.
- Offline render API matching the `ref-render` processing contract.
- Tests: every `renders` case in `cases.json` and new MPE/tuning cases.

Exit: all corpus render cases within tolerance at each case's sample rate
and block size.

## M8: VST3 plugin in FL Studio on macOS

- Wire the engine into `sas-plugin`: parameters exposed with stable IDs
  from the registry, sample-accurate automation, MIDI/MPE, state
  save/restore as preset JSON, latency and tail, and `bypass`.
- macOS universal `.vst3` bundle (nice-plug bundler), ad-hoc signing,
  smoke test in a VST3 validator.
- Manual test: load in FL Studio on macOS, play the corpus MIDI, compare
  bounced audio against the reference with `audio-diff`.

Exit: plugin passes the validator; an FL Studio bounce of two corpus
cases is within tolerance; the test is recorded in the ledger.

## M9: LLM control layer

- `llm-control`: a typed API (and a tool schema for LLMs) to list and set
  parameters by registry name with value-scale-aware conversion, connect
  and disconnect modulations by source and destination name, generate and
  validate preset JSON and `WavetableCreator` JSON, and render offline to
  WAV through the engine.
- Validation against the registry: ranges, enum options, legal
  modulation sources and destinations.
- Tests: deterministic golden tests (generated JSON loads in the
  reference and the port and renders identically) and
  schema/validation tests.

Exit: an LLM-driven session can create a patch and wavetable from text,
and that patch renders identically in the reference and the port.

## M10: GUI (egui with wgpu)

- egui through nice-plug's adapter, drawn with wgpu (Metal on macOS);
  decision in `decisions/gui-framework.md`.
- A rebuild of upstream's own interface: oscillators and the wavetable view
  and editor, filters, envelopes/LFOs, drag-to-modulate, the mod matrix,
  effects and preset browsing, plus the LLM chat drawer. Layout, skin and
  fonts follow upstream's defaults (owner working default, 2026-10-22); no
  upstream names or logo (rule 2) and no factory content (rule 3).
- Checked against upstream's own interface (`reference/gui`): exact layout
  fixtures, side-by-side screenshots and interaction tests. Milestones G1–G7
  in the project's GUI plan.

Exit: usable editor in FL Studio on macOS, with screenshots recorded.

## M11: performance (only after parity)

- Profile the full corpus. Optimise hot paths (SIMD, allocation-free
  processing, voice culling) while keeping every parity test green.
- Target CPU use comparable to upstream at 48 kHz/128 with 8 voices.

Exit: performance target met with the full parity suite still passing.
