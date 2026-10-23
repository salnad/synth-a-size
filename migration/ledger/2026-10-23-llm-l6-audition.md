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
  - `PreviewEngine`: a second `Synth` on a thread of its own, which starts
    at the first audition. It runs on neither the audio thread nor the GUI
    thread, and its caller (the chat worker) waits for the reply.
    - Seeds: the thread has its own seed counter (below), so the process's
      counter never moves. Each audition builds a fresh synth (about 35 ms)
      from that counter restarted at 0, then loads the snapshot and renders
      at 44.1 kHz in 128-sample blocks. So an audition never depends on
      earlier ones: the same patch gives the same analysis, random
      modulation included. Before each build the thread takes the
      process-wide `RandomValues` table's one-time seed from its own
      counter (a no-op once taken, which in the plugin the host's synth
      always has), so every build starts the same way.
    - CPU is bounded: at most 6 s of audio, and a render past its
      wall-clock budget (5 s by default; a full-length preview of a heavy
      patch took 2.6 s) stops there, is analysed as far as it got, and says
      so (`cut_short`).
    - Stopping ends a render at the next block (about 3 ms of audio): the
      engine's own flag on drop, or its owner's check (`stopped_by`).
      Building the synth, loading the patch and the analysis (about 0.1 s
      together) run to completion. The engine joins its thread on drop.
    - Nothing in the crate calls `set_next_seed`.
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
- `sas-common`: `random::use_thread_seed_counter` gives the calling thread
  a seed counter of its own; `take_next_seed` draws from it when set, else
  from the global counter as before. Opt-in, so no other thread changes.
- `sas-plugin`: the chat worker's controller gets a `PreviewEngine`
  stopped by the worker's shutdown flag or the current message's cancel
  flag, so the user's Stop also cuts an audition short. The worker drops its agent, which
  joins the preview thread, before signalling that it has ended, so plugin
  drop also waits for the preview thread.
- `sas-llm` (CLI): sessions and evals get a `PreviewEngine` too.
- `migration/DIVERGENCES.md` D13 extended.

**Measured**

- `cargo test -p sas-llm-listen`, 18 tests:
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
    - drop ends the thread;
    - `audition` is offered only with an engine, changes nothing and
      rejects bad notes;
    - no source in the crate calls `set_next_seed(`.
  - The seed test, alone in its own binary: three auditions, two of them
    on a patch whose Audio File Source has no `random_seed`, leave the
    global counter where it was. Without the thread counter the same
    auditions moved it by 490.
  - Two auditions of a patch with a random LFO on the cutoff give the same
    analysis, with an unrelated audition between them and on a fresh
    engine, under 16 parallel test threads.
  - An owner's stop check ends a render after 20 blocks, and the engine
    then auditions again.
  - `fft`: a sine lands in its bin.
- `sas-common`: a thread's own counter draws its own sequence, restarts
  when installed again, and leaves other threads on the global counter.
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
- If a preview were the first synth built in a process (the `sas-llm` CLI,
  which has no host synth), it would take `RandomValues`' one-time seed
  from its own counter, and a host synth built later in that process
  would not take it from the global counter. The plugin always builds the
  host's synth first.
- The chat panel has no Listen button yet (LLM plan §6). The model calls
  `audition` itself; a button for the user can come with G7's drawer.
- Opt-in audio sent to audio-capable models stays parked (I15).

**Review (#102).** Parity Reviewer requested changes at 3fc1934: the
preview wasn't seed-isolated, since any seed drawn on the preview thread
(building its synth, or a file source built from JSON on a later load)
came from the global counter, so a session with auditions gave later
loads different seeds than one without. Fixed with the opt-in thread
counter, installed by the preview thread; the global counter is never
saved and restored. Also taken: the reused synth carried random-source
state across auditions, so each audition now builds a fresh one.

Graph Reviewer approved at 3fc1934, with three notes, all taken: a user's
Stop cuts an audition short; the stop claim says it covers rendering, not
building, loading or analysis; and the budget went from 3 s to 5 s, since
a heavy patch took 2.56 s.

**Blocked:** nothing. Merge waits for #101, which waits for #100, #95 and
#92 (Salil's rule-3 OK).
