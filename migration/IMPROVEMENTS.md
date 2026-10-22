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
