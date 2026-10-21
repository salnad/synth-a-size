# Owner decisions: bounded JSON state and no-fast-math exact cases

The owner (Salil) approved the following on 2026-10-21 (LEDGER.md entry
"2026-10-21: owner approval — JSON bound and no-fast-math exact cases"):

1. "ULP-bounded, not byte-exact: preset and wavetable JSON values must agree
   within a few ULP of float rounding".
2. Exact comparison against the no-fast-math build for frequency-mode
   interpolation between keyframes with disjoint spectra, folding at the
   clamp, and frequency-interpolated audio fades.

The specific numbers below (4 f32 ULP or 1e-6 absolute, whichever is
looser) are the coordinator's interpretation of "a few ULP", **pending owner
confirmation**. The five allowlisted case ids are the measured instances of
the three categories in decision 2. Both rules are enforced in code,
identically, by `migration/scripts/run_corpus.py` and `port/crates/parity`
(sas-parity).

## 1. JSON state parity is bounded, not byte-exact

Preset and wavetable JSON state must match **structurally**, and numbers
must agree within a small bound.

- **Structure is exact.** Object keys must be identical; their order and the
  formatting are ignored. Value types must be identical, from null, bool,
  integer, float, string, array and object; an integer is not a float.
  Strings, integers, booleans and array lengths must match exactly.
- **Floats** must agree within **4 f32 ulp** (after rounding both values to
  f32) **or 1e-6 absolute, whichever is looser**. These numbers are the
  coordinator's interpretation of the owner's "a few ULP", pending owner
  confirmation.
  - The absolute floor is needed because source-formula values near zero
    differ by up to 96 ulp but only 7.8e-7 absolute.
  - Below |x| = 4 the absolute floor is the looser term. At 1.0, 1e-6 is
    about 8 ulp, so sample data in [-1, 1] is effectively held to 1e-6
    absolute. From |x| = 4 up, 4 ulp is the looser term.
  - -0.0 equals 0.0.
- **Base64 f32 data.** Wavetable creator JSON, and presets that embed it,
  store keyframe samples as base64 little-endian f32 strings (`wave_data`).
  For keys the case declares in `base64_f32_keys`, both strings are decoded.
  The two arrays must have exactly the same length, and each element must
  agree within the same bound. Keys are declared, never guessed from the
  content.
  - Only the keys pinned in code may be declared: `BASE64_F32_KEYS` =
    `{"wave_data"}` in both resolvers. Any other name is rejected at load.
    Declaring `audio_file` would otherwise let int16 PCM pass the f32 bound,
    because reinterpreted PCM words are denormals within 1e-6.
  - Extending the set is a code change plus an owner decision.
  - Every other string stays exact, including PCM int16 base64 such as
    `audio_file` in PCM form.
- **Opt-in per artifact.** The case (or its file) declares which text
  artifacts are JSON state:

  ```json
  "artifacts": { "saved.json": "json" },
  "base64_f32_keys": ["wave_data"]
  ```

  Undeclared text artifacts stay byte-exact. The determinism comparisons
  (`compare --exact`, committed hashes) stay byte-exact for every artifact.
- **The bound is pinned** in both resolvers (`JSON_BOUND` in run_corpus.py
  and `cases::JSON_BOUND` in sas-parity), like the tolerance floors.
  - A case may tighten it with `"json_bound": {"ulp": 0, "abs": 0}`, or
    declare the artifact `"exact"` for byte-exact.
  - Anything looser is rejected at load. Loosening is a code change in both
    resolvers and needs a new owner decision.
- A text artifact must still be upstream's own serialization of the state
  (migration/AGENTS.md). The bound covers float noise; it is not a licence
  to reformat numbers.

`assert_artifact!` picks the mode from the case file; a test cannot choose
it.

## 2. Ill-conditioned wavetable morphs are compared exactly against the no-fast-math build

The owner approved exact comparison against the no-fast-math build for
frequency-mode interpolation between keyframes with disjoint spectra,
folding at the clamp, and frequency-interpolated audio fades. Some content
depends on FFT rounding noise. In frequency-mode keyframe
interpolation between disjoint spectra, folding at the clamp and
frequency-interpolated audio fades, upstream takes interpolated phases from
`arg(conj(from)·to)` of noise-level bins.

- The official fast-math reference misses the module limits on this content
  against its own `REF_FAST_MATH=OFF` build: −27 to −62 dB, and 2.6e-3 for
  folding.
- The port equals the no-fast-math build bit for bit (Wavetable Creator's
  M4a measurements).
- Such cases are therefore compared **exactly** against the no-fast-math
  build: arrays and artifacts must be bit-equal.
- Well-conditioned content keeps the module limits against the official
  reference.

### Mechanism

- **Declaration:** `"reference": "nofastmath"` on the case (never at file
  level).
- **Allowlist gate:** the case id must be listed below. The resolvers read
  the list from this section and nowhere else.
- **No tolerances:**
  - The case cannot set `tolerances` or carry a `tolerance_exception`.
  - Its normal limits, used only by the calibration check, are the module
    defaults plus its declared artifact modes under the pinned
    `JSON_BOUND`.
  - File-level tightening of `tolerances` or `json_bound` does not count,
    and the case itself cannot set either. So a case cannot make the
    official reference "fail" on purpose.
