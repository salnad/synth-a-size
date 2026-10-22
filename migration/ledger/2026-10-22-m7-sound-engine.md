## 2026-10-22: M7 slice A, `SoundEngine`, offline render and the plugin's engine

First slice of M7's full-patch work (Voice Engine). It builds on #45
(`SynthVoiceHandler` note articulation), the M3 modulators (#18, #25, #33,
#37), the filter modules (#58) and Effects' `ReorderableEffectChain` (#71),
all merged.

**What changed**

- `synth_voice_handler.rs` builds the **whole upstream voice graph, in
  upstream order**: producers, 8 LFOs, 6 envelopes, `TriggerRandom` and 4
  random LFOs, the filters module, the voice output stage (velocity track ×
  voice amplitude × amplitude envelope, squared, which is the voice killer)
  and the 64 modulation-bank processors with their amount and power
  controls. The order is load-bearing: every oscillator, sample source and
  random modulator takes the next engine RNG seed when it is constructed or
  cloned, so each voice's seeds depend on everything built before it.
- `modulation_bank.rs`: `ModulationConnection` and
  `ModulationConnectionBank` (`common/synth_types.cpp`).
- `sound_engine.rs`: `SoundEngine` (`synth_engine/sound_engine.cpp`): the
  engine controls, the voice handler, Effects' chain, the output stage
  (`Add` with the direct output, `Decimator(3)`, decoding `StereoEncoder`,
  `SmoothVolume`, `PeakMeter`, `Clamp` into the engine output), every status
  output (compressor meters, chorus delays, phaser cutoff, flanger
  frequency, peak meter), `checkOversampling`/`setOversamplingAmount`,
  `process` (legato, denormal flushing, mono modulations with no voices,
  status outputs), `setBpm`, `correctToTime`, `allSoundsOff` and the
  note, wheel, pressure, slide and pedal forwarding.
- `midi_manager.rs`: `MidiManager::processMidiMessage` and its helpers,
  with the parts of JUCE's `MPEZoneLayout` it reads (default 15-member lower
  zone; zones set by the MPE configuration RPN). Upstream's MPE-zone
  `processSlide` passes `(value, start, end, 0)` to
  `setChannelRangeSlide(from, to, value, sample)`; the call is reproduced as
  written.
- `midi_file.rs`: Standard MIDI File reading as `ref-render` does it
  through JUCE (`convertTimestampTicksToSeconds` with the tempo map, tracks
  merged and stable-sorted, meta and sysex skipped).
- `synth.rs`: `Synth`, the engine-facing parts of `SynthBase`: the
  constructor (engine, tuning, default wavetables rendered), preset loading
  through `sas-preset`'s `PresetState` (`loadControls`, the sample, the
  wavetables, the LFO shapes through their JSON, `checkOversampling`),
  `setTuning`, `processMidi`, `processAudio`, and `ref-render`'s block loop
  (`render`, `RenderSynth::renderBlock`: engine sub-blocks of at most 128,
  each preceded by `correctToTime` and its MIDI, denormals flushed for the
  whole block).
- `sas-plugin`: `SoundEngineHost` implements the wrapper's `Engine` on
  `Synth`, including #65's `set_tuning`. `Synth`'s
  graph is `Rc`/`RefCell`-based and wholly owned, so the host asserts `Send`
  (`unsafe impl`, with the invariant written at the impl): the engine moves
  between threads only as a whole and is used from one thread at a time.
  Its public surface passes only plain values (no accessor exposes the
  `Synth` or a graph handle), and the graph keeps no thread-local buffers.
  `SynthASize` still uses `SilentEngine`; switching it is Plugin Wrapper's
  step.
- `migration/DIVERGENCES.md` D9 (unsupported `clone()` panics) now lists
  `SynthVoiceHandler` (upstream asserts and returns `nullptr`) and
  `SoundEngine` (upstream inherits `SynthModule::clone`, which slices it).
- `aarch64-semantics` also forwards to `sas-modulation` and `sas-common`,
  now direct dependencies that have it.

**Not in this slice:** modulation routing (`connectModulation`,
`disconnectModulation`, the modulation-change queue, mono vs poly
destination choice and the drop to audio rate) is slice B; a loaded preset's
modulations are not connected yet, and its line mappings are reset to
linear. `getEqualizerMemory`, the oscilloscope/audio memory and MIDI learn
are GUI-side.

**Parity**

- `render/init_saw__single_note`, `__chords`, `__single_note_48k_b128` and
  `render/unison_pad_fx__chords` pass the render tolerances (`rms_diff_db`
  0.01, `residual_db` −50, `lsd_db` 0.1):
  - `init_saw__single_note`: max abs error 1.2e-7, residual −135.6 dB;
  - `init_saw__single_note_48k_b128`: 1.2e-7, −135.3 dB;
  - `init_saw__chords`: 3.1e-3, −69.2 dB;
  - `unison_pad_fx__chords` (second oscillator, unison, chorus, delay and
    reverb; no modulations): −61.4 dB, the documented proxy level for the
    case. It is the effect chain's coverage: bypassing the chain fails it.
- `engine/synth_voice_handler`: 13 cases, bit-identical, including the
  un-parked `mono_retrigger` and `poly_overflow` (they needed the voice
  killer) and the new `chord_progression_engine_rate`, which replays the
  render's chord MIDI at the engine's internal rate (oversample 2, default
  controls, as `init_saw`) so the render's voice allocation is pinned:
  active and pressed voices match through all four chords.
- Dropping `|| !legato()` from `noteOn`'s retrigger condition now fails
  `mono_retrigger` (the gap recorded with #45).

**`init_saw__chords` against the official reference.** The 3.1e-3 /
−69.2 dB is the official reference's own fast-math noise for this case, not
a port drift: the port is **bit-identical to the `REF_FAST_MATH=OFF`
reference on all four renders** (checked by Graph Reviewer), and
`migration/decisions/reference-determinism.md` records exactly this proxy
noise for `init_saw__chords` (a phase increment rounded differently by the
fast-math build, drifting from the fourth chord at 2.25 s). No follow-up.
