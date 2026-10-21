# Divergences from upstream

Every intentional difference between the Rust port's behaviour and upstream
(the vendored source in `/vital`, as its release build behaves) is listed
here. The owner asked for this file (Salil, 2026-10-21). It is kept separate
from the work log in `migration/ledger/`, so the complete list is in one
place.

Each entry gives:

- what differs;
- upstream's behaviour;
- the port's behaviour;
- why;
- the approving decision or PR.

Any PR that introduces an intentional behavioural divergence adds an entry
here, and reviewers check for it (migration/AGENTS.md).

What is **not** listed:

- **Per-target emulation that matches upstream on each target.** Examples
  are the A1–A12 sites in `decisions/reference-arm.md` (`toInt`,
  float-to-int conversion, the libc++/libstdc++ RNG and `expf` columns) and
  A10's official-build complex division. The port reproduces upstream
  there, so these are not divergences.
- **Test-harness policies.** These are listed under
  [Verification exceptions](#verification-exceptions) because they are not
  port behaviour.

## Port divergences

### D1. Host text entry reads the displayed value, clamped

*Merged in #59.*

- **Differs:** what a value typed into a host parameter's text field sets.
- **Upstream:** `ValueBridge::getValueForText` returns
  `unskew(text / display_multiply)`, an engine-domain value. JUCE's VST3
  wrapper hands it to the host as the *normalized* value
  (`juce_VST3_Wrapper.cpp:529`), and `convertToEngineValue` never clamps it.
  - Typing "72" into Filter 1 Cutoff sets the engine cutoff to
    72 × 128 + 8 = 9224.
  - An option name parses as 0, so it selects option 0.
- **Port:** the typed value is read as the displayed value: `post_offset` is
  removed, the value is divided by `display_multiply`, unskewed and clamped
  to the parameter's range, with stepped controls rounded to a step. An
  option name (case-insensitive) selects the matching option. Implemented
  in `port/crates/plugin/src/params.rs` (`value_from_text`).
- **Why:** upstream's result is an out-of-range bug. nice-plug clamps text
  input to [0, 1] anyway, so the closest reproduction would make every typed
  number above 1 select the maximum. Only the host's parameter text entry
  is affected; engine values, automation and preset state are not.
- **Approved:** owner (Salil), 2026-10-21, relayed by the coordinator.
  Recorded in `migration/ledger/2026-10-21-owner-approval-plugin-text-entry.md`
  and `decisions/plugin-framework.md` ("Host text entry"). Implemented in
  #59.

### D2. Note events addressed to every channel or key are dropped

*Merged in #59.*

- **Differs:** note-on, note-off and poly-pressure events that address all
  channels or all keys.
- **Upstream:** they cannot occur. JUCE's VST3 wrapper always delivers
  concrete MIDI messages with a channel and a key.
- **Port:** such events are dropped (`processor.rs`, `midi_message`). Only
  CLAP can send them, and VST3 always names both.
- **Why:** a wildcard has no 3-byte MIDI equivalent for the engine's
  `MidiManager`. CLAP is exported but neither tested nor shipped.
- **Approved:** #59.

### D3. A host tempo of zero or less is ignored

*Merged in #59.*

- **Differs:** a block whose host transport reports a tempo ≤ 0.
- **Upstream:** `SynthPlugin::processBlock` skips `setBpm` when the tempo
  is 0 but, while playing, still computes the song position as
  `ppqPosition / (bpm / 60)`. A zero tempo therefore makes the clock
  infinite or NaN, and a negative one is passed to `setBpm`
  (`vital/src/plugin/synth_plugin.cpp:151-157`).
- **Port:** a tempo ≤ 0 is ignored. Neither the tempo nor the song position
  changes for that block (`processor.rs`:
  `transport.tempo.filter(|bpm| *bpm > 0.0)`).
- **Why:** there is no meaningful result to reproduce, and an infinite or
  negative clock would corrupt every tempo-synced module.
- **Approved:** #59.

### D4. A preset or host state that fails to load changes nothing

*Merged: preset loading in #7, host state restore in #59.*

- **Differs:** loading a corrupted preset, or one from a newer version.
  This applies to `PresetState` loads and to host state restores.
- **Upstream:** `LoadSave` applies the steps that ran before the failure,
  so the synth is left partially loaded. For a host state, it reports the
  error and keeps going. Some malformed inputs are undefined behaviour
  upstream: more than 64 modulations, more than 3 wavetables or 8 LFOs, a
  modulation without `source` or `destination` (read through a const
  reference).
- **Port:**
  - A failed load leaves the state unchanged. `PresetState` loads into a
    scratch state first.
  - The undefined inputs are ignored or rejected as `Error::Corrupted`
    (`port/crates/preset/src/state.rs`).
  - In the plugin, `filter_state` empties a saved state whose preset can't
    load, so restoring it changes nothing (`port/crates/plugin/src/state.rs`).
- **Why:** a partial load gives a state that matches neither preset.
  Undefined behaviour has no defined output to reproduce. Authored and
  corpus presets never hit these paths: every `error_*` preset fails the
  same way in the reference and the port.
- **Approved:** #7 (preset loading,
  `migration/ledger/2026-10-20-m2-preset-registry.md`) and #59 (host state).

