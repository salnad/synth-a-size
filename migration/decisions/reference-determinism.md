# Decision: make the C++ reference bit-identical across x86_64 CPUs, then calibrate tolerances

Status: accepted (2026-10-20). Owner approved the tolerance change; see
LEDGER.md for that date.

## Problem

The first CI run showed that the same reference build recipe gave different
audio on a GitHub runner and on a development VM. Both ran Ubuntu 24.04
with GCC 13.3, and each machine was deterministic on its own. Most cases
differed only at −120 to −195 dB, but `unison_pad_fx__chords` differed by
up to 3.0e-3 (−69 dB residual). That is more than the first tolerances
allowed, and reference renders that depend on which machine produced them
make parity results meaningless.

## Machines

| Machine | CPU | Notes |
|---|---|---|
| Dev VM | Intel Xeon (family 6, model 0xcf), AVX-512, FMA | 8 cores |
| CI run 39503118688 | AMD EPYC 9V45 (Zen 5), AVX-512, FMA | 2 cores |
| CI run 39504159036 | AMD EPYC 7763 (Zen 3), AVX2, FMA | 2 cores |
| CI run 39504834964 | AMD EPYC 9V74 (Zen 4), AVX-512, FMA | 2 cores |

Package versions on every machine: `gcc-13 13.3.0-6ubuntu2~24.04.1`,
`libc6 2.39-0ubuntu8.9`, `libstdc++6 14.2.0-4ubuntu2~24.04.1`. CI captures
CPU flags, glibc's view of the CPU features, package versions and binary
hashes with `migration/scripts/machine_info.sh` and uploads them as the
`reference-binaries` artifact.

## Investigation

| Hypothesis | Test | Result |
|---|---|---|
| Build differs (host targeting, `-march=native`, fast-math codegen) | Compared `.text` SHA-256 of the runner-built and VM-built binaries | **Ruled out.** `.text` is byte-identical (`ref-render` `270dcfe8…`). Clean rebuilds in other directories are also identical. |
| Binary vs runtime | Downloaded the runner-built binary and ran it on the VM | **Runtime.** On the VM the runner's binary reproduced the VM build's output exactly (15/15). Against what the same binary produced on the runner, 11 of 15 outputs differed. |
| `poly_float` SIMD width selection | Read `poly_values.h` | **Ruled out.** The width is fixed at compile time: SSE2 on x86, and the AVX2 path is `static_assert(false)`. |
| Threads / CPU count | `taskset -c 0` and `-c 0,1` | **Ruled out.** Same hash. |
| Environment, locale, HOME | `env -i`, `LANG=C`, other locales | **Ruled out.** Same hash. |
| Uninitialised heap memory | `MALLOC_PERTURB_` = 1, 85, 170, 255 | **Ruled out.** Same hash. |
| Random seeds | Seeds come from a deterministic counter; repeated runs are identical | **Ruled out.** |
| Denormals / FTZ / DAZ | Same code on every machine; denormal arithmetic is deterministic IEEE; JUCE sets FTZ/DAZ the same way everywhere | **Ruled out.** |
| glibc scalar libm ifunc dispatch | `ref-fp-probe` hashes all 26 imported libm functions over 2M inputs each, run on both machines, with default dispatch and with FMA masked | **Not the cause here, but a real hazard.** Hashes are identical across the machines in both modes. On *one* machine, masking FMA changes 13 of 14 double-precision functions (`sin`, `exp`, `pow`, ...), so a CPU without FMA/AVX2 *would* differ. The corpus renders do not change when FMA is masked. |
| **SSE approximate reciprocals** | `objdump` of `ref-render`, and probe of raw `rcpps`/`rsqrtps` | **Root cause 1.** See below. |
| **glibc libmvec SSE kernels** | Hash tripwire, `nm -D`, `objdump` of `libmvec.so.1`, and probe of `_ZGV*` entry points | **Root cause 2.** See below. |

### Root cause 1: `-ffast-math` turns division into `RCPPS` + one Newton step

The upstream-equivalent flags (`-O3 -ffast-math`) let GCC implement
vectorised single-precision division and `sqrtf` with `RCPPS`/`RSQRTPS`
plus one Newton–Raphson step. GCC's `<xmmintrin.h>` defines `_mm_div_ps`
as plain vector division, so every `poly_float` division in the engine
(`poly_values.h`: `div()` → `_mm_div_ps`) is affected. The old binary had
291 `rcpps`, 2 `rsqrtps` and 1 `rsqrtss` in 87 functions, including
`SynthOscillator::process`, every filter's `setupFilter`/`process`,
`Envelope::process*`, `Delay::processWithInput`, `Reverb` and `Compressor`.

`RCPPS`/`RSQRTPS` are only specified to about 12 bits; their exact output
is implementation-defined. `ref-fp-probe` shows it directly:

