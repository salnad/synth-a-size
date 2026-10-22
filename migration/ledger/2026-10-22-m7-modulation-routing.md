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
  Every corpus preset needs a load/save case, so `velocity_filter_poly` is
  also in `modules/preset/state.json` (state and roundtrip artifacts, x86
  hashes).
- `tests/voice_handler_routing.rs`: #26's coverage (below).
- `tests/modulation_routing.rs` and the reload renders: disconnect coverage
  (below). `VoiceHandler::is_nonaccumulated_output_active` (a read-only
  query) supports it.
- `tests/render.rs` serialises its renders behind one lock: the engine
  RNG's seed counter is process-wide, and its three tests would otherwise
  race on it.

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

**Disconnect coverage**

Disconnection runs on every preset change (`clearModulations`), so it is
covered from both sides:

- **Reload renders** (`tests/render.rs`), each against the case's own
  reference: A→B, loading `analog_filter_lfo`, `mono_legato_glide`,
  `line_wavetable_fold` or `velocity_filter_poly` before
  `init_saw__chords`, `unison_pad_fx__chords` and
  `analog_filter_lfo__chords` (12 renders); A→B→A, the case's preset, then
  `init_saw`, then the case's preset again, for all five modulated cases.
  The earlier presets' modulation changes are applied before the next load,
  as a block boundary does, so the later load disconnects real connections.
  All pass. Leaving a disconnected source at audio rate (the control-rate
  restore) fails `line_wavetable_fold__velocity`'s A→B→A.
- **Structural** (`tests/modulation_routing.rs`): disconnect zeroes the
  destination scale first, so these are invisible in audio and asserted
  directly, for `lfo_1` → `filter_1_cutoff`, `env_2` → `osc_1_level`,
  `velocity` → `filter_1_cutoff`, `macro_control_1` → `volume` and
  `lfo_1` → `voice_tune`: after disconnecting, the destinations' connected
  inputs, both switches and the poly output's active state are back as
  before the connection, and the connection processor is disabled and at
  control rate. Removing the unplug, either switch reset, the inactive
  poly output, the disable, the processor's return to control rate or the
  source owner's return to control rate each fails it.
- Upstream behaviour the test follows rather than "switches off": an
  audio-rate poly destination is a `ModulationSum` whose reset input counts
  as connected, so upstream's `poly connectedInputs() == 0` never holds for
  it; its switches stay on and its poly output stays active after a
  disconnect, as `updateAllModulationSwitches` already leaves them. The
  control-rate `voice_tune` and the mono-only `volume` do reset. The
  source owner's return to control rate is asserted for `lfo_1`:
  `LfoModule::setControlRate` sets the module's own flag. An envelope
  module only forwards to its inner envelope, so its owner's flag cannot
  show the restore. Removing the restore fails the `lfo_1` assertion;
  inverting it also fails `line_wavetable_fold__velocity`'s A→B→A render.