### D5. `create_filter(Comb)` returns `None`

- **Differs:** the generic filter factory for the comb model.
- **Upstream:** `SynthFilter::createFilter(kComb)` builds `CombFilter(1)`.
  Its memory rounds to one sample, and processing reads past the 2-float
  buffer (undefined behaviour).
- **Port:** `synth_filter::create_filter(FilterModel::Comb)` returns `None`
  instead of building a filter that could panic or diverge in `process`.
- **Why:** there is no defined output to reproduce. Nothing in the engine
  uses that path: `FilterModule` builds its comb through `CombModule`, with
  full memory.
- **Approved:** M5a follow-up (Character Filters / Filter Interface),
  recorded in `migration/LEDGER.md` ("M5a PR 1 follow-up,
  `create_filter(Comb)` returns `None`").

### D6. A keyboard map that indexes past the scale panics

*Merged in #24 (tuning files). Follow-up open: reject such a map at load.*

- **Differs:** a `.kbm` whose mapping names a scale degree beyond the loaded
  scale. An example is degrees 9–11 on a 7-degree Scala scale.
- **Upstream:** `Tuning::loadScale` reads `scale[note_in_scale]` past the end
  of the scale vector (undefined behaviour; `vital/src/common/tuning.cpp:340`).
- **Port:** the index is bounds-checked, so loading such a mapping panics
  (`port/crates/engine/src/tuning.rs`, `scale[note_in_scale as usize]`). The authored test inputs
  that hit it were corrected in #24. These are invalid inputs, not narrowed
  coverage.
- **Why:** there is no defined output to reproduce.
- **Approved:** #24 (`migration/ledger/2026-10-21-m7-tuning-files.md`).
  Open follow-up: a panic is not acceptable in a shipped plugin, so the
  loader should reject such a map with an error instead. Update this entry
  when that lands.

### D7. A line generator with `num_points` outside 1–100 is an error

- **Differs:** `LineGenerator::jsonToState` with `num_points` < 1 or > 100.
- **Upstream:** behaviour is undefined. With no points, rendering a looped
  line reads `points_[-1]`.
- **Port:** `LineGenerator::from_json` returns an error and leaves the
  generator unchanged (`port/crates/common/src/line_generator.rs`).
- **Why:** there is no defined output to reproduce.
- **Approved:** #4 (`sas-common`).

### D8. Source formulas instead of the release build's fast-math codegen

- **Differs:** float results wherever upstream's `-ffast-math` release build
  compiles a source formula into something else (reassociation, folded
  checks, inlined approximations).
- **Upstream (release, fast-math):** the codegen differs from the source.
  - `PredefinedWaveFrames::createSaturatedSin`'s `tanh` is reordered.
  - The default saw's `i / 2047.0f` (`wavetable_group.cpp:95`) becomes a
    multiplication by a reciprocal.
  - The JSON dump's `signbit` test is folded, so `-0.0` prints as `0.0`.
  - `std::abs(std::complex<float>)` is inlined as `sqrtf(re² + im²)`.
  - The preset remap curves and wave data are reassociated.
  - Degenerate JSON numbers: `isfinite` is folded, so `1E400` parses as
    infinity, NaN and infinity dump as a garbage double, and the DAZ flag
    makes subnormal doubles dump as `0.0`.
- **Port:** follows the source.
  - `futils::tanh` keeps the source order.
  - The saw divides by 2047.
  - The dump tests `signbit`.
  - `abs` is `hypotf` (`port/crates/wavetable/src/complex.rs`).
  - The remap and wave data follow the source formulas.
  - JSON follows nlohmann's documented behaviour: overflow is a parse
    error, NaN and infinity dump as `null`, and subnormals print normally
    (`sas-common`).
  - The result is bit-identical to the `REF_FAST_MATH=OFF` build. Against
    the official build the difference is rounding-level. For example,
    `keyframe_waves` is 1.2e-7 instead of exact, and preset values differ
    by up to 96 ulp, within 1e-6. All of it is inside the parity
    tolerances and the SAS-36 JSON bound.
- **Exception:** A10, `std::complex<float>` division at the phase
  modifier, follows the official build's plain float formula on every
  target since #54. That is a deliberate return to the release form, so it
  is not a divergence.
- **Why:** the fast-math forms are compiler artefacts with no source to
  port. Mirroring them would mean reproducing GCC's choices call site by
  call site, and they differ between compilers and targets. Authored
  content never hits the degenerate JSON values.
- **Approved:** owner decision 2026-10-21. It is recorded in
  `migration/LEDGER.md` ("Source semantics, owner decisions and the
  remaining cases (2026-10-21)", and "M4a `sas-common`", Decisions, for
  the JSON numbers), in `decisions/owner-decisions-json-and-morphs.md`
  and in `migration/ledger/2026-10-20-m2-preset-registry.md`. Implemented
  in #4, #6, #7 and #8.

### D9. Unsupported `clone()` panics

- **Differs:** `clone()` on `Upsampler`, `Decimator`, `IirHalfbandDecimator`
  and `VoiceHandler`.
- **Upstream:** `VITAL_ASSERT(false); return nullptr;`. The assert is
  compiled out of release builds, which return `nullptr`.
- **Port:** `clone` / `clone_box` panics, with a message naming upstream's
  assert.
- **Why:** nothing in the engine clones these processors. A `nullptr`
  processor has no Rust equivalent other than a panic or an `Option`, and
  an unreachable path does not justify an `Option` in every caller.
- **Approved:** #30 (oversampling: `Upsampler`, `Decimator`,
  `IirHalfbandDecimator`) and #21 (`VoiceHandler`).