| Probe (2M inputs) | Intel VM | AMD EPYC 7763 and 9V74 |
|---|---|---|
| `rcpps` | `863b7d5c…` | `1b829da3…` |
| `rsqrtps` | `b2c3fd69…` | `d52f80aa…` |
| `rcpps` + Newton step (GCC's division sequence) | `55157e76…` | `37aba7f1…` |
| exact `divss` | `53a36230…` | `53a36230…` |

The Newton step reduces the error but does not make the result correctly
rounded, so ULP-level differences remain. They start in the first block:
the first differing frame was 0 in 6 of 8 renders, and 16 and 5 in the
other two. Feedback paths then amplify them. In `unison_pad_fx` the first
difference above 1e-4 appears at 0.34 s, and the largest is 3.0e-3. Among
the module captures, `distortion` and `envelope` were identical across
machines, while `digital SVF` and `ladder` differed from frame 0 and
`stereo_delay` from frame 6400.

GCC 13 quirk: `-mrecip=none` does **not** disable this (verified on a
minimal file and on the full build); `-mno-recip` does.

### Root cause 2: auto-vectorised libm calls go to libmvec

With `__FAST_MATH__` defined, glibc's `<bits/math-vector.h>` declares
SIMD variants of `sinf`, `asinf` and others, and GCC's vectoriser calls
libmvec (`_ZGVbN4v_sinf`, `_ZGVbN4v_asinf`, `_ZGVbN4v_acosf`, `_ZGVbN4v_tanf`,
`_ZGVbN4v_cosf`, `_ZGVbN4vv_atan2f`, `_ZGVbN2v_{sin,cos,exp}`). libmvec's
SSE kernels use `RCPPS`/`RSQRTPS` internally. After root cause 1 was fixed,
the committed-hash tripwire still caught one case, `line_wavetable_fold`,
at −139 dB. Its Wave Folder (`WaveFoldModifier::render`) calls
`sinf`/`asinf` in a loop, which GCC vectorised into libmvec. Probe on the
Intel VM vs AMD EPYC 9V74:
`libmvec_asinf` `a603d6b3…` vs `50df133a…`, and `libmvec_acosf` `5737ae81…`
vs `3a9497ed…`. libmvec `sinf`/`cosf`/`tanf`/`atan2f` and the
double-precision kernels match, because SSE has no double-precision
approximate reciprocal.

## Changes

All changes are in `migration/reference/` and `migration/scripts/`;
`/vital` is untouched.

1. **`-mno-recip`**: division and `sqrtf` are exact IEEE operations. The
   rest of `-ffast-math` (reassociation, reciprocal-math, finite-math) is
   kept, so the reference stays as close to upstream's `-Ofast` semantics as
   determinism allows. One `rsqrtss` remains, in
   `juce::dsp::FIR::Coefficients<float>::normalise()`, which nothing in the
   engine calls (allow-listed with that justification).
2. **`-U__FAST_MATH__`**: libm calls stay scalar, so libmvec is no longer
   linked at all.
3. **ISA pinned**: `-march=x86-64 -mtune=generic -msse2 -mfpmath=sse`, plus
   `-ffp-contract=off` so an `-march` override cannot bring in FMA
   contraction.
4. **libm variants pinned**: `ref-render`/`ref-module` re-exec themselves
   once with `GLIBC_TUNABLES=glibc.cpu.hwcaps=-AVX,-AVX2,-FMA,-FMA4`, so
   every x86_64 CPU runs glibc's baseline SSE2 libm variants (the double
   functions differ with and without FMA, as measured above).
   `REF_NO_LIBM_PIN=1` opts out. Each render's metadata records
   `libm_variants`, `glibc` and `opt_flags`.
5. **Guards in CI**:
   - `check_fp_determinism.sh` fails on any `rcp*`/`rsqrt*`, AVX-512
     approximation, FMA or x87 transcendental outside the allowlist, and on
     any libmvec import.
   - `run_corpus.py hashes --check` compares every corpus output with the
     committed `migration/corpus/reference_hashes.json` whenever the build
     identity matches (upstream SHA, compiler, glibc, flags, libm pin).
     GitHub runners vary between AMD generations from run to run (three in
     this investigation), so this re-tests cross-machine determinism
     continually.
6. **`ref-fp-probe`**, built without fast-math, hashes libm, raw
   `rcpps`/`rsqrtps` and the libmvec kernels for direct comparison between
   machines.

The fix moves the reference itself: exact division changes renders by up to
3.1e-3 (`unison_pad_fx` −64.8 dB, `init_saw__chords` −68.4 dB), and
dropping libmvec changes the filter and Wave Folder cases by ≤ 2.4e-7. Old
renders were tied to one CPU vendor, so there was no single upstream value
to preserve.

## Result

