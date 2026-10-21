# Owner decisions: bounded JSON state and no-fast-math exact cases

The owner (Salil) approved the following on 2026-10-21:

1. "ULP-bounded, not byte-exact: preset and wavetable JSON values must agree
   within a few ULP of float rounding" (LEDGER.md entry "2026-10-21: owner
   approval — JSON bound and no-fast-math exact cases"). **SAS-36** then
   approved the bound as implemented: 4 f32 ULP or 1e-6 absolute,
   whichever is looser.
2. Exact comparison against the no-fast-math build, first for
   frequency-mode interpolation between keyframes with disjoint spectra,
   folding at the clamp, and frequency-interpolated audio fades. **SAS-35**
   (the noisy-reference rule) then generalized it to every case where the
   C++ reference itself amplifies fast-math noise. Salil named the 3
   pitch-tracking LFO cases, the 8 chaotic Lorenz random-LFO cases, the 22
   set-aside oscillator cases, and any future cases of the same kind. Each
   case must still pass `--justify-nofastmath` in CI.

SAS-35 and SAS-36 are recorded in
`migration/ledger/2026-10-21-owner-approval-sas-35-sas-36.md`. Both rules
are enforced in code, identically, by `migration/scripts/run_corpus.py` and
`port/crates/parity` (sas-parity).

## 1. JSON state parity is bounded, not byte-exact

Preset and wavetable JSON state must match **structurally**, and numbers
must agree within a small bound.

- **Structure is exact.** Object keys must be identical; their order and the
  formatting are ignored. Value types must be identical, from null, bool,
  integer, float, string, array and object; an integer is not a float.
  Strings, integers, booleans and array lengths must match exactly.
- **Floats** must agree within **4 f32 ulp** (after rounding both values to
  f32) **or 1e-6 absolute, whichever is looser**. The owner approved these
  numbers as implemented (SAS-36).
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

## 2. Cases where the reference amplifies fast-math noise are compared exactly against the no-fast-math build (SAS-35)

Some content turns fast-math rounding noise into large output differences
in the C++ reference itself. Examples are pitch tracking, chaotic (Lorenz)
random LFOs, long feedback and delay tails, and, in wavetable morphs,
phases taken from `arg(conj(from)·to)` of noise-level FFT bins.

- On such content the official fast-math reference misses the module limits
  against its own `REF_FAST_MATH=OFF` build. In wavetable render it is −27
  to −62 dB, and 2.6e-3 for folding.
- A faithful port equals the no-fast-math build bit for bit (Wavetable
  Creator's M4a measurements).
- Such cases are therefore compared **exactly** against the no-fast-math
  build: arrays and artifacts must be bit-equal.
- Well-conditioned content keeps the module limits against the official
  reference.

### Mechanism

- **Declaration:** `"reference": "nofastmath"` on the case (never at file
  level).
- **Allowlist gate:** the case id must be listed in
  `migration/decisions/nofastmath/<area>/<module>.md`, one file per corpus
  module, as ``- `<area>/<module>/<case>` (SAS-35; <targets>): <reason>``. The
  resolvers read the list from these files and nowhere else. The format,
  what is rejected and the steps to re-enable a parked case are in
  [`nofastmath/README.md`](nofastmath/README.md).
  - The cited approval must be a key pinned in both resolvers
    (`NOFASTMATH_APPROVALS`: `SAS-35`). Its ledger heading must exist and
    name the key.
  - `run_corpus.py allowlist-approval --base <base>` (CI repo checks) fails
    an added id unless it is a loaded module case that declares
    `"reference": "nofastmath"`. So an id cannot be staged ahead of its
    case or left pointing at a parked one. Removing an id needs nothing.
- **No tolerances:**
  - The case cannot set `tolerances` or carry a `tolerance_exception`.
  - Its normal limits, used only by the calibration check, are the module
    defaults plus its declared artifact modes under the pinned
    `JSON_BOUND`.
  - File-level tightening of `tolerances` or `json_bound` does not count,
    and the case itself cannot set either. So a case cannot make the
    official reference "fail" on purpose.
- **Calibration gate (cross-target):** a case qualifies if the official
  reference **fails** its normal limits against the no-fast-math build on
  x86 **or** on macOS arm64. Its allowlist line declares those targets
  (`x86`, `macos`, or both), and each target's CI proves its own:
  `run_corpus.py compare <official> <nofastmath> --justify-nofastmath` in
  `ci.yml` (x86, gating), and in the advisory `reference-arm.yml` PR job and
  the nightly `reference-arm-main.yml` suite (macOS). Under the Mac CI policy
  a `macos` declaration needs the PR job's `justified on macos` result at the
  PR head as evidence, which the reviewer checks before merge.
  - The step works out its target from the official tree's build identity:
    x86_64, or aarch64 built by Apple clang. Any other build, such as the
    aarch64 Linux proxy, is an error.
  - A declared case whose official reference passes there fails CI
    (`NOT justified on <target>`). Cases declared only for the other target
    are reported as not gated.
  - Wherever it is declared, the case is compared exactly against the
    no-fast-math build on both targets.
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

The first five ids (wavetable render, the three originally approved
categories) are in `nofastmath/wavetable/render.md`.

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
