## 2026-10-21: M7 PR 3, `SynthVoiceHandler` note articulation

Follows M7 PR 1 (#21) and builds on #26 (dsp-core `RouterHooks` forwarding
of idle adds and the sample rate) and #7 (`sas-preset`, for parameter
details), all merged.

**What changed**

- `port/crates/engine/src/synth_voice_handler.rs` ports
  `modules/synth_voice_handler.{h,cpp}` as far as the merged crates allow:
  - the constructor's registered outputs (`output_`, `direct_output_`,
    `midi_offset_output_`);
  - `createNoteArticulation`: `PortamentoSlope`; `pitch_bend_range` ×
    pitch wheel, local MPE bend, `voice_tune` and `voice_transpose` into
    the bent MIDI note; the `note` percentage; the note relative to the
    reference;
  - the macros, the `stereo`, `note` and expression modulation sources and
    status outputs, `num_voices`, and `setupPolyModulationReadouts`;
  - `noteOn`/`noteOff` with `note_retrigger`, and `process` (feedback reset
    and status output update).
  The producers, LFOs, envelopes, random sources, filters, voice output
  stage, voice killer and modulation bank follow their crates.
- `aarch64-semantics` also forwards to `sas-effects`, `sas-oscillators` and
  `sas-preset`, which have the feature on main.
- `parameters.rs`: `sas-preset`'s registry as dsp-core's `parameter_details`
  (`Parameters::getDetails`).
- `VoiceHandler::register_output` / `register_control_rate_output` gain
  kernel-side forms, for registering from inside `init`.

**Reference capture**

- `migration/reference/src/modules/engine/synth_voice_handler.cpp` builds the
  **full upstream** `SynthVoiceHandler` as `SoundEngine::init` does
  (`setPolyphony`, inputs plugged, `init`, `disableUnnecessaryModSources`,
  oversample, sample rate), sets the case's controls,
  `updateAllModulationSwitches`, and replays an event script. It captures,
  per block:
  - active voices, pressed notes, `midi_offset_output`, and the
    `note_retrigger` trigger (read before processing);
  - 14 status outputs: the `note` and expression sources, `stereo`,
    `num_voices` and the 4 macros.
  It calls `prepareDestroy()` before deleting the handler, as
  `~SoundEngine` does (without it, teardown crashes).
  `hold_released_voices` sets `env_1_release` to its maximum (~32 s), so
  the upstream envelope frees no released voice within a case; the port has
  no voice killer yet.
- 8 cases in `migration/corpus/modules/engine/synth_voice_handler.json`:
  - chord with zoned bends, bend range, tune, transpose and macros;
  - poly portamento, and portamento with force, scale and negative slope;
  - mono legato glide with pitch bend (block 64 and 128);
  - MPE local bend, slide and pressure on member channels;
  - poly releases with lift;
  - portamento at oversample 2.
- These cases are also the end-to-end coverage of #26's hook forwarding
  (`createPolyModControl`'s idle switches, mono controls to the global
  router).

**Metrics:** 8 cases × 2 arrays, all **bit-identical** (max abs error 0).

**Pending (cases authored, held until the voice killer exists)**

- Mono retrigger (polyphony 1, no legato) and polyphony overflow both kill
  voices. Upstream frees a killed voice through the amplitude envelope's
  fast kill, whatever the release time; without the envelope the port keeps
  it active. Measured: `mono_retrigger` matched the reference until the
  first killed voice was freed (block 55), then its active-voice count was
  one higher (max abs error 6, on `block_state`). These cases are added with
  the modulation crate's envelope.