### D10. No FMA contraction, no estimates, and JUCE's fallback FFT

- **Differs:** floating-point codegen and the FFT backend, mainly on the
  shipping target (arm64 macOS).
- **Upstream (release):**
  - The real Mac build contracts multiply-adds. Apple clang defaults to
    `-ffp-contract=on`, and upstream's NEON `vmlaq_f32`/`vmlsq_f32` fuse
    into FMLA.
  - JUCE's FFT uses Accelerate/vDSP (`JUCE_USE_VDSP_FRAMEWORK` defaults to
    1).
  - On x86, `-ffast-math` turns vectorised division and `sqrtf` into
    `RCPPS`/`RSQRTPS` estimates plus a Newton step. It also sends
    auto-vectorised libm calls to glibc's libmvec.
- **Port:**
  - It never contracts (no `mul_add`/`vfmaq_f32` where upstream multiplies
    and adds separately).
  - It uses no reciprocal or rsqrt estimates, and division and sqrt are
    exact.
  - It ports JUCE's portable fallback FFT on every target.
  - Transcendentals go to the platform libm without libmvec.
- **Why:** these are CPU- and compiler-dependent rounding with no source
  to port. Fused contraction and estimate instructions make results
  differ between machines. The pinned reference avoids them too (see
  Verification exceptions).
  - Measured on macOS arm64: contraction −91.2 dB residual, vDSP FFT
    −127.5 dB.
  - Both are far below every tolerance.
