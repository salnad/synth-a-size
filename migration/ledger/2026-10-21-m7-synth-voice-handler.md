## 2026-10-21: M7 PR 3, `SynthVoiceHandler` note articulation

Follows M7 PR 1 (#21) and builds on #26 (dsp-core `RouterHooks` forwarding
of idle adds and the sample rate) and #7 (`sas-preset`, for parameter
details), all merged. #26's end-to-end coverage is still open (below).

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
- 10 cases in `migration/corpus/modules/engine/synth_voice_handler.json`:
  - chord with zoned bends, bend range, tune, transpose and macros;
  - poly portamento, and portamento with force, scale and negative slope
    (overlapping notes);
  - `portamento_force_lone_notes`: each note released before the next, so
    only `portamento_force` makes a lone note glide;
  - mono legato glide with pitch bend (block 64 and 128), which releases
    the older, held note;
  - `mono_legato_release_playing`: legato with `voice_override` steal,
    releasing the *playing* note while another is held (no voice is killed),
    so `note_retrigger`'s `noteOff` condition is decided by `!legato()`;
  - MPE local bend, slide and pressure on member channels;
  - poly releases with lift;
  - portamento at oversample 2.
- Mutations checked: dropping `!legato()` from `noteOff`'s retrigger
  condition fails `mono_legato_release_playing`; unplugging
  `portamento_force` fails `portamento_force_lone_notes`. Dropping
  `|| !legato()` from `noteOn`'s condition still passes: that path only
  differs with legato off and the voices full, i.e. in `mono_retrigger`
  (pending below), so it is covered only once that case lands.

**Metrics:** 10 cases × 2 arrays, all **bit-identical** (max abs error 0).

**Open: #26's end-to-end coverage**

These cases do not cover #26's `RouterHooks` forwarding. Reverting either
half of #26 in dsp-core (the idle forward in `add_idle_processor`, or the
`set_sample_rate` before forwarding `add_processor`) still passes them:
`createPolyModControl`'s idle switches change behaviour only once a
modulation is connected, and the add-time sample rate is overwritten by
`setSampleRate` afterwards. That coverage stays open for the modulation bank
PR, where a connected poly modulation makes the switch's ownership
observable.

**Pending (held until the voice killer exists)**

- `mono_retrigger` (polyphony 1, no legato, `voice_override` kill) and
  `poly_overflow` (polyphony 2, four overlapping notes) both kill voices.
  Their inputs are parked in
  `migration/corpus/modules/engine/pending/synth_voice_handler.json`.
  Upstream frees a killed voice through the amplitude envelope's fast kill,
  whatever the release time; without the envelope the port keeps it active.
  Measured on `block_state`: `mono_retrigger` matches the reference until
  the first killed voice is freed (block 55), then its active-voice count is
  one higher (max abs error 3); `poly_overflow` likewise from block 52 (max
  abs error 3). These cases are added with the modulation crate's envelope.