- **Calibration gate:** a listed case is valid only while CI's calibration
  step shows that the official reference **fails** the case's normal limits
  against the no-fast-math build. The step is
  `run_corpus.py compare build/ref build/proxy --justify-nofastmath`. If the
  official reference passes, CI fails and the case must use the official
  reference. This implements the owner's proposed general rule (pending
  decision) alongside the explicit list; the owner may keep both.
- **Captures:**
  - The calibration job renders the no-fast-math tree and uploads it as
    `parity-captures-nofastmath`.
  - The parity job reads it through `SAS_PARITY_CAPTURES_NOFASTMATH`
    (default `build/parity-nofastmath`). Locally, sas-parity builds
    `migration/reference/build-nofastmath` and renders on demand.
- **Hashes:**
  - Captures are pinned with their own build identity (`fast_math: false`)
    in each module's `.hashes.json`, for listed cases only:
    `run_corpus.py hashes <tree> --write --reference nofastmath`.
  - The identity is known only if it is exactly a CI identity's
    no-fast-math counterpart: `fast_math: false`, and `opt_flags` equal to
    the CI flags with ` -ffast-math -U__FAST_MATH__` removed. Everything
    else must be equal. Any other flag change (FMA contraction, `-march`)
    is a different build.
  - The official reference still renders and hash-pins these cases, so its
    determinism stays covered.

### No-fast-math exact cases (allowlist)

These five ids are the measured instances of the three approved categories:
frequency-mode interpolation between keyframes with disjoint spectra,
folding at the clamp, and frequency-interpolated audio fades.

To add a case, add one line to this list. The lines must have the form
``- `<case id>` `` followed by optional text. CI (`run_corpus.py
allowlist-approval`) fails the change unless the same change adds a
LEDGER.md section whose heading contains "owner approval" and which names
each added id. Removing an id needs no approval: the case returns to the
official reference.

- `wavetable/render/wave_linear_freq_disjoint_spectra`: frequency-mode linear interpolation between disjoint spectra
- `wavetable/render/wave_cubic_freq_disjoint_spectra`: frequency-mode cubic interpolation between disjoint spectra
- `wavetable/render/multi_group_disjoint_spectra`: a multi-group table containing it
- `wavetable/render/wave_folder_at_clamp`: wave folder at ±1
- `wavetable/render/audio_fade_3_phase_0`: frequency-interpolated audio fade, phase style 0

`audio_fade_3_phase_1` and `audio_fade_3_phase_2` are well-conditioned once
the source formulas are fixed, so they stay on the official reference.
Pending owner decisions may add some comb-filter and 120 Hz crossover (2×
and above) cases. Oscillator pitch sweeps and reverb tails show the same
pattern and are pending too.

### macOS arm64

Can a macOS arm64 no-fast-math build serve the same role? This was measured
on CI run 39521775311 of the throwaway branch
`cursor/macos-nofastmath-probe-f875`. That branch combines the Apple Silicon
reference build (PR #2) with the wavetable render module (PR #8), plus
probe-only versions of the allowlisted cases. It rendered all 633 module
outputs on Linux x86_64 (GCC 13), macOS 15 arm64 (Apple clang) and macOS 14,
all with `REF_FAST_MATH=OFF`.

| Comparison | Result |
|---|---|
| macOS 15, repeat render | 633/633 outputs bit-identical |
| Same binary, macOS 15 vs macOS 14 | 632/633 bit-identical; `simd/poly_function/poly_pow` differs in 110 values by 1 ulp (Apple libm `powf`, already known from PR #2). All allowlisted cases are bit-identical. |
| Linux x86_64 vs macOS 15 arm64 | 453/633 bit-identical, 180 differ, 140 of them in `wavetable/render`. Every array of all five allowlisted cases differs, by up to 1.9e-6 absolute (folding) and 1.5e-7 to 6.6e-7 for the others. 8 of 72 full renders differ. |

Findings:

- **The Linux no-fast-math captures cannot be the exact target on macOS
  arm64.** The two no-fast-math builds disagree on exactly the content this
  rule covers. These numbers are not loosened or folded into a tolerance.
- **A macOS arm64 no-fast-math build can serve the same role on macOS.** It
  is deterministic, and stable across macOS 14 and 15 on all allowlisted
  content. It needs its own pinned identity, which includes the macOS
  version because Apple libm changes (`powf`). Its own calibration gate
  would be the macOS official reference against the macOS no-fast-math
  build.
- **Not established yet:** whether the arm64 Rust port reproduces the macOS
  no-fast-math build bit for bit. The port of these cases is not on main,
  and the source of the cross-platform difference is not attributed.
  Candidates are the ARM-vs-x86 code paths in upstream (A1–A8 in PR #2),
  Apple libm versus glibc float functions, and complex arithmetic. If
  exactness does not hold there, it will be recorded, not loosened.
- **Wiring the macOS tree** (its own `parity-captures-nofastmath` from the
  macOS job, hashes and calibration gate) waits for PR #2 to merge. Nothing
  in this mechanism is x86-specific: `SAS_PARITY_CAPTURES_NOFASTMATH` points
  at whichever platform's no-fast-math tree the job renders.