After the fix, all 15 corpus outputs (8 renders and 7 module outputs) are
**bit-identical between the Intel VM and the AMD EPYC 9V74 runner**. The
EPYC 7763 run matched 14 of 15 before the libmvec fix, and the miss was the
libmvec case. CI now enforces this on every run.

Remaining assumptions, all recorded in the build identity: glibc 2.39
(libm algorithms change between glibc versions), GCC 13.3 and x86_64.
aarch64 is not covered.

## Tolerance calibration

There are two noise floors:

- **Cross-machine (after fix): zero** on every metric (bit-identical).
- **"Faithful port" proxy:** a `REF_FAST_MATH=OFF` build with the same ISA
  pins vs the default build. It runs the same algorithm and differs only in
  float rounding and evaluation order, which is the kind of difference a
  correct Rust port (no fast-math) will show. CI rebuilds this proxy every
  run and requires it to pass the corpus tolerances (`tolerance-calibration`
  job), so the limits can never be tightened below what a correct port can
  reach.

Proxy noise per case:

| Case | max abs | residual | LSD | RMS diff |
|---|---|---|---|---|
| `unison_pad_fx__chords` | 2.6e-3 | **−61.4 dB** | **0.043 dB** | **1.6e-5 dB** |
| `mono_legato_glide__legato` | 2.3e-3 | −66.2 dB | 0.0016 dB | 0 |
| `init_saw__chords` | 3.1e-3 | −69.2 dB | 0.0055 dB | 0 |
| `analog_filter_lfo__chords` | 2.4e-4 | −81.7 dB | 0.0005 dB | 1e-5 dB |
| `analog_filter_lfo__velocity_96k_b512` | 3.4e-7 | −122.3 dB | 0.0002 dB | 0 |
| `init_saw__single_note`, `..._48k_b128` | 1.2e-7 | −135 dB | 0.0001 dB | 0 |
| `line_wavetable_fold__velocity` | 2.1e-7 | −137.6 dB | 0.0001 dB | 0 |
| module captures (worst) | 1.9e-6 | −118.2 dB | 0.0002 dB | 0 |

The sensitive cases are phase drift, not glitches. In `init_saw__chords`
the difference starts exactly when the fourth chord begins (2.25 s) and
grows steadily. In `mono_legato_glide` it starts with the pitch-bend swell
(1.2 s). A ULP-level difference in a phase increment produces a phase
error that grows linearly over a sustained note. That is why the
sample-level metrics are fragile on long, detuned or bent notes while the
phase-blind metrics stay near zero.

### Final render tolerances (`cases.json` defaults)

| Metric | Old | New | Margin over worst proxy case | Reasoning |
|---|---|---|---|---|
| `max_abs_error` | 1e-3 | **not gated** | n/a | A single-sample peak is dominated by phase drift on sustained notes. The proxy hit 3.1e-3 on a correct implementation. `residual_db` measures the same sample-level agreement as an energy ratio, which is robust to that. |
| `residual_db` | −60 dB | **−50 dB** | 11.4 dB (worst −61.4) | Owner's choice. It still catches any real structural error (wrong filter, envelope, routing or missing voice), which shows up between −30 and 0 dB. |
| `lsd_db` | 0.5 dB | **0.1 dB** | 2.3× (7.3 dB) over the worst (0.043 dB) | Tightened 5×. The worst case is the chorus/delay/reverb unison pad; every other case is ≤ 0.0055 dB. |
| `rms_diff_db` | 0.1 dB | **0.01 dB** | ≈ 600× (56 dB) over the worst (1.6e-5 dB) | Tightened 10×. Set at a perceptual floor (0.01 dB ≈ 0.12 % amplitude) rather than at the noise, which is four orders of magnitude lower; a tighter level gate would only measure summation order. |

### Module tolerances

Unchanged: `max_abs_error` 1e-4 (1e-5 for the envelope), `residual_db`
−80 dB (−100 dB for the envelope). Module captures are short and have no
feedback chaos, so the sample-level gates stay. The worst proxy module
case is now 1.9e-6 / −118 dB, a margin of at least 18 dB (38 dB for the
filters). Before the fix, `stereo_delay_impulse` missed its limits at
−63 dB; that was entirely `RCPPS`, and it now sits at −149 dB.

## Consequences for the port

- Rust's `f32` division and `sqrt` are exact, matching the fixed reference.
  The port must not use approximate reciprocal intrinsics or a SIMD math
  library with vendor-specific results; AGENTS.md rule 5 covers this.
- Adding a new corpus case requires its proxy noise to pass the defaults
  (CI checks this). If a new case cannot meet them, record the measurement
  and ask a human; do not loosen the limits.
- If CI's hash tripwire reports a mismatch while the build identity
  matches, a new CPU-dependent path has appeared. Investigate it before
  re-baselining the hashes.