- **Approved:**
  - `decisions/reference-arm.md`: "Consequences for the Rust port" 2–3
    and "Residual risks", owner-approved with #2.
  - `decisions/reference-determinism.md`, for x86 `RCPPS` and libmvec.
  - migration/AGENTS.md hard rule 5 and "Reference-specific facts".

## Verification exceptions

These are policies of the parity harness, not port behaviour. They change
**what the port is compared against or how**, never what the port does. Each
has its own decision record.

- **The pinned reference build.** Parity is measured against a pinned C++
  build of `/vital`, not upstream's shipped binaries.
  - Flags: `-ffp-contract=off`, `-mno-recip` and `-U__FAST_MATH__` (no
    libmvec).
  - glibc is re-executed with `GLIBC_TUNABLES` pinned to its baseline
    variants.
  - `JUCE_USE_VDSP_FRAMEWORK=0` (JUCE's fallback FFT).
  - The macOS runner image is fixed; macOS 15 is the primary reference.
  - See `decisions/reference-determinism.md` and
    `decisions/reference-arm.md`. D10 is the port-side counterpart.
- **Tolerance floors and `tolerance_exception`.** Outside SAS-35, parity is
  within tolerances, not exact.
  - The floors `RENDER_FLOOR` and `MODULE_FLOOR` were owner-approved on
    2026-10-20 and are pinned in `run_corpus.py` and sas-parity.
  - A case may only loosen with a structured `tolerance_exception` citing
    a dated ledger heading that names it.
  - See `decisions/reference-determinism.md` and migration/AGENTS.md
    rule 6.
- **Normalized-form captures.** Large-scale quantities (Hz, ms, samples)
  may be captured in the form their upstream consumer uses, computed the
  same way in the ref-module and the parity test. For example, a keytrack
  frequency is captured as `frequency * (1.0f / sample_rate)`, with a raw
  case over the band that passes. See migration/AGENTS.md.
- **Bounded JSON state (SAS-36).** Declared JSON artifacts compare
  structurally, with floats within 4 f32 ulp or 1e-6. See
  `decisions/owner-decisions-json-and-morphs.md`.
- **No-fast-math exact comparisons (SAS-35).** Allowlisted noisy-reference
  cases compare bit for bit against the `REF_FAST_MATH=OFF` build, gated
  per target by `--justify-nofastmath`. See `decisions/nofastmath/` and
  `migration/ledger/2026-10-21-owner-approval-sas-35-sas-36.md`.
  - **A11 constraint:** the port follows the official build's merged
    sin/cos in the phase modifier (A11). So no `reference: nofastmath` case
    may render a phase modifier, whose no-fast-math form would differ. See
    `decisions/nofastmath/README.md`.
- **macOS libm drift list.** Cases where Apple libm differs between macOS 14
  and 15 are reported with their metrics instead of failing the same-binary
  check. See `migration/corpus/macos_libm_drift.json` and
  `decisions/reference-arm.md`.
- **macOS arm64 parks.** These are temporary, owner-approved skips of a case
  on the macOS reference only; x86 keeps the case live. See
  `migration/corpus/parked_arm64.json` and migration/AGENTS.md.
- **Parked cases in `pending/`.** Case files under
  `migration/corpus/modules/<area>/pending/` are not loaded until their
  decision lands. Each says why in its description.
