# Decision: Apple Silicon reference, and what parity means on arm64

Status: accepted (2026-10-20). Context: the only shipping target is an
Apple Silicon (arm64) macOS VST3 for FL Studio. There is no universal or
x86_64 macOS build and no Windows/Linux plugin. The x86_64 Linux reference
and its CI remain for development and the existing tests
([reference-determinism.md](reference-determinism.md)).

## Decision

1. **Shipped-plugin parity is checked on macOS arm64 against a macOS arm64
   reference**, not against the x86 reference. The reference is built on
   GitHub `macos-15` (Apple M1) with Apple clang 17 and the pins below. It is
   deterministic: repeat renders are bit-identical, and the same binary
   renders the corpus bit-identically on macOS 14 and macOS 15.
2. **Upstream does not compute the same thing on ARM and x86.** Three
   conversion sites in upstream behave differently per ISA, and they change
   most real patches audibly (6 of 15 corpus outputs fail the tolerances
   between macOS arm64 and x86). The Rust port mirrors upstream *per
   target* at exactly those sites (see "Consequences for the Rust port").
   Everything else must be bit-for-bit ISA-independent.
3. aarch64 Linux is kept only as an optional proxy (manual
   `workflow_dispatch`). It agrees with macOS within all tolerances (worst
   −95 dB) and shows the same ARM-vs-x86 differences, which is useful for
   isolating Apple-specific effects, but it is not needed routinely.

## Findings

All numbers come from CI runs 39509052466, 39510159624 and 39510949182 on
PR #2. The tools are `scripts/cross_platform_report.py`,
`scripts/arm_attribution.py`, `ref-fp-probe` (`machine_info.sh`) and
`scripts/fp_contract_flags_probe.sh`.

### A. Upstream ISA-dependent semantics (not noise; these change the sound)

| # | Site | x86 (SSE2 build) | arm64 (NEON build) | Evidence |
|---|---|---|---|---|
| A1 | `utils::toInt` (`poly_utils.h`): `_mm_cvtps_epi32` vs `vcvtq_s32_f32` | rounds to nearest even | truncates toward zero | Probe: `poly_toInt` equals `nearbyint` on x86 but differs on arm64. `floor`/`ceil` are written to be correct either way; direct `toInt` callers are not. |
| A1a | delay lines: `Memory::get` / `StereoMemory::get` (`memory.h`) | Catmull-Rom parameter t ∈ [0.5, 1.5] | t ∈ (0, 1] | `stereo_delay_impulse` module: −13.3 dB residual, LSD 1.36 dB. Attribution: `unison_pad_fx` with random phase off still fails (−46 dB) and passes once chorus/delay are off (−65.7 dB). |
| A1b | wave-frame index: `wave_index = toInt(frame)` (`synth_oscillator.cpp`) | nearest frame | lower frame | `line_wavetable_fold` −43 dB (LSD 0.39). With wave-frame modulation off: −137.5 dB (pass). |
| A1c | `cubicLookup` index (`lookup_table.h`), oscillator phase accumulators `phase += toInt(...)` | rounding | truncation | Filter and phase cases pass at −85 to −127 dB, so this is below the tolerances on the corpus. |
| A2 | random phase: `uint32_t p = random(-1..1) * amount * INT_MAX` (`synth_oscillator.cpp:583`). Negative float → unsigned is undefined behaviour. | wraps modulo 2^32 (64-bit `cvttss2si`) | `FCVTZU` saturates to 0: every voice with a negative draw starts at phase 0 | Probe: `float_to_uint32_negative` differs. `init_saw` (random phase on by default) +4.7 dB residual; with random phase off: −135.4 dB (pass). The random stream itself is not the cause: `ref-rng-probe` gives identical upstream `RandomGenerator` streams on x86 (libstdc++) and macOS (libc++) except at the A9 edge. A render makes only a handful of phase draws, so an A9 hit there has a probability of about 1e-7, whereas the A2 cast affects every negative draw (half of them). |
| A3 | out-of-range float → `int32_t` | 0x80000000 | saturates | Probe: `float_to_int32_overflow` differs. No corpus case isolates it. |
| A4 | `poly_float::sum()` (`poly_values.h`) horizontal add order | `(v0+v2)+(v1+v3)` (shuffle + add) | `(v0+v1)+(v2+v3)` (`vpadd_f32`) | Reverb feedback matrix (`reverb.cpp:265, 326`) is the main user; its `toInt` only sees floored, exact integers, so A1 does not apply there. Attribution: see below. `poly_int::sum` is integer and order-independent. |
| A5 | `poly_float::max`/`min`: `_mm_max_ps`/`_mm_min_ps` vs `vmaxq_f32`/`vminq_f32` | returns the second operand when either is NaN, or for a ±0 tie | FMAX/FMIN propagate NaN; ±0 ordered (`max(-0, +0) = +0`) | Only observable with NaN or signed-zero ties. The corpus renders are finite; not isolable. Bounded by the passing cases (≤ −85 dB). |
| A6 | `poly_float::notEqual`: `_mm_cmpneq_ps` vs `greaterThan \| lessThan` | unordered: NaN → true | ordered: NaN → false | Call sites (`synth_oscillator.cpp:1298` midi-track flag, `modulation_connection_processor.cpp:75` power check) compare finite parameters with 0. No effect without NaN. |
| A7 | `poly_int::min` (`poly_values.h`) | signed (`_mm_cmpgt_epi32` directly) | unsigned (`vminq_u32`) | `poly_int::max` goes through `greaterThan`, which is unsigned on both (SSE2 XORs the sign bit), so only `min` differs. Through `utils::clamp(v, lo, hi) = max(min(v, hi), lo)`, a *negative* index stays negative on x86, an out-of-bounds read one element before the table, but clamps to `hi` on arm64. This matters only for lookup inputs below about −0.5/scale (`lookup_table.h:40`, `synth_lfo.h:83`, `line_map.cpp:33`, `modulation_connection_processor.cpp:188/229/258`). Not reached by the corpus. |
| A8 | `anyMask()`: `_mm_movemask_epi8` vs `vpmax_u32` | 16-bit byte mask | maximum lane value | Every call site uses it as a boolean on a comparison mask (`if (reset_mask.anyMask())`, `== 0` asserts, `bool x = ...anyMask()`), where both are non-zero exactly when any lane is set. No behavioural effect in upstream; the numeric value differs. |
| A9 | `vital::utils::RandomGenerator` → `std::uniform_real_distribution<float>` / `generate_canonical` (implementation-defined): one 32-bit mt19937 draw u, then u / 2^32 in float | libstdc++ (x86 Linux): when u ≥ 2^32 − 128 the quotient rounds to 1.0f and is clamped to 1 − 2^-24, so the result is max − (max − min)·2^-24 | libc++ (macOS): returns exactly 1.0, so the result is exactly max | Source comparison by Wavetable Creator (libc++ 17 vs libstdc++ 13): identical except this edge, about 3e-8 per draw. `ref-rng-probe`: 21 upstream `RandomGenerator` streams (every engine range, distinct seeds, plus `polyVoiceNext`), 2.05e8 draws. The 14 streams with no edge input are bit-identical between x86 and macOS. The 7 with exactly one edge input each differ, and macOS hits exactly max once in each, which x86 never does (CI run 39518725978). No corpus case is known to hit it. |

