## 2026-10-23: LLM L6, the listen loop (`audition`)

**Why.** L6 of the owner-approved LLM plan (§5, "Optional listen loop"):
the model can listen to the patch it is editing. A second engine renders a
short preview off the audio thread and returns compact analysis, never
audio (the owner's working default). Stacked on #101 (L5).

**Changed**

- `sas-llm-control`:
  - `listen`: `AuditionRequest` (MIDI notes played together, how long
    they're held, the tail after release, velocity) with its limits: at most
    8 notes, 4 s held and 2 s of tail. Lengths are clamped; bad notes or
    velocities are errors. The `Auditioner` trait is what a renderer
    implements; this crate still has no engine dependency.
  - `Controller::with_auditioner`, and `Controller::specs`: the core tools,
    plus `audition` when the controller has an auditioner. The agent sends
    `controller.specs()`, so sessions without a renderer (the replay tests)
    are unchanged.
  - The `audition` tool takes a snapshot of the patch and returns the
    analysis. It is not an edit: no undo step, nothing changes.
- `sas-llm-listen` (new crate):
  - `PreviewEngine`: a second `Synth` on a thread of its own, built at the
    first audition and reused. It runs on neither the audio thread nor the
    GUI thread, and its caller (the chat worker) waits for the reply.
    - Each audition loads the snapshot (which resets voices and effect
      tails) and renders at 44.1 kHz in 128-sample blocks.
    - CPU is bounded: at most 6 s of audio, and a render past its
      wall-clock budget (3 s by default) stops there, is analysed as far as
      it got, and says so (`cut_short`).
    - A stop flag ends a render within one block: the engine's own on drop,
      or its owner's through `stopped_by`. The engine joins its thread on
      drop.
    - Building the synth takes seeds from the process's seed counter, as
      opening another instance does upstream. Nothing in the crate calls
      `set_next_seed`.
  - `analysis::analyze`, about 180–300 tokens of JSON:
    - level: peak and RMS in dB, clipped and non-finite sample counts;
    - envelope: attack to within 3 dB of the held peak, the peak's time,
      sustain relative to the peak, decay to within 1 dB of the sustain,
      and release to 40 dB below the level at release (`null` if still
      sounding);
    - pitch: McLeod's normalised square difference over 4096 samples of
      the held part (30 Hz to 2 kHz), as Hz and MIDI note with cents, with
      clarity and the harmonic-to-noise ratio it implies. A chord reads as
      its common period; noise has no pitch;
    - stereo: L/R correlation and side-to-mid level;
    - spectrum: centroid and 85% rolloff per time slice (`null` for slices
      60 dB below the loudest), and a 16-band × 8-slice spectrogram, log
      bands from 40 Hz to 16 kHz, one digit per cell (9 = loudest, 6 dB per
      step).
- `sas-plugin`: the chat worker's controller gets a `PreviewEngine`
  stopped by the worker's shutdown flag. The worker drops its agent, which
  joins the preview thread, before signalling that it has ended, so plugin
  drop also waits for the preview thread.
- `sas-llm` (CLI): sessions and evals get a `PreviewEngine` too.
- `migration/DIVERGENCES.md` D13 extended.

**Measured**

- `cargo test -p sas-llm-listen`, 17 tests:
  - 7 analysis tests on signals with known answers:
    - a 440 Hz sine's pitch (±1 Hz, clarity > 0.99), level, attack,
      release, correlation, centroid and spectrogram band;
    - a slow attack and a decay to a −6 dB sustain;
    - noise (no pitch, centroid above 8 kHz, wide independent channels);
    - inverted channels, silence, clipped and NaN samples;
    - the JSON staying under 1300 characters.
  - 8 preview tests (plus one ignored, which prints an example):
    - the init patch plays A4 at 440 Hz (±2) and A3 at 220 Hz on one
      reused engine;
    - repeated auditions agree within 0.5 dB, and a closed filter lowers
      the centroid;
    - a zero budget cuts the render short and says so;
    - a full 6 s, 8-note preview fits the default budget;
    - drop ends the thread, and so does a set stop flag;
    - `audition` is offered only with an engine, changes nothing and
      rejects bad notes;
    - no source in the crate calls `set_next_seed(`.
  - The seed test, alone in its own binary: building the preview synth
    takes seeds, a second audition takes none, and the counter never moves
    back.
  - `fft`: a sine lands in its bin.
- `sas-llm-control --test listen`: 3 tests for the request's defaults,
  clamping and errors.
- `sas-plugin --test chat`: the model auditions through the plugin's chat
  without an error, an undo step, or a change to the synth.
- Example (the init patch, C–E–G, 0.6 s held): 708 characters.
- Clippy `-D warnings` and rustfmt are clean for the workspace, for
  `sas-llm-agent` with `http` and for `sas-plugin` on
  `aarch64-apple-darwin`.

**Decisions and follow-ups**

- The preview renders at 44.1 kHz whatever the host's rate. The features
  don't depend on it, and the cost stays fixed.
- Stop doesn't interrupt a preview in progress; it ends with the message
  after the model call, and a render takes at most 3 s.
- The chat panel has no Listen button yet (LLM plan §6). The model calls
  `audition` itself; a button for the user can come with G7's drawer.
- Opt-in audio sent to audio-capable models stays parked (I15).

**Blocked:** nothing. Merge waits for #101, which waits for #100, #95 and
#92 (Salil's rule-3 OK).
