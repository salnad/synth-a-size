# Possible improvements beyond parity

**These are parked ideas, not work to do.** The priority is a port that
stays as close to 1:1 with upstream as possible
([AGENTS.md](AGENTS.md) rule 5; [PLAN.md](PLAN.md)). This list records
ideas that came up along the way, so they are not lost and not slipped
into code. Taking one up needs an owner decision. Any resulting behaviour
change also needs an entry in [DIVERGENCES.md](DIVERGENCES.md).

The owner asked for this list (Salil, 2026-10-22). It is kept separate from
DIVERGENCES.md, which lists differences the port *already has*, and from
the decision records in `decisions/` and `ledger/`.

Each entry gives the idea, a one-line why, the tradeoff (including any
parity cost), and where it came from.

## I1. Per-sample host automation

- **Idea:** apply host automation sample-accurately. nice-plug's
  `SAMPLE_ACCURATE_AUTOMATION = true` splits each host block at every
  automation point.
- **Why:** fast automation (filter sweeps, gated volume) would track the
  host's curve more tightly than once per block.
- **Tradeoff:** upstream applies only the last point of each parameter
  queue, once, before `processBlock` (JUCE's VST3 wrapper,
  `processParameterChanges`), then smooths. Per-sample automation changes
  the engine's chunking and the values inside a block, so heavily
  automated projects would no longer render bit-exactly like upstream.
