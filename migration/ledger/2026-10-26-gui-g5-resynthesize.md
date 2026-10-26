## 2026-10-26: GUI G5 (3), Resynthesize Preset to Wavetable

**Changed.** The wavetable view's menu gets upstream's "Resynthesize Preset
to Wavetable" after Initialize. It renders the current patch playing a low
note and turns the audio into the oscillator's wavetable.

- `sas_plugin::resynthesis`:
  - `render` ports `SynthBase::renderAudioForResynthesis` step for step:
    `allSoundsOff`, 1 s of running in from -1 s in 64-sample blocks, note
    16 at velocity 0.7 for 4 s, the left channel normalized to its peak
    (at least 0.01), and `allSoundsOff`.
  - `creator_from_audio` ports `WavetableEditSection::resynthesizeToWavetable`:
    the oscillator's creator cleared, `initFromAudioFile` pitched with wave
    blending, named "Resynthesize", the file source's window one period of
    note 16.
  - `Resynthesizer` runs these on a thread of its own, started by the first
    resynthesis. Each job builds a fresh synth from the patch with the
    thread's own seed counter restarted at 0 (`isolate_seeds`, which also
    makes sure the process's shared oscillator table exists first, as L6's
    preview engine does), and loads the result through
    `EditHandle::set_wavetable`. The process's counter never moves, and the
    audio thread is untouched.
  - Dropping it (the plugin unloading) stops a running render at its next
    64-sample block and skips queued jobs, as #102 does for the preview.
    Each job runs under `catch_unwind`, so a panic loses only its own
    result, and `wait_idle` still returns.
- `EditorModel::resynthesize_to_wavetable(osc)` returns at once. The
  plugin starts the job with the live patch (`SynthParams::preset_state`)
  at the host's sample rate, which `SynthParams` now keeps from
  `activate`.
- `Synth::note_on` wraps `SoundEngine::noteOn`, so the note's velocity is
  upstream's 0.7, not a MIDI byte.
- D22 records what differs from upstream's live-engine render: a fresh
  synth from rest with its own seeds. Randomness can sound different;
  tempo-synced and other patches render the same.

**Verified.**

- `resynthesis` unit tests (each test seeds its thread as the worker
  does, so they pass in any order in parallel runs):
  - the render is 4 s, normalized to a peak of 1, sounding from the note
    on, and the same patch from the same seeds renders the same;
  - the creator is one pitched file source named "Resynthesize", its
    window one period of note 16;
  - a render stops at the block after it's told to;
  - a panicking job leaves the worker running the next;
  - dropping the resynthesizer stops the running job and skips the queued
    ones.
- `tests/resynthesis_seeds.rs` (its own binary, since the counter is
  process-wide): two resyntheses load "Resynthesize" into oscillators 2
  and 3, leave oscillator 1 alone, move the revision, give both the same
  table, and leave the process's seed counter where it was. Taking the
  worker's own counter out fails the test.
- `tests/processor.rs`: the processor keeps the host's tempo (140 BPM
  after a block reporting it, and still after a block without one).
- `tests/controls.rs`: the menu lists Resynthesize last, and choosing it
  reaches the model with no host edit.
- `cargo clippy --workspace --all-targets -D warnings`, `cargo fmt`, and the
  sas-gui and sas-plugin tests pass.

**Host checks.** None new. The render runs off the audio thread, so
playback doesn't stop while it runs, in FL Studio or REAPER.
