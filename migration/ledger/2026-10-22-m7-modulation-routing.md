## 2026-10-22: M7 slice B, modulation routing

Follows slice A (#75). Voice Engine.

**What changed**

- `sound_engine.rs`: `SoundEngine::connect_modulation` and
  `disconnect_modulation` (`synth_engine/sound_engine.cpp`), as written:
  - the source goes into the connection's `ModulationConnectionProcessor`,
    which plugs into the poly destination when the source is polyphonic and
    the destination has one (the voice handler's non-accumulated output is
    activated for it), the mono destination otherwise;
  - the source and the processor drop to audio rate when the destination
    and the source are both audio-rate; on disconnect the processor returns
    to control rate, and the source too when no other audio-rate connection
    uses it (`num_audio_rate`);
  - the mono and poly modulation switches turn on with the first
    connection and off with the last;
  - the processor and destination are primed with `process(1)`.
- `synth.rs` (`synth_base.cpp`): `createModulationChange`,
  `isInvalidConnection` (a modulation of its own amount clears the slot),
  `connectModulation` and `disconnectModulation` by name, the
  modulation-change queue (applied at the start of each rendered block, as
  `processModulationChanges`; on the plugin path, before the next
  `correct_to_time` or `process`), `clearModulations`, and
  `LoadSave::loadModulations` in `load_preset` (bank slot by slot, unknown
  sources and destinations skipped, line mappings loaded).
- Corpus: an authored preset `velocity_filter_poly` (oscillator 1 into the
  analog low-pass, velocity → `filter_1_cutoff`) and MIDI `velocity_overlap`
  (a chord whose notes differ only in velocity, then overlapping notes with
  contrasting velocities), from `generate.py`, rendered as
  `render/velocity_filter_poly__overlap` with its x86 CI-identity hash.
  Voices that sound together carry different poly modulation.
- `tests/voice_handler_routing.rs`: #26's coverage (below).

**Parity**

Every `renders` case in `cases.json` passes and is **bit-identical to the
`REF_FAST_MATH=OFF` reference** (max abs 0):

| Case | Residual vs official |
|---|---|
| `init_saw__single_note` | −135.6 dB |
| `init_saw__chords` | −69.2 dB |
| `init_saw__single_note_48k_b128` | −135.3 dB |
| `unison_pad_fx__chords` | −61.4 dB |
| `analog_filter_lfo__chords` | −81.7 dB |
| `analog_filter_lfo__velocity_96k_b512` | −122.3 dB |
| `mono_legato_glide__legato` | −66.2 dB |
| `line_wavetable_fold__velocity` | −137.6 dB |
| `velocity_filter_poly__overlap` | passes |

The official-reference residuals above about −100 dB are the official
build's own fast-math noise (`decisions/reference-determinism.md`).
Forcing every connection to the mono destination fails
`analog_filter_lfo__chords` and, on its own, `velocity_filter_poly__overlap`
(−2.7 dB).

**#26's end-to-end coverage**

Reverting either half of #26 (the idle forward in `add_idle_processor`, or
the sample rate set before forwarding an add) leaves **every render
bit-identical**, `velocity_filter_poly__overlap` included: in the port,
neither forward changes any output. What #26 guarantees is structural, so
`tests/voice_handler_routing.rs` checks it:

- `idle_processors_are_forwarded_to_the_voice_router`;
- `forwarded_processors_take_the_handlers_sample_rate` (adds and idle adds
  after a sample-rate change);
- `poly_modulation_switches_belong_to_the_voice_router`: on the full
  `SynthVoiceHandler`, every poly modulation switch lives inside the voice
  router (the handler's own directly, the modulators' and filters' through
  their sub-modules), none in the global router.

Reverting the idle forward fails all three; reverting the sample rate fails
the second.
