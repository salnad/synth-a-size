## 2026-10-23: G0 telemetry snapshot and status-output audit

Third G0 PR, stacked on the edit bus (#94). This is what the editor shows of
the playing engine, read without locks shared with the audio thread.

**Changed**

- `sas_engine::telemetry` (new):
  - **Snapshot.** A triple-buffered `TelemetrySnapshot`. The audio thread
    fills the back buffer and publishes it with one atomic swap; the editor
    takes the newest one (`Telemetry::read`). Readers share a mutex the
    audio thread never touches. Nothing allocates, frees or waits.
  - **Contents:**
    - every status output, by index;
    - each modulation destination's mono and poly totals: the readouts'
      `trigger_value`, as upstream's `ModulationMeter` and
      `Wavetable3d::getOutputsTotal` read them, plus whether the mono
      readout's processor is enabled;
    - the oscilloscope memory.
  - **Fixed tables.** `status_names()` (198) and `destination_names()`,
    with `status_id` and `destination_id`, so the editor resolves ids once
    and reads by index.
  - **`AudioMemory`.** `SynthBase::audio_memory_` (`kAudioMemorySamples`
    frames), a ring of relaxed atomics for the spectrogram, too large to
    copy every block.
- `Synth`:
  - `update_memory_output` ports `SynthBase::updateMemoryOutput` and runs
    after every `process`, as upstream's `writeAudio` calls it. The
    oscilloscope window restarts on a new note, spans whole periods of the
    last note and is sampled every `sample_rate / kOscilloscopeMemorySampleRate`
    frames. Upstream's `int += float` offset update is kept (the sum in
    float, truncated). It reads the output only.
  - `write_telemetry(&mut TelemetryWriter)` fills and publishes a
    snapshot. The status outputs and readouts are cached at construction in
    table order, so writing allocates nothing.
- Plugin:
  - `Engine::audio_memory` and `Engine::publish_telemetry`;
  - `BlockProcessor` publishes at the end of each block that runs the
    engine (not while bypassed; upstream's bypassed block doesn't update
    its memories either);
  - `SynthPlugin::telemetry()` hands out the reader.

**Status-output audit.** The GUI plan counted 20 call sites in the port
against upstream's 32.

- Upstream: `SoundEngine::init` has 11 call sites (six compressor bands,
  the chorus delays loop, phaser, flanger, two peak meters). `SynthVoiceHandler`
  has 21 (macros, random LFOs, random, stereo, sample phase, voice count, the
  slot source and amount pair, three per LFO, two per envelope, eight
  expression sources).
- The port folds the compressor bands and the expression sources into
  loops, hence 20 sites, but registers the same names.
- `tests/telemetry.rs::the_engine_registers_exactly_upstreams_status_outputs`
  walks every module in the engine graph. It finds exactly the 198 names
  upstream's sites expand to, each registered once. Nothing is missing.

**Tests**

- `engine/tests/telemetry.rs`:
  - the audit;
  - every destination has a mono total (more than 300 also have a poly
    one);
  - a published snapshot equals the engine's own status values and readout
    values, for every destination;
  - the audio memory's latest frames equal the rendered output;
  - the oscilloscope window is the output at every other frame (44.1 kHz),
    802 entries for A3: 200.45 frames doubled to 1603.6.
- Unit tests:
  - the newest snapshot wins;
  - a threaded stress test (20 000 publishes against a busy reader) never
    tears a snapshot or goes back in time;
  - the audio-memory ring order and wrap.
- `plugin/tests/edit_bus.rs::each_block_publishes_telemetry`.
- The no-allocation stress tests now count `write_telemetry` (engine, in the
  edit tests and the steady-state routed-preset test) and
  the publish inside `BlockProcessor::process` (plugin): **0 allocations
  and 0 frees.**
- No audio changes: every existing test passes unchanged, parity included.

**Not covered.** The oscilloscope memory has no reference capture. It's
GUI-only data that never reaches the audio, so it's checked structurally
here; the editor's screenshots (G3) compare what it draws.
`SoundEngine::getEqualizerMemory` (the EQ's spectrum) isn't in the snapshot
yet; it arrives with the EQ editor (G6).