- **Status:** the default stays matching upstream,
  `SAMPLE_ACCURATE_AUTOMATION = false` (#79). The test
  `automation_applies_once_per_block_like_upstream` pins it. The owner
  resolved the automation-timing question that way on 2026-10-22; see
  `ledger/2026-10-22-owner-approval-automation-timing.md` and
  `decisions/plugin-framework.md`.
- **Source:** `ledger/2026-10-22-m8-plugin-sound.md` (Chain Reviewer on
  #79); `port/crates/plugin/src/lib.rs`.

## I2. FMA contraction and vDSP for speed (M11)

- **Idea:**
  - let the compiler fuse multiply-adds, or use `mul_add` / `vfmaq_f32` at
    hot sites;
  - use Accelerate/vDSP for the FFT on macOS, as upstream's real Mac
    build does.
- **Why:** fewer instructions in the DSP inner loops and a faster FFT on the
  shipping target.
- **Tradeoff:**
  - It gives up bit-exactness against the pinned reference: −91.2 dB
    (contraction) and −127.5 dB (vDSP) on macOS arm64. That's far inside
    the tolerances, but it breaks the SAS-35 exact comparisons.
  - Results become compiler- and CPU-dependent unless pinned per target.
  - M11 only starts against a passing parity suite.
- **Source:** [DIVERGENCES.md](DIVERGENCES.md) D10; `decisions/reference-arm.md`
  (consequences 2–3, residual risks); [PLAN.md](PLAN.md) M11.

## I3. Gentler handling of inputs that are undefined upstream

- **Idea:** where upstream's behaviour on malformed input is undefined, the
  port currently picks the smallest defined behaviour: an error, `None`, a
  refused load or the table's nearest end. It could instead *repair* such
  input and tell the user.
  - Load the valid parts of a damaged preset instead of none of it.
  - Clamp a keyboard map that runs past its scale.
  - Show the reason in the UI.
- **Why:** a user with a slightly damaged preset or `.kbm` gets a usable
  result instead of a refusal.
- **Tradeoff:**
  - It moves further from upstream on invalid input only. Authored and
    corpus content is unaffected, so there's no parity cost there.
  - Each repair is a new divergence to document and test.
  - Partial loads give states that match neither file, which is why D4
    chose "changes nothing".
- **Source:** [DIVERGENCES.md](DIVERGENCES.md) D4–D7 and D11;
  `ledger/2026-10-21-tuning-kbm-validation.md`.

## I4. Bypass set from the edit controller while processing

- **Idea:** honour `IEditController::setParamNormalized` for the bypass
  parameter even while the plugin is processing. nice-plug ignores it then.
- **Why:** Steinberg's validator logs "The bypass parameter was not correctly
  set" in its bypass-persistence test (the test still passes). A host that
  set bypass only on the controller would not see it take effect.
- **Tradeoff:** it needs a change in nice-plug, either a fork or an upstream
  contribution, and agents may not open upstream PRs without a human.
  Hosts deliver parameter changes through `process()`, so FL Studio is
  unaffected today. There is no audio parity cost.
- **Source:** `ledger/2026-10-22-m8-plugin-sound.md` (validator run, #79);
  #59's build logged the same message.

## I5. Ship and test CLAP

- **Idea:** test and ship the CLAP build alongside VST3. It is already
  exported but neither tested nor shipped.
- **Why:** CLAP hosts (Bitwig, Reaper and others) would get a native
  plugin, with per-note expressions and polyphonic modulation available
  later.
- **Tradeoff:**
  - It adds a second format to validate in CI.
  - CLAP can address notes to every channel or key, which the port drops
    today (D2). Supporting them would mean new behaviour with no
    upstream counterpart.
  - Upstream has no CLAP build, so there's nothing to compare against
    beyond the shared engine.
- **Source:** `decisions/plugin-framework.md` (formats);
  `ledger/2026-10-21-m8-plugin-wrapper.md` ("CLAP export stays because it
  costs nothing; it is not tested or shipped"); [DIVERGENCES.md](DIVERGENCES.md) D2.

## I6. One platform-independent, higher-precision libm

- **Idea:** compute the float transcendentals (`sinf`, `expf`, `powf`, ...)
  with one pure-Rust, ideally correctly rounded, implementation on every
  target. The double-precision ones already use the pure-Rust `libm` crate.
- **Why:**
  - The same input would give the same output on x86 and macOS, and
    across macOS versions.
  - It would remove Apple libm drift, such as `powf` differing between
    macOS 14 and 15, and the glibc-vs-Apple low-bit differences.
- **Tradeoff:**
  - It gives up matching each platform's reference, which calls the
    platform libm as upstream does. Today that is −95 dB worst, within
    tolerance, but exact comparisons on macOS would break.
  - It may also be slower.
- **Source:** `decisions/reference-arm.md` ("Apple libm vs glibc",
  consequence 4); `migration/corpus/macos_libm_drift.json`;
  `migration/LEDGER.md` (M4a wavetable state: double transcendentals via
  the `libm` crate).

## I7. Prepare heavy preset loads off the audio thread

- **Idea:** build a heavy engine state (wavetables, samples) for a preset
  load on a background thread and swap it in. Today the load runs with
  processing paused, as upstream does.
- **Why:** no audio dropout or CPU spike in the audio callback when a
  project or preset loads.
- **Tradeoff:**
  - It needs a lock-free swap and a defined order between a pending swap
    and parameter changes.
  - The rendered audio after the swap must stay identical to a paused
    load, so there's no parity cost if done right, but there's a new
    race surface.
- **Source:** `ledger/2026-10-21-m8-plugin-wrapper.md` (follow-ups: "A heavy
  engine load should be prepared off the audio thread and swapped in").

## I8. Effects-only plugin

- **Idea:** port upstream's audio-input effects engine,
  `vital/src/synthesis/effects_engine/`. It has the full reorderable
  effect chain, a filter, LFOs, envelopes, random LFOs, macros and the
  modulation matrix, all driven by MIDI, but no oscillators.
- **Why:** it would be a second product, an effect plugin, built from blocks
  that are already ported.
- **Tradeoff:**
  - It needs its own plugin wrapper, parameter set, GUI, and a name of its
    own that is not upstream's.
  - There is no official reference build. The Makefile's `effects_*`
    targets build from an `effects/` project folder that is missing from
    the open-source release. Parity would need a reference built by the
    harness.
- **Status:** the owner kept it out of scope as a parked idea (Salil,
  2026-10-22).
- **Source:** `vital/src/synthesis/effects_engine/` (`sound_engine.h`,
  `effects_modulation_handler.h`); `vital/Makefile` (`effects_lv2`,
  `effects_vst`, `effects_vst3`).

## I9. Cross-crate inlining in release builds (LTO)

- **Idea:** build `sas-bench` and the plugin with `lto = "fat"` and
  `codegen-units = 1`, or mark the small `sas-simd` helpers `#[inline]`.
  The hot helpers live in `sas-simd` and are called from other crates
  (`sas-oscillators`, `sas-filters`). Without LTO or `#[inline]`, Rust
  does not inline a non-generic function across crates.
- **Why:** measured with the benchmark (48 kHz, 256-sample blocks, this
  repo's Linux x86_64 VM), fat LTO alone takes the port from 1.40–1.64× to
  1.14–1.53× the official reference's render time, a gain of 0–12 % by
  scenario. The output hashes are unchanged in every scenario.
- **Tradeoff:**
  - No parity cost: Rust does not reassociate or contract floating point
    when it inlines, so the output stays bit-identical. The parity suite
    still has to confirm that per target.
  - Longer release builds.
  - It closes only part of the gap; I10 and I11 are the larger,
    site-specific items.
- **Source:** `ledger/2026-10-22-perf-benchmark.md`; `migration/BENCHMARKS.md`.

## I10. Keep the oscillator's wavetable interpolation in registers

- **Idea:** make `SynthOscillator`'s per-voice interpolation path
  (`interpolate_multiple_buffers`, `poly_utils::get_value_matrix_separate`,
  `get_catmull_interpolation_matrix`, `Matrix::new`) compile to one inlined
  loop, as GCC does for upstream's `processOscillators`. This means
  `#[inline]`/`#[inline(always)]` on the helpers and avoiding the
  intermediate `Matrix` values.
- **Why:** this is the largest part of the port's gap. In
  `poly16_unison`, the oscillator outside its FFT takes about 2.4× the
  C++ time. The two matrix helpers show up as separate out-of-line
  symbols in `perf`.
- **Tradeoff:** no parity cost if the arithmetic and its order are kept,
  and bit-exact parity tests guard it. The cost is more code and more
  `unsafe`-free SIMD plumbing in `sas-simd`.
- **Source:** `ledger/2026-10-22-perf-benchmark.md` (perf profiles of
  `poly16_unison`, Rust and C++).

## I11. Inline the ladder filter's saturator

- **Idea:** inline `futils::algebraic_sat` into `LadderFilter::process`.
  Today it is an out-of-line call per sample and stage.
- **Why:** at 8x oversampling, `algebraic_sat` alone is 17 % of the
  port's render time, and the ladder filter costs about 1.6× upstream's,
  where GCC inlines the saturator. `oversampling_8x` has the widest
  Rust ÷ C++ ratio, 1.6–1.9×.
- **Tradeoff:** no parity cost (same arithmetic). Covered by I9 if LTO is
  adopted, otherwise one `#[inline]`.
- **Source:** `ledger/2026-10-22-perf-benchmark.md` (perf profile of
  `oversampling_8x`).

## I12. Fault in delay memory before the first audio block

- **Idea:** allocate effect delay lines (`MemoryTemplate`, used by the
  delay, chorus, flanger and reverb) with their pages already touched,
  for example by writing the zeros at construction as upstream's
  `std::make_unique<poly_float[]>(n)` does. Today `vec![0.0; n]` takes
  lazily mapped zero pages from `calloc`.
- **Why:** the first block after a synth is built costs about 3 ms in the
  port and 0.7 ms upstream (init patch, 44.1 kHz, 64-sample blocks), and
  up to 13–19 ms at 8x oversampling. That is 2–10× the block's real-time
  budget, so it is a likely dropout on the first block after a preset
  load. 83 % of it is page faults, taken while the effect chain's first
  `enable` → `hard_reset` → `DelayMemory::clear_all` writes those pages
  for the first time. Later note-ons do not spike. This is Linux (glibc)
  behaviour: on the macOS arm64 CI runner the port's first block is no
  slower than upstream's (0.4 ms vs 0.6 ms), so the shipped Mac plugin is
  not affected today. A Linux build or another allocator would be.
- **Tradeoff:**
  - No audio change.
  - The memory is committed when the synth is built (as upstream).
  - The remaining memset in the first block is upstream behaviour: C++
    pays it too.
  - Arguably a real-time-safety fix rather than an optimisation, so it may
    be worth taking before M11 if a Linux build ships.
- **Source:** `ledger/2026-10-22-perf-benchmark.md` (the "first µs"
  column; perf of the first block).

## I13. A faster FFT for the oscillators' spectral transforms

- **Idea:** replace the per-block inverse real FFT that every wavetable
  oscillator voice runs (`spectral_morph::transform_and_wrap_buffer` →
  `FourierTransform::transform_real_inverse`, a port of JUCE's fallback
  FFT) with a faster real FFT: split-radix or SIMD, or vDSP on macOS
  (I2).
- **Why:** it is the largest single cost in both engines for unison
  patches: about 34 % of the port's time and 45 % of upstream's in
  `poly16_unison`. The port's FFT costs about 1.1× the C++ one.
- **Tradeoff:**
  - It changes the last bits of every wavetable oscillator's output
    against the reference, which uses JUCE's fallback FFT. That breaks
    the bit-exact and SAS-35 comparisons; vDSP measured −127.5 dB (I2).
  - Needs an owner decision and a recorded divergence.
- **Source:** `ledger/2026-10-22-perf-benchmark.md` (perf profiles, Rust
  and C++).

## I14. MCP bridge inside the plugin

- **Idea:** let outside agents drive a running plugin instance in FL
  Studio over MCP: a localhost-only server inside the plugin, off by
  default, protected by a random token shown in the chat panel, one port
  per instance. It would use the same tools as the built-in chat, through
  the GUI's edit bus, so undo and the host-automation checks still apply.
- **Why:** agents outside the DAW could shape the live sound, not only
  write preset files.
- **Tradeoff:**
  - An open socket in a plugin is new attack surface, even on localhost.
  - nice-plug's `GuiContext` exists only while the editor is open, so with
    the editor closed parameter edits may not reach the host.
  - No engine or parity cost.
- **Status:** parked. The LLM plan starts with a standalone stdio MCP server
  (`sas-mcp`, milestone L7) over an offline session: agents can build
  patches and write preset files without a socket in the plugin, and it is
  testable in CI.
- **Source:** the owner-approved LLM plan (2026-10-22), whose working
  defaults put MCP second, as a standalone server;
  `ledger/2026-10-23-llm-l1-tool-core.md`.

## I15. Send preview audio to audio-capable models

- **Idea:** in the LLM listen loop (L6), send the short offline preview
  render itself to models that accept audio input, not only the analysis
  features computed from it.
- **Why:** a model that hears the sound may judge timbre better than one
  reading a spectral summary.
- **Tradeoff:**
  - Audio costs far more tokens than the roughly 300-token analysis.
  - It sends audio to a third party, which the working defaults rule out
    ("the listen loop sends analysis only, never audio").
  - Only some providers accept audio. No engine or parity cost.
- **Status:** parked by the owner's working defaults (2026-10-22).
- **Source:** the owner-approved LLM plan (2026-10-22): the listen loop
  renders a short preview offline and sends about 300 tokens of analysis
  (levels, attack and decay, spectral centroid over time, a coarse
  spectrogram) instead of audio; `ledger/2026-10-23-llm-l1-tool-core.md`.

## I16. Note preview through the live engine

- **Idea:** let the LLM layer play a note or phrase through the running
  plugin instance (MIDI injected into the processor), instead of rendering a
  separate offline preview.
- **Why:** the user hears exactly what the model changed, in context, with
  the project's effects and tempo.
- **Tradeoff:**
  - Injected notes mix with the host's MIDI and would be recorded in a
    bounce; it needs a clear UI and a guarantee that nothing plays during
    export.
  - It adds a MIDI path upstream does not have. The engine itself is
    unchanged.
- **Status:** parked. The listen loop (L6) renders offline with a second
  engine, so nothing reaches the host's audio or a bounce.
- **Source:** the owner-approved LLM plan (2026-10-22), which keeps every LLM
  feature away from the live engine's audio path;
  `ledger/2026-10-23-llm-l1-tool-core.md`.

## I18. Allocation-free graph rewiring for modulation connections

- **Idea:** make connecting and disconnecting a modulation allocation-free
  on the audio thread: preallocate each destination's spare inputs and
  feedback nodes, and let the routers rebuild their processing order
  (`update_all_processors`, `create_added_processors`) into reused
  buffers.
- **Why:** with G0's edit bus, drag-to-modulate and chat edits connect
  modulations while the synth plays. Draining the bus and the connection
  bookkeeping are allocation-free; the graph rewiring after them is not.
- **Tradeoff:**
  - Upstream allocates here too (`Processor::plugNext` creates an
    `Input`, `ProcessorRouter::connect` creates a `Feedback` for a cycle,
    `updateAllProcessors` clones processors), so this goes beyond parity.
  - No numeric change if done right, but it reaches into the processor
    graph that every module relies on.
- **Source:** `ledger/2026-10-23-g0-edit-bus.md` (the no-allocation
  stress tests leave the rewiring uncounted).