A4–A8 were identified by Core Framework from `poly_values.h`. Like A1–A3,
they are mirrored per target in `sas-simd` and switched by its
`aarch64-semantics` feature. A9 is a C++ standard-library difference (x86
reference: libstdc++; macOS reference: libc++), not an ISA one. It is
mirrored per target in `sas-common` (Wavetable Creator, PR #4) and
follows the same feature.

Attribution in the corpus (`scripts/arm_attribution.py`, macOS 15 arm64
vs x86, run 39515281713):

| Variant | Residual | Reading |
|---|---|---|
| `init_saw` single note, random phase on | +4.7 dB (fail) | A2 |
| same, random phase off | −135.4 dB | A2 removed: only rounding-level noise is left |
| same, random phase off, reverb on | −91.5 dB (pass) | A4 (reverb `sum()` order) costs about 44 dB of null depth and stays far inside tolerance |
| `unison_pad_fx`, random phase on | +0.2 dB (fail) | A2 + A1a |
| random phase off | −46.2 dB (fail) | A1a (chorus/delay `Memory::get`) |
| random phase off, chorus/delay off | −65.7 dB (pass) | |
| … and reverb off | −65.3 dB | not A4 |
| … and unison 1 voice | −67.2 dB | not unison. This patch's own float-rounding floor is −62.9 dB (no-fast-math proxy), from phase drift over sustained chords |
| `line_wavetable_fold`, env 2 → wave frame | −43.1 dB (fail) | A1b |
| wave-frame modulation off | −137.5 dB | A1b removed |

A5–A8 are not reached by the corpus (no NaNs, no signed-zero ties, no
negative lookup indices; `anyMask` is only used as a boolean). Their
combined effect is bounded by the passing cases.

Random phase is on by default, so A2 alone makes almost every real patch
differ between upstream on Intel and upstream on Apple Silicon. Upstream's
Apple Silicon behaviour is what the shipped plugin must match.

### B. Build- and platform-level drift on arm64 (pinned)

| Source | Effect if not pinned (macOS 15, worst case over corpus) | Pin |
|---|---|---|
| FP contraction. Apple clang defaults to `-ffp-contract=on` (`fast` with `-ffast-math`) and GCC/aarch64 to `fast`. Upstream's NEON `mulAdd`/`mulSub` are `vmlaq_f32`/`vmlsq_f32`, written as `a + b * c` in `arm_neon.h`, so they fuse into FMLA. The probe's `neon_vmlaq` with default flags equals `fma`. | −91.2 dB residual, LSD 0.0019 dB (aarch64 Linux: −89.4 dB) | `-ffp-contract=off` on arm64. Gives the same unfused multiply-then-add as the x86 SSE2 path. |
| **Apple clang 15 ignores `-ffp-contract=off` under `-ffast-math`.** Its driver passes `-menable-unsafe-fp-math` to cc1, and the backend then fuses regardless (2 FMAs in the probe kernel). Apple clang 17 passes `-funsafe-math-optimizations` and honours it (0 FMAs). A macOS 14 native build had about 9,500 fused instructions (`fmadd`/`fmla`/`fmls`/`fnmsub`) across `ref-render` and `ref-module`, in engine code such as `LineGenerator`, wavetable modifiers and `MidiManager`. | n/a | The macOS reference identity requires Apple clang 17 (`macos-15`). CMake warns for Apple clang < 17, and `check_fp_determinism.sh` fails any fused instruction. |
| JUCE FFT backend. Upstream Mac builds use Accelerate/vDSP (`JUCE_USE_VDSP_FRAMEWORK` defaults to 1); the x86 reference uses JUCE's portable fallback. | −127.5 dB, LSD 0.00015 dB | `JUCE_USE_VDSP_FRAMEWORK=0` (portable fallback FFT, same code as x86). The guard fails on any vDSP / vForce import. |
| Apple libm vs glibc | Every probed transcendental differs in low bits. aarch64 Linux (GCC, glibc) vs macOS (Apple clang, Apple libm): −95.3 dB worst, all cases pass. | Not pinnable; recorded in the identity (`libm_variants`: "Apple libm (macOS 15.7.9)"). |
| macOS version (Apple libm updates) | The same binary is bit-identical on macOS 14.8 and 15.7 for the whole corpus, but Apple's `powf` returns different bits on the two (probe). | Reference runner pinned to `macos-15`; the macOS version is part of the identity; a deployment target of 14.0 lets one binary run on both for this check. |
| Estimates (FRECPE/FRSQRTE) | Upstream uses them only on 32-bit ARM (`NEON_ARM32`); AArch64 uses exact `vdivq_f32` and scalar `sqrtf`. FRECPE/FRSQRTE are bit-exact by the Arm architecture spec anyway. | The guard fails on any `frecpe`/`frecps`/`frsqrte`/`frsqrts`. |
| libmvec | No AdvSIMD libmvec calls on aarch64 (`-U__FAST_MATH__`, as on x86). Apple has no auto-vectorised libm. | The guard fails on `_ZGV*` / `vv*` imports. |
| Denormals | JUCE sets FPCR.FZ on arm64 (x86: MXCSR FTZ+DAZ). Edge-case tininess detection differs, at the 1e-38 level. | Same code path as upstream; no measurable effect on the corpus. |

Pinned macOS flags, as recorded in metadata:
`-O3 -ftree-vectorize -ftree-slp-vectorize -funroll-loops -ffast-math -U__FAST_MATH__ -ffp-contract=off -DJUCE_USE_VDSP_FRAMEWORK=0`,
Apple clang 17.0.0 (clang-1700.0.13.5), macOS 15.7.9, deployment target 14.0.
The guard (`check_fp_determinism.sh`) passes.

Basic IEEE arithmetic agrees across the ISAs: `exact_div`, `sqrtf`,
`fma`, unfused multiply-add and `nearbyint` hash identically on x86 and
arm64.

### C. Measurements against the corpus tolerances (unchanged)

| Comparison | Cases passing | Worst residual | Worst LSD | Worst RMS diff |
|---|---|---|---|---|
| macOS 15 arm64 vs x86_64 Linux | **9/15** (10/16 with the harness echo case added by PR #3) | +4.7 dB (`init_saw`) | 4.76 dB | 0.74 dB |
| aarch64 Linux vs x86_64 Linux | 9/15 (same six fail) | +4.7 dB | 4.76 dB | 0.74 dB |
| macOS 15 vs aarch64 Linux | 15/15 | −95.3 dB | 0.0019 dB | 1.8e-6 dB |
| macOS 14 (same binary) vs macOS 15 | 15/15, bit-identical | −inf | 0 | 0 |
| macOS repeat render | bit-identical | | | |
| macOS no-fast-math proxy vs pinned | 15/15 | −62.9 dB | 0.036 dB | 2.0e-5 dB |
| macOS contraction on vs off | 15/15 | −91.2 dB | 0.0019 dB | 3.5e-6 dB |
| macOS vDSP FFT vs fallback | 15/15 | −127.5 dB | 0.00015 dB | 4.2e-7 dB |
| `simd/poly_function` (Core Framework, 127 cases), macOS vs x86 | 124/127 (95 bit-exact) | fails: `to_int`, `trunc`, `encode_phase_and_voice` (all A1) | | |

The six macOS-vs-x86 failures are `init_saw` × 3 (A2), `unison_pad_fx`
(A2 + A1a), `line_wavetable_fold` (A1b) and the `stereo_delay_impulse`
module (A1a). Removing the responsible mechanism makes each pass
(attribution table above). The no-fast-math proxy passes on macOS with
margins like x86 (−62.9 vs −61.4 dB), so the existing tolerances are
achievable on arm64.

## Why not use the x86 reference for macOS parity?

- Upstream on Apple Silicon *is* different (section A). Parity with the
  x86 reference would demand x86 behaviour from a plugin that only ships on
  arm64. That contradicts "port upstream faithfully", and it fails 6 of 15
  cases for an implementation that matches upstream on the target.
- The macOS reference can be made deterministic: bit-identical repeats and
  bit-identical across macOS 14/15 with one binary. The guard removes the
  only compiler-dependent arithmetic (contraction), and the identity
  records compiler and macOS version.
- Running parity natively on arm64 exercises the Rust NEON code path that
  ships.

## Consequences for the Rust port

1. **Per-target upstream semantics at the enumerated sites only (A1–A9).**
   They live in one place in `sas-simd`:
   - `to_int` (rounding vs truncation);
   - float→u32 (wrap vs saturate) and float→i32 overflow;
   - horizontal `sum` order;
   - float `max`/`min` NaN and ±0 handling;
   - `not_equal` ordering;
   - signed vs unsigned `poly_int` `min`;
   - the `any_mask` value;
   - the `uniform_real_distribution` canonical edge (A9, in `sas-common`).

   The x86_64 variants are the default on x86_64. The aarch64 variants are
   used on aarch64, and on x86_64 when the `aarch64-semantics` feature is
   enabled, so x86 developers can reproduce the shipping behaviour against
   macOS reference renders. Rust's `as` casts truncate and saturate (the
   arm64 behaviour), so the x86 variants must be written explicitly. Any new
   ISA-dependent upstream site is added to the table in section A; never
   reproduce one implicitly.
2. **No FMA contraction, no estimates.** Rust never contracts implicitly;
   do not use `mul_add` / `vfmaq_f32` where upstream does a separate
   multiply and add. On aarch64 that includes upstream's
   `vmlaq_f32`/`vmlsq_f32`, which the reference compiles unfused. Do not use
   reciprocal or rsqrt estimate intrinsics; division and sqrt are exact.
3. **FFT:** port JUCE's fallback FFT, which both references use, rather than
   using vDSP or a third-party FFT with different rounding.
4. **libm:** transcendental calls go to the platform libm like upstream
   (Apple libm on macOS), which matches the macOS reference's calls.
   Cross-platform libm differences are tolerance-level (≤ −95 dB on the
   corpus).
5. **Where parity is checked:**
   - On macOS arm64, `sas-parity` runs against captures rendered by the
     macOS reference in the same CI job (`reference-arm.yml`), with
     `SAS_PARITY_CAPTURES=build/platform/pinned SAS_PARITY_NO_BUILD=1 cargo test --workspace`.
     This is the gate for the shipped plugin. The macOS arm64 build identity
     (Apple clang 17.0.0, macOS 15.7.9, pinned flags) has its own entries in
     `corpus/reference_hashes.json` and the per-module `.hashes.json` files.
     The job checks them with `hashes --check --require-known-identity`.
   - On x86_64 Linux, `sas-parity` runs against x86 captures (`ci.yml`) with
     x86 semantics, for development speed and the existing tests.

## Residual risks

- Apple Silicon generations: CI runs on M1 only. Arm64 IEEE arithmetic is
  deterministic across implementations, but Apple's libm could in principle
  differ by CPU generation. If an M2+ runner becomes available, run the
  same binary there; the probe makes that a one-step check.
- Apple libm changes with macOS releases (`powf` already differs between 14
  and 15). Keep the runner image pinned, and re-baseline the hashes
  deliberately when moving to a new macOS, recording the reason in
  LEDGER.md.
- The reference pins contraction off and the fallback FFT, while
  upstream's real Mac binaries fuse and use vDSP. Both effects are measured
  at −91 dB and −127 dB, far below every tolerance.
