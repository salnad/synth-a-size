# Ledger

Append-only work log. Add a dated entry at the end for every work
session, with what changed, what was measured, the decisions made, and
anything blocked. Any tolerance change must cite a human approval
recorded here (see AGENTS.md, rule 6).

---

## 2026-10-20: M0 harness and scaffolding

**Changed**

- Vendored upstream at `636ca0ef517a4db087a6a08a6a8a5e704e21f836` into
  `/vital` (2,751 files, byte-identical). Excluded only the prebuilt
  Firebase binaries (`third_party/firebase_cpp_sdk/{libs,frameworks}`,
  318 files, about 137 MB); see UPSTREAM.md. Added
  `scripts/verify_upstream.sh`.
- Reference harness (`migration/reference`): CMake build of upstream's
  unity files with the headless defines, plus `ref-render` (preset + MIDI
  → float WAV + metadata JSON; mirrors `SynthPlugin::processBlock`) and
  `ref-module` (single-processor capture to NPY; registry: `filter` for
  all 8 models, `distortion`, `envelope`, `dc_filter`, `stereo_delay`).
- `audio-diff` (Rust): max abs error, RMS level difference, null
  residual, band-wise log-spectral distance; tolerances; exit codes 0/1/2;
  JSON reports.
- Corpus: 5 authored presets, 4 MIDI files, 8 render cases and 5 module
  cases (`corpus/cases.json`); `scripts/run_corpus.py` renders and
  compares trees.
- `/port` workspace: 12 crates (`sas-*`). The plugin stub is built on
  nice-plug 0.4.2 and exports VST3 and CLAP entry points.
- CI (`.github/workflows/ci.yml`): upstream/name/corpus checks; reference
  build, smoke render, corpus render, bit-exact determinism check,
  artifact upload; Rust fmt/clippy/build/test.

**Measured**

- Upstream builds unmodified with GCC 13.3 and JUCE 6.0.5 on Ubuntu
  24.04. Full reference build takes about 30 s on 8 cores; rendering
  1 s of audio takes about 0.1 s.
- All 20 preset × MIDI combinations render non-silent and finite, and
  are bit-identical across runs. The corpus render tree is bit-identical
  across two runs (15 outputs).
- The reference output depends on the host block size:
  `unison_pad_fx` × chords at block 64 vs 128 gives a −31 dB residual and
  a 0.6 dB LSD. Parity comparisons must use identical block sizes.
- Writing 16-bit instead of float output: residual −83 dB, LSD 0.08 dB.
  A useful scale for what "inaudible" looks like in these metrics.

**Decisions**

- Plugin framework: nice-plug, the community fork of nih-plug (see
  `decisions/plugin-framework.md`).
- Reference build deviations from upstream headless, all documented in
  `reference/README.md`: `JUCE_USE_CURL=0`, `JUCE_DSP_USE_SHARED_FFTW=0`
  (otherwise output depends on whether libfftw3f is installed),
  `ProjectInfo` version 1.0.6 to match the plugin (headless uses
  99999.9.9), `-O3 -ffast-math` without LTO, and HOME isolated per run.
- Initial parity tolerances (cases.json defaults): `max_abs_error` 1e-3,
  `rms_diff_db` 0.1, `residual_db` −60, `lsd_db` 0.5. Module cases are
  stricter (−80 to −100 dB residual). These are targets set before any
  Rust exists. They may be tightened freely; loosening needs recorded
  human approval.
- `audio-diff`'s spectral metric is band-based (1/6 octave, 80 dB
  per-signal range) rather than per-bin. Per-bin LSD on sparse spectra
  scored a 6 dB gain change as 1.7 dB.

**Measured after the first CI run: how much the reference moves by itself**

Two experiments show how much float noise alone changes each case,
without any change to the algorithm:

- *Cross-machine*: the same build recipe and compiler (GCC 13.3, Ubuntu
  24.04) on the GitHub runner vs a local AVX-512 Xeon. Renders are
  deterministic on each machine but not identical across them.
- *No fast-math*: a local `REF_FAST_MATH=OFF` build vs the default build.
  This is the closest proxy for a faithful port that rounds differently.

| Case | Cross-machine max abs / residual | No fast-math max abs / residual / LSD |
|---|---|---|
| `init_saw__single_note` | 7e-9 / −188 dB | 1e-7 / −136 dB / 0.000 |
| `init_saw__chords` | 1e-7 / −169 dB | **3.1e-3** / −67 dB / 0.007 |
| `init_saw__single_note_48k_b128` | 3e-9 / −196 dB | **2.7e-3** / −61 dB / 0.010 |
| `analog_filter_lfo__chords` | 5e-7 / −127 dB | 2.4e-4 / −80 dB / 0.001 |
| `analog_filter_lfo__velocity_96k_b512` | 3e-7 / −126 dB | 4.1e-5 / −84 dB / 0.001 |
| `unison_pad_fx__chords` | **3.0e-3** / −69 dB | **3.1e-3** / −61 dB / 0.045 |
| `line_wavetable_fold__velocity` | 2e-7 / −139 dB | 1.8e-4 / −86 dB / 0.001 |
| `mono_legato_glide__legato` | 3e-6 / −121 dB | **2.3e-3** / −66 dB / 0.002 |
| module `stereo_delay_impulse` | 1e-10 / −172 dB | **3.8e-4 / −63 dB** (case limits 1e-4 / −80) |
| other module cases | ≤ 8e-6 / ≤ −105 dB | ≤ 1.1e-5 / ≤ −104 dB |

Bold values break the current `cases.json` limits. `rms_diff_db` was
≤ 0.0001 dB everywhere, and LSD stayed ≤ 0.045 dB against a 0.5 limit.

**Open decision for a human: calibrate the sample-level tolerances
before M1 relies on them.** By the rules, the limits in `cases.json` are
unchanged. But a faithful but not bit-exact port is likely to miss the
default `max_abs_error` (1e-3) on 4 of 8 render cases, to sit right at
the `residual_db` (−60 dB) limit on two, and to miss the
`stereo_delay_impulse` module limits. The phase-blind metrics have a lot
of headroom. Options:

1. Keep the limits, and treat any miss as a numerics bug to remove
   (strictest; may force porting libstdc++/glibc rounding behaviour).
2. Recommended: set full-patch defaults from measured float noise with a
   margin: drop the default `max_abs_error`, use `residual_db` −50, and
   tighten `lsd_db` to 0.25 and `rms_diff_db` to 0.05. Give
   `stereo_delay_impulse` `residual_db` −55 with no `max_abs_error`.
   Keep the other module limits.
3. Per-case limits from the table above, each with 6 dB of margin over
   that case's no-fast-math residual (e.g. −61 dB measured → −55 dB
   limit).

**Open / next**

- Decide the tolerance calibration above before M1 parity tests start
  depending on `cases.json`.
- M1: `simd` and `dsp-core`. Add `ref-module` entries for operator
  processors first.
- Not yet verified: building the reference on macOS (not needed for
  CI); loading the plugin in FL Studio (M8).

---

## 2026-10-20: reference determinism fixed; tolerances recalibrated (owner-approved)

**Human approval:** the project owner approved the tolerance change on
2026-10-20, in reply to the open decision above: "drop the default per-sample
max-abs-error gate, set the residual threshold to −50 dB, and tighten the two
spectral metrics", recalibrated against the post-fix noise floor. This
entry is the record AGENTS.md rule 6 requires.

**Root cause of the cross-machine difference** (full evidence in
`decisions/reference-determinism.md`):

- *Not* the build: runner-built and VM-built binaries have byte-identical
  `.text`, and the runner's binary run on the VM reproduces the VM's
  output exactly. Also ruled out: `poly_float` width, threads/CPU count,
  environment/locale, uninitialised heap (`MALLOC_PERTURB_`), seeds,
  denormals, and scalar libm dispatch (probe-identical on both machines).
- **Cause 1:** `-ffast-math` makes GCC implement `poly_float` division
  (`_mm_div_ps`) and `sqrtf` as `RCPPS`/`RSQRTPS` plus a Newton step (294
  instructions in 87 functions). Those instructions return vendor-specific
  bits; the probe hashes differ between Intel and AMD and match between
  two AMD generations. `-mrecip=none` does not disable this in GCC 13;
  `-mno-recip` does.
- **Cause 2:** GCC auto-vectorised the Wave Folder's `sinf`/`asinf` into
  glibc libmvec SSE kernels, which use the same instructions internally.
  The libmvec `asinf`/`acosf` probes differ between Intel and AMD; this
  explained the one case the committed-hash check still caught.

**Changed**

- Reference flags: `-mno-recip`, `-U__FAST_MATH__` (no libmvec),
  `-march=x86-64 -mtune=generic -msse2 -mfpmath=sse -ffp-contract=off`.
- libm pinned to glibc's baseline variants via a self re-exec with
  `GLIBC_TUNABLES=glibc.cpu.hwcaps=-AVX,-AVX2,-FMA,-FMA4`. On one machine,
  masking FMA changes 13 of 14 double libm functions.
- Render metadata records `glibc`, `opt_flags` and `libm_variants`.
- New `ref-fp-probe`, `scripts/machine_info.sh` and
  `scripts/check_fp_determinism.sh`. CI uploads the binaries and
  diagnostics (`reference-binaries`).
- `corpus/reference_hashes.json` plus `run_corpus.py hashes --check` in CI:
  the cross-machine tripwire.
- New CI job `tolerance-calibration`: a no-fast-math reference must pass
  the corpus tolerances against the default reference.
- `cases.json` render defaults: `max_abs_error` gate removed,
  `residual_db` −60 → −50, `lsd_db` 0.5 → 0.1, `rms_diff_db` 0.1 → 0.01.
  Module limits unchanged.

**Measured**

- After the fix, all 15 corpus outputs are bit-identical on the Intel VM,
  an AMD EPYC 9V74 (Zen 4) runner and an AMD EPYC 7763 (Zen 3) runner.
  Before the libmvec fix, the EPYC 7763 matched 14 of 15.
- The fix itself moves the reference by up to 3.1e-3 (exact division,
  `unison_pad_fx`); dropping libmvec moves it by ≤ 2.4e-7.
- Faithful-port proxy (no-fast-math vs default) after the fix: worst
  residual −61.4 dB, worst LSD 0.043 dB, worst RMS difference 1.6e-5 dB.
  Modules ≤ −118 dB; `stereo_delay_impulse` went from −63 to −149 dB,
  because its divergence was entirely `RCPPS`. Margins of the new limits:
  residual 11.4 dB, LSD 2.3×, RMS ≈ 600× (set at a 0.01 dB perceptual
  floor).
- The remaining sample-level sensitivity is phase drift on sustained,
  bent or detuned notes (it grows linearly from note onset), not glitches.

**Open / next**

- M1: `simd` and `dsp-core`, ported with exact division and sqrt (no
  approximate reciprocals).

---

## 2026-10-20: harness scale-out for parallel module threads

**Changed**

- `ref-module`: modules self-register (`REGISTER_REF_MODULE`, in
  `reference/src/module_registry.h`), one file each under
  `reference/src/modules/<area>/<module>.cpp`, picked up by a CMake
  `CONFIGURE_DEPENDS` glob. The existing five modules moved there:
  `filters/synth_filter`, `filters/dc_filter`, `effects/distortion`,
  `effects/stereo_delay`, `modulation/envelope`. Duplicate names fail at
  startup.
- Module cases now live one file per module,
  `corpus/modules/<area>/<module>.json` (ids `<area>/<module>/<case>`). The
  five existing cases moved to `filters/digital_svf`, `filters/ladder`,
  `effects/distortion`, `effects/stereo_delay` and `modulation/envelope`,
  with identical specs and tolerances. `cases.json` gained
  `module_defaults` with the unchanged module limits (1e-4 / −80 dB).
  Cases may only be stricter unless they carry a `tolerance_exception`;
  `run_corpus.py` and `sas-parity` both enforce this.
- Expected hashes: `<module>.hashes.json` next to each case file. The
  render hashes stay in `reference_hashes.json`. Both use a multi-identity
  format (`entries: [{identity, hashes}]`), so another architecture can add
  its own entry.
- `run_corpus.py`: per-module case resolution, a capture layout of
  `modules/<id>/{spec,case,capture}.json` plus NPYs, a `list` command and
  per-file hash write/check.
- New crate `port/crates/parity` (`sas-parity`): resolves cases from
  `migration/corpus` with the same rules as `run_corpus.py`, locates
  captures (`SAS_PARITY_CAPTURES`, default `build/parity`) or renders them
  on demand, refuses stale captures (their `case.json` must equal its own
  resolution), and exposes `ModuleCase` / `RenderCase` / `assert_parity!`.
  On failure it dumps the port output and report to
  `build/parity-failures/`. Tolerances cannot be set from tests.
- CI: the `reference-harness` job uploads `parity-captures`. A new
  `parity` job runs `cargo test --workspace` against them, with
  `SAS_PARITY_NO_BUILD=1`. The `rust` job no longer runs tests; it keeps
  fmt, clippy and build.
- AGENTS.md "How to port a piece of the engine" rewritten as the module
  thread workflow.

**Measured**

- Refactored `ref-module` captures are bit-identical to before: all 15
  hash values are unchanged under the new keys.
- The `sas-parity` self-tests pass locally: every corpus case against its
  own reference, a perturbed output fails `max_abs_error`, a length
  mismatch fails, inputs and triggers match the spec, and the tolerance
  rules reject looser or unknown values.

**Open / next**

- Module threads add their capture files, case files, hashes and parity
  tests as described in AGENTS.md.

---

## 2026-10-20: harness scale-out review fixes (Parity Reviewer, PR #3) and exact-match artifacts

**Review fixes** (each one has a regression test in
`migration/scripts/test_run_corpus.py`, run in CI, and in sas-parity):

- B1: `tolerance_exception` is now structured and case-level only:
  `{"approved_in": "<LEDGER.md heading>", "tolerances": {...}}`. Every
  loosened metric must equal its approved value, every approved metric must
  actually be loosened, and the cited ledger section must exist and name
  the case id. Free text, file-level exceptions and loosening past an
  approval are rejected, in both resolvers.
- B2: the approved floor is pinned in code in both resolvers. Render floor:
  `residual_db` −50, `lsd_db` 0.1, `rms_diff_db` 0.01. Module floor:
  `max_abs_error` 1e-4, `residual_db` −80. Defaults that are missing a floor
  metric, or looser than it, are rejected. `cases.json` keys are validated.
- B3: `run_corpus.py render` deletes every earlier output of a case before
  rendering and writes `case.json` only when the render exits 0.
- B4: when the tree's build identity is known (it has render hashes), a
  module hash file without that identity is an error. AGENTS.md now says
  hashes are written from a CI-identity tree (the PR's `reference-renders`
  artifact). `hashes --write` refuses unknown identities unless given
  `--new-identity`.
- N1: hashes are computed from the files themselves (WAV data, NPY
  payload, artifact bytes), not read from `capture.json`. sas-parity checks
  every loaded reference output, render and artifact against the committed
  hash for its build identity, and every input against `capture.json`.
- N2: `parity_report` and `report()` are `#[doc(hidden)]` and documented as
  debugging-only.
- N3: CI runs `hashes --check --require-known-identity`. In CI
  (`SAS_PARITY_NO_BUILD`), sas-parity rejects captures whose build identity
  has no committed hashes.
- N4: Python rejects NaN and Infinity (`parse_constant`), and render cases
  and defaults reject unknown keys, matching Rust.
- N5: a lock left by a dead process is taken over. `ref-module --list`
  shows repository-relative source paths. `--only` matches exact ids or
  path prefixes.
- Found while testing: the harness hash has always used offset basis
  1469598103934665603 (one digit short of the standard FNV-1a basis). It is
  kept to avoid churning every committed hash, documented, and pinned by
  test vectors in both Python and Rust.

**Exact-match artifacts** (requested for Wavetable Creator and Preset
Registry):

- `ModuleDef` gains an optional artifact hook:
  `sink.text(name, content)` writes `artifact_<name>`, and
  `sink.array(name, data, channels)` writes extra NPY outputs.
  Artifact-only modules (`create = nullptr`) are allowed.
  `ref-module --corpus-dir` gives hooks access to corpus input files.
- Artifacts are recorded in `capture.json`, pinned in the hash files as
  `<case>:artifact/<name>`, compared exactly by `run_corpus.py compare`, and
  asserted with `assert_artifact!(id, name, actual)` /
  `ModuleCase::assert_artifact` in sas-parity (byte-equal, no tolerance).
- Self-test module `harness_echo` (`harness/echo/params_and_file`) covers
  the path end to end.

**Measured**

- The existing 15 hashes are unchanged; `harness/echo` adds 3.
- Python regression tests: 19 pass. sas-parity: 18 unit tests, 8 self-checks
  and the tamper test pass.

---

## 2026-10-20: harness follow-ups from the PR #3 re-review

- Reviewer note 2: the ledger check for `tolerance_exception` now requires
  the cited section to name the case id as a whole token, so the characters
  on either side must not be letters, digits, `_`, `-` or `/`. Before, it
  was a substring match, so `filters/x/ab` satisfied an approval for
  `filters/x/a`. The change is mirrored in `run_corpus.py`
  (`names_case`) and `sas-parity` (`cases::names_case`), with the same
  regression table in both test suites.
- Reviewer note 3: AGENTS.md now says numeric data goes through
  `sink.array` (tolerances apply), and text artifacts must be upstream's
  own serialization, never reduced-precision prints.

---

## 2026-10-20: run_corpus compare handles artifact-only modules

- Bug found by Wavetable Creator (PR #4, CI run 39514821022, cases
  `common/json/*`): `run_corpus.py compare` reported
  `ERROR <case> (no reference outputs)` for module cases that emit only
  text artifacts. That broke the reference-harness determinism step and the
  calibration compare for artifact-only modules.
- Fix: a case has outputs if it has `out_*.npy` or `artifact_*` files.
  Text artifacts are compared byte for byte in every compare, determinism
  and calibration alike. An artifact present in only one of the two trees
  is an error in either direction.
- Regression tests (`test_run_corpus.py`) use a text-only fake case:
  identical trees pass with and without `--exact`, an edited artifact
  fails, missing or extra artifacts error, and audio-diff is never invoked.
  The main test fails on the old code with the reported message.

---

## 2026-10-20: M1 PR 1, `sas-simd` (poly_float, poly_utils, futils)

**Changed**

- `port/crates/simd`: `PolyFloat`/`PolyInt`/`PolyMask` over a `Backend`
  trait with three implementations: SSE2 (x86_64), NEON (aarch64, the
  shipping arm64 macOS target) and a scalar model of each upstream path
  (`Scalar` = SSE2 semantics, `ScalarNeon` = NEON semantics). Ported
  `poly_values.h`, `poly_utils.h`, `futils.h`, `matrix.h`, `common.h` and
  the scalar part of `utils.h`/`utils.cpp` (the RNG engine and PCM
  conversion live in `sas-common`, owned by Wavetable Creator).
- Each backend follows its own upstream path, because upstream's SSE2 and
  NEON paths are different arithmetic: float `max`/`min` (MAXPS vs FMAX NaN
  and ±0 rules), `notEqual` (unordered vs ordered), horizontal sum order
  (`(v0+v2)+(v1+v3)` vs `(v0+v1)+(v2+v3)`), `toInt` (round-to-nearest-even
  with `0x80000000` vs truncate-and-saturate), int `min` (signed vs
  unsigned) and `anyMask` (byte mask vs max lane). Table in
  `crates/simd/src/backend/mod.rs`.
- No FMA and no estimate instructions anywhere: `mul_add`/`mul_sub` are a
  separate multiply and add; division and `sqrt` are exact.
- Matrix moved into PR 1 (not PR 2) because `poly_utils` depends on it.

**Measured**

- Native vs scalar model, bit for bit: ~22k float-quad cases and ~6k int
  cases over every op, on x86_64 (SSE2 vs `Scalar`) and on aarch64 under
  qemu (NEON vs `ScalarNeon`). The NaN payload of `+ - * /` with two NaN
  operands is compared as NaN-ness, because compilers may commute these
  ops (in the port and the reference alike).
- Standalone C++ probe (local; superseded by the `simd/poly_function`
  ref-module below): upstream's own headers, 1,257 input quads, 100
  functions:
  - x86_64, `-O3 -ffp-contract=off -msse2`, no fast-math: **100/100
    bit-identical** to the Rust SSE2 backend.
  - aarch64 (g++ 13 cross, run under qemu), no fast-math: **100/100
    bit-identical** to the Rust NEON backend.
  - With the reference's `-ffast-math -U__FAST_MATH__ -mno-recip`, 38
    functions differ by a few ULP (reassociation and reciprocal-math). This
    is the "faithful port vs fast-math" noise the tolerance proxy already
    covers, not a port difference.
- futils accuracy pins (they guard against coefficient typos; they are not
  parity tolerances): exp2 relative error < 1e-5, log2 absolute < 1e-3
  (measured 4.8e-4), tanh absolute < 1e-3.

**Reference parity (the gate).** The probe is now a ref-module,
`simd/poly_function` (`migration/reference/src/modules/simd/poly_function.cpp`,
cases in `migration/corpus/modules/simd/poly_function.json`): 127 cases,
one per upstream function. It covers every poly op, the poly_utils
conversions, shuffles, interpolation and libm maps, poly futils, and the
mono futils/utils overloads. Each case runs on 4,096 frames of stereo noise
over that function's working range, chosen so outputs stay audio-scale
(|out| ≲ 16). `port/crates/simd/tests/parity.rs` runs the port on the
captured inputs through `sas-parity` under `module_defaults`
(`max_abs_error` 1e-4, `residual_db` −80). Against the official pinned
fast-math x86_64 reference:

- 127/127 pass; 90/127 are bit-identical.
- Worst `max_abs_error` is 7.6e-6 (`mono_frequency_to_midi_note`), 13×
  under the limit. Worst `residual_db` is −132.0 dB (`catmull_row0`),
  52 dB under the limit.
- The cases that are not bit-identical are fast-math reassociation in the
  reference: polynomial/Catmull matrices, the tanh/sat family, libm-based
  `powf`/`logf` maps.
- Extra, not the gate: against the `REF_FAST_MATH=OFF` reference, 127/127
  are bit-identical. The no-fast-math proxy also passes all 127 cases
  under the tolerances, as the calibration job requires.
- arm64: the same test runs on aarch64 against arm64 captures once the
  macOS arm64 job runs `cargo test` against them (ARM Reference). The g++
  cross build under qemu (above) is only a proxy for Apple clang.

**Mono vs poly futils (decision, coordinator-approved).** Each overload
follows its own upstream formula. `tests/futils_mono_vs_poly.rs` requires
bit equality where upstream's arithmetic is identical (exp2, log2, exp,
log, exp_half, pow, magnitudeToDb, quickTanh, algebraicSat, quickSin, sin,
quickSin1, sin1). It also checks that the port keeps the overloads that
upstream itself makes differ (line numbers in `futils.h`):

| Function | Mono | Poly | Difference |
|---|---|---|---|
| `dbToMagnitude` | 168–170 | 172–174 | mono uses natural `exp` (extra log2(e) scale, an upstream bug kept for parity) |
| `quickTanhDerivative` | 199–206 | 190–197 | numerator summed in a different order |
| `algebraicSatDerivative` | 243–250 | 252–260 | numerator summed in a different order |
| `tanh` | 275–284 | 262–273 | different association |
| `powerScale` | 360–369 | 371–378 | bypass threshold 0.01 vs 0.005; poly bypass goes through `maskLoad` (−0 → +0) |

**Per-ISA semantics (`decisions/reference-arm.md`).** The enumerated sites
live in `sas-simd` only. A1 is the poly `to_int` (rounding on x86_64,
truncation on aarch64), part of the backend lane semantics. A2/A3 are the
scalar `(uint32_t)float` / `(int32_t)float` casts, in
`sas_simd::conversions` (`float_to_u32`, `float_to_i32`, with explicit
`x86_64::` and `aarch64::` variants). The cargo feature
`aarch64-semantics` makes an x86_64 build use the aarch64 column (via the
scalar NEON model, which is bit-identical to the NEON backend under qemu),
and `sas_simd::AARCH64_SEMANTICS` reports which one a build uses. Tests
cover both columns on every host. A new CI job runs the `sas-simd` tests
with the feature on x86_64. The parity test refuses captures whose build
arch doesn't match the build's semantics. Five more poly lane operations
differ between upstream's SSE2 and NEON code and are mirrored per target
in the same backend table: horizontal-sum order, float max/min NaN and ±0
handling, `notEqual` NaN ordering, int `min` signedness, and `anyMask`.
They are not yet in the decision's section A table; that decision is open
with the coordinator.

**Notes for other threads**

- `poly_utils::trunc`/`to_int` round on x86 and truncate on arm64, as
  upstream does. `floor`/`ceil`/`round` agree on both for in-range values.
- `utils::equal(NaN, NaN)` is false on x86 and true on arm64 (upstream).
- Upstream runs the engine under JUCE `ScopedNoDenormals` (FTZ/DAZ on x86,
  FZ on arm64). The port has to set the same mode around processing (M1 PR 2
  or M7) for parity on paths that produce denormals.

**Open / next**

- PR 2: processor graph, operators, lookup tables, utilities,
  one_pole_filter.
- arm64 parity in CI: the macOS arm64 job needs to run
  `cargo test -p sas-simd --test parity` against its arm64 captures. That
  workflow belongs to ARM Reference.

---

## 2026-10-20: case names restricted to [A-Za-z0-9_-]

- Follow-up (reviewer note on #12, landed with #10): an `out_*.npy` present
  only in the test tree is now an error ("missing in reference tree"),
  symmetric with artifacts. Test: `test_compare_flags_array_missing_in_reference`.

- Reviewer nit after #9: because `.` and `:` count as token boundaries, a
  case named `…/a.b` or `…/a:x` would satisfy a ledger approval for `…/a`.
  Area, module, case and render names must now match `[A-Za-z0-9_-]+` and
  are rejected at load in both `run_corpus.py` (`check_name`) and
  `sas-parity` (`cases::check_name`). Both test suites test this.
- All existing case files comply: 6 module cases and 8 render cases.

---

## 2026-10-20: M1 PR 2, `sas-dsp-core` (processor graph, operators, utilities)

**Changed**

- `port/crates/dsp-core` ports:
  - `framework/processor`, `processor_router`, `feedback`, `value`,
    `operators`, `synth_module` and `circular_queue`;
  - `lookups/lookup_table.h` and `lookups/memory.h`;
  - `utilities/*` (smooth values, value switch, portamento slope, legato
    filter, peak meter);
  - `filters/one_pole_filter.h`;
  - `ScopedNoDenormals` (JUCE's flush-to-zero guard; the engine must hold it
    around processing as upstream does);
  - `testing::run_module`, a step-for-step port of `ref-module`'s driving
    sequence for module threads' parity tests.
- Design (crate docs): a `Processor` node held as `Rc` with a
  `ProcessorImpl` kernel. Clones share state, inputs and outputs and copy
  the kernel, as upstream's copy constructor does. Buffers are
  `Rc<[Cell<PolyFloat>]>`, so upstream's buffer redirection and aliasing
  are safe. Routing tables, module data and `VoiceHandler`'s router
  overrides (`RouterHooks`) live on the node, so graph edits never re-enter
  a running kernel.
- Upstream behaviours kept deliberately (all verified against reference
  captures):
  - `reorder` bumps the router's own change counter, so the router it runs
    on keeps its insertion order; only copies rebuild from the reordered
    global order.
  - Cycles always get a control-rate `cr::Feedback`.
  - `Interpolate` is stateless (shared between voices) despite `fraction_`.
  - `getDependencies` walks owned inputs.
  - The scalar float→int casts at `ValueSwitch::setSource` and `PeakMeter`
    go through `sas_simd::conversions` (A2/A3; in range in practice).
- Not yet here: `utils::RandomGenerator`, waiting for `sas-common` (#4).
  The `synth_constants` values the framework needs are copied in
  `dsp-core::constants`; the parameter registry is reached through
  `synth_module::set_parameter_lookup`.

**Reference parity (the gate)**

New ref-modules in `migration/reference/src/modules/framework/`
(`operators.cpp`, `values.cpp`, `graph.cpp`, `lookups.cpp`) cover:
- every operator and `cr::` operator;
- `Value`/`cr::Value` triggers, `SmoothValue`, `cr::SmoothValue`,
  `PortamentoSlope` and `PeakMeter`;
- three `ProcessorRouter` graphs: `ModulationSum` with control and audio
  sources, a control-rate feedback cycle, and an audio chain added in
  reverse order;
- probes for `Memory`, `StereoMemory`, `OneDimLookup` and `OnePoleFilter`
  (plain and `futils::tanh`).

Cases are in `migration/corpus/modules/framework/*.json`.
`port/crates/dsp-core/tests/parity.rs` mirrors the module table and runs
under `module_defaults`. Against the official pinned fast-math x86_64
reference:

- 75/75 outputs (73 cases) pass; 64/75 are bit-identical.
- Worst `max_abs_error` 2.3e-5 (`operators/tempo_chooser_sync_4`, the
  keytrack frequency path through libm `powf`), 4.4× under the limit.
  Worst `residual_db` −122.9 dB
  (`operators/tempo_chooser_keytrack_phase_increment`), 43 dB under the
  limit.
- The non-identical outputs are fast-math reassociation in the reference:
  Catmull matrices in `Memory`/`OneDimLookup`, `futils::tanh` saturation,
  `powf`/`logf` maps. `values/peak_meter_burst:memory_peak` differs only in
  16 signed zeros (`-fno-signed-zeros`), with zero error.
- The no-fast-math proxy passes all of them under the tolerances
  (calibration job).

**Parity review fixes (PR #11): full-range coverage**

The first revision narrowed the keytrack case to notes 0–48, because above
about 130 Hz the fast-math reference's own ULP noise on a raw Hz value
exceeds the absolute 1e-4 gate. The review rejected that as reduced
coverage. Rule, now also in `migration/AGENTS.md`: quantities with a large
natural scale (Hz, samples, ms) cover their full working range. Where the
raw value cannot pass, they are captured in the normalized form their
upstream consumer uses, computed identically in the ref-module and the
parity test (an f32 multiply by the f32 reciprocal). Inputs are never
narrowed to fit the gate, and if no faithful form exists, record it and ask.

Applied in this PR, to `framework/*` and to the `simd/poly_function` cases
merged with #5. All under `module_defaults`, no tolerance changed:

| Case | Range | Captured as | max abs error |
|---|---|---|---|
| `operators/tempo_chooser_sync_4` (kept) | keytrack notes 0.3–48.3 | Hz | 2.3e-5 |
| `operators/tempo_chooser_keytrack_phase_increment` (new) | MIDI 48–127, transpose +24, tune 0.3 (notes 72.3–151.3, up to 47.6 kHz) | SynthLfo phase increment `frequency * (1.0f / getSampleRate())` (`synth_lfo.cpp:411-413`) | 1.4e-6 |
| `operators/cr_midi_scale` (kept) | 0–4800 cents | Hz | 0 (bit-identical) |
| `operators/cr_midi_scale_phase_increment` (new) | 0–13600 cents (MIDI 0–136, up to 18.7 kHz) | same phase increment | 2.4e-7 |
| `operators/cr_frequency_to_phase`, `cr_frequency_to_samples` | 8 Hz–20 kHz (was 20/200–2000) | raw | 0 (bit-identical) |
| `operators/cr_time_to_samples` | 0–4 s (was 0–0.01) | raw | 0 (bit-identical) |
| `poly_function/{poly,mono}_midi_note_to_frequency` | notes −36–48 (was −36–12) | Hz | 3.8e-5 / 4.6e-5 |
| `poly_function/{poly,mono}_midi_note_to_normalized_frequency` (new) | notes 0–136 (up to 21.1 kHz), b = 44100 | `frequency * (1.0f / sample rate)`, as the filters consume it (`digital_svf.cpp:81`) | 3.6e-7 |
| `poly_function/midi_note_to_frequency` (futils) | notes 0–136 | raw | 0 (bit-identical) |
| `poly_function/{poly,mono}_cents_to_ratio` | ±4800 cents (was ±2400) | raw | 2.9e-6 |
| `poly_function/midi_offset_to_ratio`, `mono_note_offset_to_ratio` | ±48 semitones (was ±24) | raw | 0 / 2.9e-6 |
| `poly_function/{poly,mono}_frequency_to_midi_note` | 8 Hz–21 kHz (was 1–16 Hz) | raw | 3.1e-5 / 2.3e-5 |
| `poly_function/*db_to_magnitude` (4 cases) | ±30 dB (was ±24, mono ±16) | raw | ≤ 6.1e-5 (`mono_db_to_magnitude`) |
| `poly_function/*magnitude_to_db` (4 cases) | 0.001–31.6 (was 0.1–3.9) | raw | 0 (bit-identical) |

Raw-Hz numbers for the bands that are captured normalized (fast-math
reference against the port; the reviewer measured the port identical to the
no-fast-math proxy):

- keytrack, notes 72.3–151.3: 6.25e-2 Hz max, residual −122.9 dB. The
  reviewer's run, notes 0–127 with tune 0.3, gave 3.4e-3 Hz (7.5e-7
  relative, 11 ULP). By band: 1.5e-5 for notes 0–48, 1.2e-4 for 48–72,
  and 3.4e-3 for 72–127.
- `cr::MidiScale`, 0–13600 cents: 9.8e-3 Hz max, −128.5 dB.
- `{poly,mono}_midi_note_to_frequency`, notes 0–136: 1.56e-2 Hz max,
  −128.2 / −128.0 dB.

`cr::MidiScale` has no consumer upstream (nothing instantiates it). Its
normalized case therefore uses the form every upstream consumer of a
MIDI-derived frequency applies, `* (1.0f / getSampleRate())` (SynthLfo,
RandomLfo and the filters' cutoffs). Flagged for the reviewer.

Also new: `poly_function/int_min` (reference-arm.md A7, signed vs unsigned
`poly_int::min` on negative inputs) and `poly_function/any_mask` (A8,
`PMOVMSKB` byte mask vs lane maximum). Both are bit-identical against the
x86_64 reference; the arm64 column runs in the macOS job. `simd/poly_function`
now has 131 cases, 92 bit-identical. Worst `max_abs_error` is 6.1e-5
(`mono_db_to_magnitude`, 1.6× under), and the worst residual −127.9 dB
(`mono_midi_note_to_normalized_frequency`).

**Open / next**

- `RandomGenerator` once `sas-common` merges.
- arm64 parity for these cases runs in the macOS arm64 job once it has a
  `cargo test` step (ARM Reference).

---

## 2026-10-20: M4a `sas-common` (shared exact helpers)

New dependency-free crate `port/crates/common` (`sas-common`), owned by the
Wavetable Creator thread until this first PR merges. Core Framework, Preset
Registry and Modulators reuse it instead of porting these pieces again
(coordinator decision).

**Added**

- `json`: JSON with nlohmann/json 3.1.2 semantics (sorted keys, typed
  integers vs. doubles, `f32` stored as double, a line-by-line port of its
  Grisu2 double formatting, first duplicate key wins, `operator[]`/`get<T>`
  read rules as `get`/`at`/`to_*` returning errors where upstream throws).
  Arithmetic conversions follow nlohmann's three paths, checked against
  the reference: `float`/`int` (`to_f32`/`to_i32`) accept booleans as 0/1;
  `double`/`int64_t` (`to_f64`/`to_i64`) and `int`-backed enums
  (`to_enum`) reject them. So `LineGenerator` accepts boolean points,
  powers and `num_points`, as upstream does (found by Preset Registry).
- `base64`: JUCE `Base64` (decoding stops at the first invalid character
  and keeps the bytes decoded so far).
- `scalar`: single-lane `futils` (`exp2`, `log2`, `exp`, `log`, `pow`,
  `powerScale`, `tanh`), `utils` (`clamp`, `iclamp`, `ilog2`,
  `interpolate`, PCM conversions) and the Catmull-Rom weights from
  `poly_utils`. **These duplicate parts of `futils.h`/`utils.h`**; Core
  Framework's `PolyFloat` versions must match them bit for bit in every
  lane (a test comparing the two is welcome once `simd` lands). Note that
  upstream's `truncToInt` is `_mm_cvtps_epi32` (round to nearest even), so
  `utils::trunc` is not a truncation; `floor`/`roundToInt` are still exact.
- `random`: `std::mt19937` plus libstdc++ 13's
  `uniform_real_distribution<float>` (one draw, `float(u32) / 2^32`, 1.0
  mapped to `nextafter(1, 0)`), constructed from an explicit seed.
- `line_generator`: `LineGenerator` with upstream JSON semantics.
- `load_save`: `compareVersionStrings`, `convertBufferToPcm`,
  `convertPcmToFloatBuffer` and `VERSION_STRING = "1.0.6"` (the reference
  build's `ProjectInfo::versionString`), needed by wavetable and preset
  upgrades.

**Measured** (scratch probe linked against the reference engine library;
it is not committed. The committed parity captures wait for the
Harness Builder's `sas-parity` helper.)

- Against the `REF_FAST_MATH=OFF` reference build: **bit-identical** on all
  checks. That covers 18,580 `exp2`/`tanh` and 9,261 `log2` inputs, 8,181
  `powerScale` pairs, 36,000 RNG values over 18 seed/range combinations,
  54 `LineGenerator` cases (9 authored JSONs × resolutions 2048/128/7 ×
  loop on/off) plus the 6 init shapes, covering buffers, `valueAtPhase`,
  `stateToJson` dumps and `linear()`, along with 43,922 JSON doubles,
  parse→dump texts, 225 version comparisons and base64 edge cases.
- Against the default (fast-math) reference: the RNG, PCM, JSON dumps,
  versions and base64 are bit-identical. The float helpers differ only by
  GCC reassociation: `tanh` ≤ 1 ulp, `log2` ≤ 2 ulp, `powerScale` ≤ 1.0e-6
  absolute, line buffers ≤ 3.0e-7, and `valueAtPhase` ≤ 8.5e-5, the
  largest on steep segments where the rounded phase index lands on the
  other side of a breakpoint. For example, fast-math folds the linear
  ramp's `1 - (1 - t)` into `t`.

**Decisions**

- Deliberate deviation for degenerate input: the fast-math reference also
  folds `isfinite`. So it parses `1E400` as infinity and dumps NaN/inf as
  a garbage double, and its DAZ flag makes subnormal doubles dump as
  `0.0`. The port follows the no-fast-math semantics here, which are
  nlohmann's documented behaviour: overflow is a parse error, NaN/inf
  dump as `null` and subnormals print normally. Authored content never
  hits these cases.

**Open / next**

- Parity captures and tests: done, see the next entry and the review
  follow-up below.
- Wavetable state model (creator/group/component/keyframe JSON) and the
  FFT next, then rendering.

---

## 2026-10-20: M4a `sas-common` parity captures

**Added**

- Artifact-only `ref-module` modules in `migration/reference/src/modules/common/`:
  - `line_generator`: `jsonToState` → render → `stateToJson`; arrays
    `buffer` and `value_at_phase`, text `saved.json`;
  - `random_generator`: `RandomGenerator` after `seed()`; array `values`;
  - `futils_scalar`: scalar `exp2`/`log2`/`tanh`/`powerScale`/Catmull-Rom
    over a fixed integer-defined grid. `exp2(e)` is captured as
    `exp2(e) · 2^-floor(e)`, an exact power-of-two scaling into [1, 2].
    Raw values reach ~5.7e3, where one float ulp (~4.9e-4) exceeds the
    1e-4 absolute gate, so only bit-exact references could pass. Every
    other array in this crate stays below 100;
  - `json_serializer`: nlohmann `parse` → `dump()`/`dump(2)` of authored
    texts, and `dump()` of 40,000 generated doubles and floats. These are
    upstream's own serialization.
- JSON inputs are string params, so the reference and the port parse the
  same bytes.
- Case files `migration/corpus/modules/common/{line_generator,random,futils,json}.json`
  (37 cases) with their hashes.
- Parity tests in `port/crates/common/tests/parity.rs`. They loop over
  every case of each file. Numbers use `assert_parity!` with the default
  module tolerances (1e-4, −80 dB); saved JSON uses `assert_artifact!`.

**Measured against the official (fast-math) reference**

- All pass with the default module tolerances; no tolerance changes.
- RNG sequences, every JSON text artifact (`saved.json`, `dump.json`,
  `pretty.json`, `floats.json`) and the Catmull-Rom outputs are exact.
- `exp2` (scaled): exact. `tanh`, `log2`: ≤ 2 ulp. `powerScale`: ≤ 1.3e-6
  (−138 dB). Line buffers: ≤ 3.0e-7 (≤ −144 dB).
- `valueAtPhase` on `steps`/`steps_loop` (vertical segments): 8.5e-5 and
  −92 dB. This is the closest to a limit (1.17× under `max_abs_error`).
  The cause is fast-math reassociation of the phase-to-index arithmetic,
  so one interpolation lands on the other side of a breakpoint. It is 0
  against the no-fast-math build.
- **Decided 2026-10-21 (source semantics, ULP-bounded JSON); `write_f64`
  now tests `is_sign_negative()` (#8).** Formerly a fast-math mirror site,
  `json/dtoa.rs` `write_f64`:
  - Upstream's source tests `std::signbit(value)`
    (`vital/third_party/json/json.h:8205`). The fast-math build folds that
    to `value < 0`, so `-0.0` prints as `0.0`; the no-fast-math build
    prints `-0.0`.
  - The port mirrors the fast-math codegen (`value < 0.0`), and the
    `fmt(-0.0)` unit test pins that pending choice.
  - The case `common/json/negative_zero` is authored in
    `migration/corpus/modules/common/pending/json.json` (not loaded by the
    harness), because no exact artifact can pass the calibration job. To
    re-enable it, move the case into `common/json.json` and write hashes.
  - The case is re-enabled as a JSON-bounded artifact once the harness
    compares JSON with `-0.0` equal to `0.0` (see the creator JSON entry's
    decision table).
- Expected hashes come from CI run 39514821022's `reference-renders`
  artifact. Local hashes are identical for `futils`, `line_generator` and
  `random`. `json` was re-written locally after the `-0.0` change, and the
  next CI run confirms it.
- `common/json/*` cases emit only text artifacts. CI compares them since
  PR #12 ("run_corpus compare handles artifact-only modules", above).
- **RNG per target C++ library (owner decision; ARM Reference records it
  as A9).** libc++'s `generate_canonical<float, 24>` has no clamp to just
  below 1. For a draw u >= 2^32 - 128 (about 3e-8 per draw) it returns
  1.0, so `uniform_real_distribution` returns exactly `max`; libstdc++
  returns `max - (max - min)·2^-24`.
  - The source comparison is LLVM 17.0.6 and main libc++ against
    libstdc++ 13.
  - `random::canonical_from_draw` follows `AARCH64_SEMANTICS`: the libc++
    column on aarch64, and with the `aarch64-semantics` feature (same name
    as `sas-simd`'s, which crates forward).
  - Unit tests cover both columns at the edge draw on every host.
  - None of the 26,624 draws in `common/random` reach the edge, so its
    captures are the same on both references.
  - Follow-up (Chain Reviewer's suggestion (a)): the case
    `common/random/seed_10694_edge_draw` (4096 draws) hits u ≥ 2^32 − 128
    at index 745 (u = 4294967238). An independent mt19937 confirms that
    it is the only such draw in the case.
  - The two columns differ there by only (max − min)·2^-24 ≈ 1.2e-7, well
    inside the gate. So `random_generator` gains an exact output,
    `at_max`, which counts values equal to `max`: 0 for libstdc++ and 1
    for libc++ at this seed.
  - With `aarch64-semantics` on x86, the test fails `at_max` (error 1),
    as intended. The x86 reference gives 0.9999999 (1 − 2^-23) at index
    745, and the case will check the libc++ column once arm64 captures
    exist.

### Review follow-up (Chain Reviewer, PR #4)

- **x86 `toInt` semantics are no longer public.**
  `scalar::round_to_i32_sse` and `scalar::poly_trunc` implement only the
  SSE2 column of A1, so they are now `pub(crate)`. Their public callers,
  `poly_floor` and `round_to_int` (and `exp2` through them), give the same
  result on both ISAs for non-NaN |x| < 2^31. A unit test checks both
  against a truncating (NEON) model over more than 100k inputs.
- **New captures for the surface that had only probe evidence.** Each is
  an artifact-only module with its own case file and committed hashes.
  - `load_save_codecs` (`modules/common/load_save.cpp`, 21 cases):
    - JUCE `Base64::toBase64` text (6 cases);
    - `convertFromBase64`, with the returned flag and the bytes kept on
      early stop: invalid characters, truncated groups, spaces, padding
      in the middle, URL-safe and non-ASCII characters (13 cases);
    - `convertBufferToPcm`/`convertPcmToFloatBuffer`: texts `pcm.json`
      and `float.json` plus the decoded PCM (int16/32768) and floats as
      arrays, including clamping above ±1;
    - `compareVersionStrings`: 19 pairs, including whitespace, non-numeric
      components and overflow.
  - `line_generator_edit` (`modules/common/line_generator_edit.cpp`, 19
    cases):
    - every `init*` shape, with and without loop;
    - sequences of `addPoint`, `addMiddlePoint`, `removePoint`,
      `flipHorizontal`, `flipVertical`, `setPoint`, `setPower`,
      `setNumPoints`, `setSmooth` and `setLoop`;
    - outputs: arrays buffer, `valueAtPhase` and state (`linear()`, point
      count), and text `saved.json`.
  - `futils_scalar` gains `exp` (|x| ≤ 80, the function's float range),
    `log` and `pow` (8 exponents). Wide-range results are split into an
    exact mantissa in [1, 2) and an integer exponent.
  - `json_accessors` (`modules/common/json_accessors.cpp`):
    - nlohmann's `get<bool/double/float/int/int64_t/std::string>` and an
      unscoped enum, on 24 values (booleans, null, strings, containers,
      fractions, int and uint extremes) and 12 enum inputs;
    - the results are serialized by nlohmann itself, with null where it
      throws;
    - inputs avoid undefined conversions (floats outside int range,
      negative floats into an enum).
- **Measured against the official reference:**
  - base64, PCM, version comparison, `exp`/`log`/`pow` and the accessors
    are exact;
  - `line_generator_edit` is within 3.9e-7, at or below −135 dB;
  - every text artifact is byte-identical;
  - the no-fast-math proxy passes all 205 `common` comparisons.
- **Formerly pending, decided 2026-10-21 (ULP-bounded JSON):**
  - `flipHorizontal` negates zero powers, so three edit cases save `-0.0`.
    That hits the same `write_f64` site as `negative_zero` above.
  - Their numeric arrays stay live, with `saved_json: false` in
    `line_generator_edit.json`.
  - Their `saved.json` variants are authored in
    `migration/corpus/modules/common/pending/line_generator_edit.json`,
    and are re-enabled as JSON-bounded artifacts with the harness support.
- **Nothing left unchecked.** All of the reviewer's list is now captured.
  `Value::get`/`at` on non-objects and `to_str` errors are covered by unit
  tests only; upstream throws there, and the port returns the error.
- `LineGenerator::from_json` rejects `num_points == 0` with an error.
  Upstream's behaviour is undefined there; the port used to panic.

---

## 2026-10-20: M4a wavetable state model, FFT and WaveFrame

`sas-wavetable` now depends only on `sas-common` (the unused `sas-dsp-core`
dependency was dropped), so `sas-preset` can use it for exact wavetable JSON.

**Added**

- `fourier_transform`: `FourierTransform` on the backend the reference
  pins, JUCE's `FFTFallback`. It is the recursive mixed-radix FFT (radices
  4,4,4,4,4,2 for 2048 points) with twiddles computed in double. Forward
  output is the full unnormalised complex spectrum. The inverse rebuilds
  bins above Nyquist as conjugates, which also negates the Nyquist bin's
  imaginary part, scales by 1/N and leaves the imaginary residue in the
  second half.
- `complex`: `std::complex<float>` with upstream source semantics
  (`abs` = `cabsf`/`hypotf`, `arg` = `atan2f`, textbook products). The
  fast-math reference expands `abs` to `sqrt(re² + im²)`; that differs by
  ≤ 1 ulp (812/5,000 probe values), which the tolerances absorb, so the
  source semantics are kept. Division became libgcc's `__divsc3` in #8,
  see the rendering entry.
- `wave_frame`: `WaveFrame` and `PredefinedWaveFrames`.
- `creator`: the `WavetableCreator`/group/component/keyframe state for all
  10 component types, with `stateToJson`/`jsonToState`. That covers
  keyframe creation by interpolation (a Line Source keyframe without a
  `line` inherits the interpolated one), every keyframe `copy`/
  `interpolate`/`smoothInterpolate`, `updateJson` (0.3.3, 0.3.7, 0.3.8,
  0.3.9, 0.4.7 and 0.7.7 steps), the line-shape-as-wavetable path, `init`,
  and `initPredefinedWaves`. Enum-valued settings are stored as the raw
  integers so they round-trip exactly.

**Measured** (scratch probe linked against the reference engine library).
Committed reference coverage:
- the creator JSON captures (next entry);
- the FFT and `WaveFrame`, through #8's `wavetable_render` captures. Every
  rendered frame and harmonic passes through both, and those cases are
  bit-identical to the no-fast-math build.

- Creator JSON load→save against the **official fast-math reference**:
  byte-identical for all 12 authored inputs. The inputs cover every
  component type, unsorted and duplicate keyframe positions, inherited
  line keyframes, defaults, required-field errors, the 0.3.2/0.3.6/0.3.8/
  0.7.6 upgrade chains and the line-shape path. `updateJson` output, the
  `init()` creator and `initPredefinedWaves()` (all six frames' base64
  `wave_data`) are also identical.
- Fast-math codegen mirrored when this landed. Both mirrors were removed
  in #8 after the owner chose source semantics with ULP-bounded JSON (see
  the rendering entry); kept here for the record:

  | Port location | Upstream expression | What GCC emits in the x86 reference | Why mirrored |
  |---|---|---|---|
  | `wave_frame.rs` `tanh_as_reference_loop` | `futils::tanh` in `PredefinedWaveFrames::createSaturatedSin` (`vital/src/synthesis/lookups/wave_frame.cpp:120`) | `x·((C·|x| + B)·x² + (|x| + 1)·A) / (|x·E·|x| + x|·(x² + D) + D)`, read from the disassembly | The frame is saved as base64 by `initPredefinedWaves()` (and Preset's 0.5.0 "Sub" upgrade). The source order differs in 372/2048 samples (≤ 1.8e-7) |
  | `group.rs` `load_default_group` | `float t = i / (kWaveformSize - 1.0f)` (`vital/src/common/wavetable/wavetable_group.cpp:95`) | `i * (1.0f / 2047.0f)` (`-freciprocal-math`) | The default saw is saved in every init wavetable's JSON. Division differs in 4 samples by 1 ulp |

  `sas_common::scalar::tanh` always kept the source order, since fast-math
  codegen differs per call site: the mirrored form still differs in
  71/18,580 inputs at a scalar call site.
- FFT against the no-fast-math reference: bit-identical (6 forward and 6
  inverse transforms, 2048 points). Against the fast-math reference it
  differs by ≤ 1.2e-7 relative (max 7.8e-3 abs on bins of magnitude
  6.3e4), from GCC reassociating the butterflies. None of five
  reassociated butterfly forms tried matched better than the source
  order.
- Double-precision transcendentals use the pure-Rust `libm` crate (MIT,
  `=0.2.16`, default features off) instead of glibc. glibc's double
  `exp`/`sin`/`cos` return different bits with and without FMA (see
  `decisions/reference-determinism.md`), which would make the port
  CPU-dependent. Measured against glibc on 6M inputs, `libm` differs in
  5% of results by 1 ulp as `f64`, but in none after the cast to `f32`
  that upstream applies. FFT twiddles are identical, and the FFT stays
  bit-identical to the no-fast-math reference. Float functions (`sinf`,
  `atan2f`, ...) stay on the system libm, because glibc's float functions
  do not depend on FMA.
- A missing `random_seed` in an Audio File Source is drawn from the
  global `RandomGenerator` counter, as upstream does. With the counter at
  the reference's value, the saved seed matches. Authored JSON should
  always set it.

**Open / next**

- Done since: rendering (#8), and reference captures through `sas-parity`
  with text artifacts (`sink.text`, PR #3).
- Spectra are captured as amplitude/1024 × phasor, inside the existing
  limits; see #8's capture entry.
- After ARM Reference's #2 lands: re-check the `libm` FFT twiddles (and
  so the FFT) against the arm64 reference. They are bit-identical to the
  x86 no-fast-math build.

- `sas-wavetable` forwards the `aarch64-semantics` feature to
  `sas-common`, for the RNG's libc++ column (A9). The Audio File Source's
  random phase generator uses that RNG. Every crate that uses the RNG
  forwards it the same way.

---

## 2026-10-20: M4a wavetable creator JSON parity captures

**Added**

- `ref-module` module `wavetable_creator_json`
  (`migration/reference/src/modules/wavetable/creator_json.cpp`):
  - `params.creator` (a JSON string) produces texts `saved.json`
    (`jsonToState` → `stateToJson().dump()`) and `upgraded.json`
    (`updateJson(input).dump()`), both upstream's own serialization.
  - `params.preset` init/predefined produces array `keyframe_waves` (the
    keyframes' time-domain samples, compared with tolerances).
- `wave_recipes.h` and `port/crates/wavetable/tests/support/mod.rs`
  expand `{"$wave": recipe}` objects (saw, square, triangle, mix; f32 or
  PCM) into base64 buffers. The two sides produce identical bits (integer
  phase arithmetic, single float operations, power-of-two slopes), so case
  files stay small.
- `migration/corpus/modules/wavetable/creator_json.json` has 14 cases:
  - every component type;
  - Audio File Source defaults;
  - an empty table;
  - a table with no version (all upgrades);
  - the 0.3.2, 0.3.6 (float audio → PCM), 0.3.8 (PCM wave data → float)
    and 0.7.6 (old line points) upgrade chains;
  - a line shape as a wavetable;
  - float positions and settings;
  - boolean numbers;
  - an Audio File Source with explicit styles;
  - `init` and predefined keyframes.

  Parity test: `port/crates/wavetable/tests/creator_json_parity.rs`.

**Measured against the official reference:** all 24 JSON artifacts are
byte-identical. `keyframe_waves` is exact for both presets; against the
no-fast-math build, predefined differs by ≤ 1.8e-7, within the default
limits.

**Owner decisions (2026-10-21)** settle the formerly pending cases:
- JSON is compared ULP-bounded rather than byte-exact: numbers within
  4 f32 ULP or 1e-6, `-0.0` equals `0.0`, and base64 f32 data decoded
  under the same bound.
- Ill-conditioned content is compared exactly against the no-fast-math
  reference.
- The port uses source semantics (no fast-math mirrors).

Each parked case is re-enabled once Harness Builder's support lands:

| Case | Where it waits | Re-enabled as |
|---|---|---|
| `wavetable/creator_json/preset_init_saved_json` | `modules/wavetable/pending/creator_json.json` | JSON-bounded artifact, `wave_data` decoded as f32 |
| `wavetable/creator_json/preset_predefined_saved_json` | same | same |
| `common/json/negative_zero` | `modules/common/pending/json.json` | JSON-bounded artifact |
| `common/line_generator_edit/*_saved_json` (3 cases) | `modules/common/pending/line_generator_edit.json` | JSON-bounded artifact |
| 5 ill-conditioned render cases | `modules/wavetable/pending/render.json` | `reference: nofastmath`, exact (see the rendering entry) |

- A missing `random_seed` is drawn from the global seed counter, so the
  0.3.6 case sets one explicitly. The counter behaviour stays covered by
  unit tests.

---

## 2026-10-21: `sas-common` scalar helpers cross-checked against `sas-simd`

**Changed**

- `port/crates/simd/tests/common_scalar.rs` (sas-simd gains a dev-dependency
  on sas-common) compares `sas_common::scalar` with sas-simd bit for bit.
  The inputs are about 480k floats: special values, a 1/128 grid over ±300,
  uniform values in ±40, and random bit patterns including NaN, infinities
  and subnormals. The comparison covers `exp2`, `log2`, `exp`, `log`,
  `pow` (also every lane of the poly versions), `power_scale` and `tanh`
  (the `mono_float` overloads), `poly_floor`, `round_to_int`,
  `pow2_to_float`, `min`, `max`, `clamp`, `iclamp`, `ilog2`, `interpolate`
  (also the poly version), `interpolate_f64`, the Catmull weights and
  interpolation, and `PI`.
- It runs on both columns. CI's `aarch64-semantics` job now also runs it.

**Result**

All 20 helpers that have a sas-simd counterpart are bit-identical. Only
`float_to_pcm` and `pcm_to_float` have none.

The "upstream differs" list is explicit in the test's docs, and each item
is pinned by an `upstream_differs_*` test:

- `tanh` and `power_scale`: the mono and poly overloads differ upstream.
  `tanh` evaluates in a different order; `power_scale`'s pass-through
  threshold is 0.01 for mono and 0.005 for poly.
- `min`/`max`: `fmin`/`fmax` against `poly_float::min`/`max` (A5).
- Rounding for NaN and `|x| >= 2^31`. sas-common implements only the SSE2
  column of `toInt` (A1), so under `aarch64-semantics` it differs from
  sas-simd's NEON column there. Examples: `poly_floor(NaN)` gives −2^31
  instead of 0, and `exp2(2^31)` gives +inf instead of 0.5. Every in-range
  value agrees.

**Open / next**

- Proposed consolidation, which needs sas-common's owner to agree. sas-simd
  would depend on sas-common and re-export its scalar helpers as
  `utils::{min, max, clamp, iclamp, ilog2, interpolate, interpolate_f64}`
  and `futils::{exp2, log2, exp, log, pow, power_scale, tanh}_mono`, with a
  single copy. Before that, sas-common's private `round_to_i32_sse` should
  follow A1 per target (sas-common already has `AARCH64_SEMANTICS`). The
  single copy would then match upstream's arm64 lane-0 result out of range
  too. Until then, this test keeps the two copies in step.

---

## 2026-10-21: scalar helpers consolidated into `sas-common`; `toInt` per target

Approved by the coordinator after the cross-check above. Core Framework
owns both sides.

**Changed**

- `sas-common` ports both columns of `utils::toInt` (reference-arm.md A1):
  - `scalar::to_int_sse2` (`_mm_cvtps_epi32`: round to nearest even;
    `i32::MIN` for NaN and out-of-range);
  - `scalar::to_int_neon` (`vcvtq_s32_f32`: truncate and saturate; 0 for
    NaN);
  - `scalar::to_int`, which picks the column from `AARCH64_SEMANTICS`.

  `poly_trunc`, `poly_floor` and `round_to_int` (and through them `exp2`,
  `exp`, `pow` and `power_scale`) now match upstream on both targets for
  NaN and `|x| >= 2^31` too, e.g. `poly_floor(NaN)` is 0 and `exp2(2^31)`
  is 0.5 on arm64. In-range results are unchanged. Unit tests cover NaN,
  ±inf, ±2^31 and ±3e9 on each column, the selected column through the
  public helpers, and in-range agreement of the columns. `sas-common`
  stays dependency-free.
- `sas-simd` depends on `sas-common`, and its `aarch64-semantics` feature
  forwards to `sas-common/aarch64-semantics`. The duplicated scalar helpers
  are now re-exports:
  - `utils::{min, max, clamp, iclamp, ilog2, interpolate, interpolate_f64}`;
  - `futils::{exp2, log2, exp, log, pow, power_scale, tanh}_mono`;
  - `common::PI`.

  The poly versions stay in `sas-simd`, and the public paths are
  unchanged.
- `tests/common_scalar.rs` now checks `sas-common`'s helpers against every
  lane of the poly versions on about 480k inputs, with no range filter
  (NaN, infinities and huge values included), on both columns. The helpers
  are `to_int`, `trunc`, `floor`, `round_to_int`, `pow2_to_float`, `exp2`,
  `log2`, `exp`, `log`, `pow`, `interpolate` and the Catmull weights and
  interpolation. The upstream-differs pairs stay pinned: `tanh` and
  `power_scale` mono vs poly, and `fmin`/`fmax` vs `poly_float::min`/`max`.
  A new test checks that both crates select the same column.
- Parity Reviewer's non-blocking notes on #14:
  1. Mono `clamp` (`fmin(max, fmax(value, min))`) vs poly `clamp`
     (`max(min(value, max), min)`) is added to the differs list. It is
     pinned for inverted bounds and NaN, and checked to agree, up to the
     sign of zero, for ordered bounds.
  2. The `±0` tie of poly `min`/`max` is pinned on both columns. On SSE2 it
     returns the second operand; on NEON, +0 for max and −0 for min. The
     mono side isn't pinned, because neither C's `fmax` nor Rust's
     `f32::max` specifies the sign of a zero tie.
  3. Callers of `sas-common`'s rounding and conversion paths, and their
     bounds:
     - `power_scale`, through `exp`'s `round_to_int`, is called by
       `LineGenerator` (`line_generator.rs:162`, `:301`) and the wavetable
       Line Source keyframe (`creator/keyframes.rs:348`). `t` is in
       [0, 1], but the power comes from preset JSON and is not bounded by
       the code. It only leaves the range where the columns agree for NaN
       or |power| > ~1.5e9, and with the per-target `to_int` those inputs
       now follow upstream on each target too.
     - `float_to_pcm` (`load_save.rs:67`, `creator/component.rs:504`)
       clamps to ±32767 before its `as i32` cast (A2/A3). NaN clamps to
       −32767, so the cast is always in range.
     - `LineGenerator::value_at_phase` (`line_generator.rs:268`) clamps
       the phase to [0, 1] before its `as usize` index (NaN gives 0).
     - `interpolate`, `pcm_to_float` and the Catmull helpers don't round.

     What remains is outside A1: the sign of zero when `fmin`/`fmax`
     (`min`, `max`, `clamp`) tie at ±0. C leaves it unspecified, Rust's
     `f32::min`/`max` documents it as non-deterministic, and the reference
     is built with `-fno-signed-zeros`. No caller depends on it.

**Parity**

No reference output changes on x86_64: the SSE2 column is what both
crates already did. The `simd/poly_function` mono cases now run through
`sas-common`, with unchanged metrics. `mono_exp`, `mono_exp2`, `mono_log`
and `mono_log2` are bit-identical. `mono_power_scale` (1.0e-6) and
`mono_tanh` (1.8e-7) differ from the fast-math reference by the same
amounts as before.

---

## 2026-10-21: M5a PR 1, `SynthFilter` interface and `DigitalSvf`

Core Filters (M5a), first PR of the `sas-filters` stack.

- `synth_filter.rs` ports `synth_filter.{h,cpp}`: input indices, styles,
  `FilterModel`, `FilterState` with `load_settings`, the static
  `CoefficientLookup` (`OneDimLookup<computeOnePoleFilterCoefficient, 2048>`),
  the `SynthFilter` trait (`setup_filter(&mut self, this, &FilterState)`) and
  `create_filter`. The comb/formant/phaser arms go to
  `character::create_filter`, owned by Character Filters (M5b), whose models
  live under `src/character/`.
- `digital_svf.rs` ports `digital_svf.{h,cpp}`: all styles (12 dB, 24 dB,
  notch/pass swap, dual notch/band, band/peak/notch, shelving), basic mode,
  drive compensation, resonance bounds and the SVF coefficient lookup.
  `processBasic24` has no upstream caller; `tickBasic24` is kept.
- `aarch64-semantics` forwards to `sas-simd`. The only per-ISA site reached
  is `toInt` in `cubicLookup` (A1c) through `sas-dsp-core`.
- New ref-module `digital_svf` (`setBasic`, `setDriveCompensation`,
  `setResonanceBounds` as the equalizer and formant manager use them).
- Cases, all under `module_defaults` (no tolerance changes):
  `filters/digital_svf` grows from 1 to 32 cases: every style with cutoff
  sweeps over MIDI 0–140 (past the 8–136 parameter range), blend −0.2–2.2,
  resonance −0.1–1.1 and drive −4–40 dB (past both clamps), plus
  shelving gain −20–20 dB, a cutoff that dips below 0 (the `max(…, 0)` in
  `process12`), per-lane settings with voice-1 and voice-0 retriggers,
  2× and 4× oversampling, 96 kHz and block sizes 1/64/128/256/512.
  `filters/digital_svf_settings` (8 cases) covers the equalizer's basic
  high-/low-pass, notch and shelves, basic 24 dB, no drive compensation and
  the formant bounds (4–30).
- Parity (`tests/digital_svf.rs`, shared driver in `tests/support/`), x86_64:
  40/40 pass. Worst max_abs_error 5.5e-5 (`digital_svf_settings/eq_highpass`,
  1.8× under), next 2.7e-5 (`formant_bounds`) and 1.7e-5
  (`band_peak_notch_cutoff_sweep`); worst residual −100.2 dB
  (`eq_highpass`). `shelving_drive_sweep` is bit-identical.
- Hashes: x86 identity from this PR's CI `reference-renders`; macOS entries
  follow once PR #2 merges.

Open: macOS arm64 hashes after PR #2.

## 2026-10-21: M5a PR 1 follow-up, `create_filter(Comb)` returns `None`

Upstream `SynthFilter::createFilter(kComb)` builds `CombFilter(1)`, whose
processing reads past its 2-float memory (undefined behaviour; found by
Character Filters). There is no defined output to reproduce, so the port's
`create_filter(FilterModel::Comb)` returns `None` instead of building a
filter that could panic or diverge in `process`. Nothing in the engine uses
that path: `FilterModule` builds its comb through `CombModule`, with full
memory. No `filter` module case uses `model: comb`.

---

## 2026-10-20: M4a wavetable rendering and `Wavetable` lookup data

**Added** (`sas-wavetable`)

- `wavetable`: `Wavetable`/`WavetableData` with upstream's per-frame
  layout. That covers `wave_data` and the `poly_float`-layout
  `frequency_amplitudes`, `normalized_frequencies` and `phases` (2056
  floats per frame, harmonic `h` at `[2h]`/`[2h+1]`), as well as
  `setNumFrames` (copying the last frame, version bumps), `loadWaveFrame`,
  `postProcess` and `getFrequencyBin`/`getFrequencyFloatBin`. This is the
  structure the Oscillators thread builds on. The GUI/audio double buffer
  is left to the engine.
- `creator::render`: `render` for every source and modifier, the group and
  creator render loops (persistent compute frames, so the last source's
  frequency ratio and sample rate carry over as upstream), `postRender`,
  `renderToBuffer`, `PitchDetector`, `detectWaveEditTable` and the
  `initFromAudioFile` styles (splice, vocoded, TTWT, pitched).
  - Mixed float/double arithmetic follows the upstream types.
  - `FileSourceKeyframe::renderFreqInterpolate`'s aliased in-place
    interpolation is reproduced.
  - The wave warp's unqualified `exp(float)` resolves to the double `exp`
    in the reference binary (checked in the disassembly), so the port uses
    the double version too.
  - No new fast-math mirroring.

**Measured** (scratch probe; 45 authored creator JSONs plus 27
well-conditioned variants, 257 frames each). Details and the capture
proposal are in the Project store: `internal/wavetable-render-parity-measurements.md`.

- Against `REF_FAST_MATH=OFF`: 41/45 cases bit-identical in every array.
  The rest come from complex division (no-fast-math uses `__divsc3`) and
  the two recorded mirrors.
- Against the official reference, on well-conditioned content:
  time-domain frames ≤ 2.1e-5 (typically ≤ 1e-6) and ≤ −114 dB. Harmonic
  coefficients (`amp/1024` × unit phasor) ≤ 9e-7 and ≤ −114 dB. Both are
  inside the module limits. Raw amplitudes (up to about 1000) exceed
  1e-4 by 1 ulp, and raw phasors flip on noise-level bins, so the
  captures should not compare raw arrays.
- **Ill-conditioned content fails against the official reference even
  though the port equals the no-fast-math build bit for bit:**
  - frequency-mode keyframe interpolation between disjoint spectra
    (saw→sine): −35 dB;
  - multi-group containing it: −27 dB;
  - frequency-interpolated audio fades: −62 dB;
  - folding at ±1: 2.6e-3.

  Upstream takes interpolated phases from `arg(conj(from)·to)` of
  FFT-noise bins, so the result depends on the FFT's rounding. These
  cases need an owner decision; tolerances are untouched.

**Open / next**

- Done: committed captures (next subsection) and the owner's decisions
  (below).

### Source semantics, owner decisions and the remaining cases (2026-10-21)

- **No fast-math mirrors left; the port follows the source:**
  - `PredefinedWaveFrames::createSaturatedSin` uses `futils::tanh` in
    source order;
  - the default saw divides by 2047 (`wavetable_group.cpp:95`);
  - the JSON dump tests `signbit`;
  - `std::complex<float>` division is libgcc's `__divsc3`. With GCC 13
    and double hardware, that is the textbook formula in double, rounded
    to float, plus libgcc's infinity recovery. Checked against GCC on 2M
    random divisions, 0 mismatches; unit tests pin 4 cases and 1/0.
- **Result:** every array and text artifact of `sas-common` and
  `sas-wavetable` is bit-identical to the no-fast-math reference. That
  covers all live cases, the 5 ill-conditioned cases (including
  `phase_coefficients` through glibc `cosf`/`sinf`) and the parked JSON
  cases.
- **Against the official reference**, the live cases are unchanged:
  - render: `wave` ≤ 2.8e-5, `coefficients` ≤ −105.8 dB,
    `phase_coefficients` ≤ 7.7e-7;
  - all 24 creator JSON artifacts are byte-identical;
  - `keyframe_waves` is now 1.2e-7 (init) and 1.8e-7 (predefined) instead
    of exact.
- **Audio fade 3** (frequency interpolation) with cleared or vocoded
  phases is well-conditioned (≤ 2.7e-7, −134 dB), so
  `audio_fade_3_phase_1` and `audio_fade_3_phase_2` are ordinary cases.
  `render.json` now has 46.
- **The 5 ill-conditioned cases** for `reference: nofastmath` are authored
  in `modules/wavetable/pending/render.json` until the harness supports
  the field:
  - `wave_linear_freq_disjoint_spectra`, `wave_cubic_freq_disjoint_spectra`
    and `multi_group_disjoint_spectra`: saw → sine → triangle → saw. The
    sine is a literal float32 base64 keyframe, because the recipe shapes'
    exactly-zero bins are computed identically by both FFT builds and are
    not ill-conditioned (≈ −120 dB). Measured −36 to −39 dB against the
    official build.
  - `wave_folder_at_clamp`: folding after a frequency-interpolated source
    that peaks at ±1; 2.8e-3 / −98 dB.
  - `audio_fade_3_phase_0`: 5.1e-4 / −73 dB.

### Rendering parity captures

- New `ref-module` module `wavetable_render`
  (`migration/reference/src/modules/wavetable/render.cpp`) emits four
  arrays:
  - `wave`: time-domain frames.
  - `coefficients`: amplitude/1024 × normalized phasor, what the
    oscillator reads.
  - `phase_coefficients`: amplitude/1024 × (cos, sin) of the stored phase.
  - `info`: frame count, frequency ratio, sample rate and the shepard
    flag.
- Complex pairs are interleaved in one channel. `audio-diff` takes the
  worst per-channel residual, and with cleared phases the imaginary
  channel is pure rounding noise; for example,
  `audio_fade_0_phase_1` measured max error 7e-8 but −0.7 dB residual on
  that channel.
- Raw amplitudes (up to ~1e3) and the phasors of silent harmonics are not
  compared directly.
- `info` holds the frame count / 257 and the sample rate / 44100 rather
  than the absolute values. Large-scale values are captured normalized,
  and the 1e-4 absolute gate is below one float ulp at 44100.
- `migration/corpus/modules/wavetable/render.json` has 44 well-conditioned
  cases:
  - every wave-source interpolation (frequency interpolation between saws
    that share harmonic support);
  - every phase, window, filter (with and without normalize) and warp
    variant;
  - slew limiter; wave folder with headroom;
  - audio fades 0 to 2 × all phase styles;
  - shepard with time and frequency interpolation;
  - line source; multi-group without normalization;
  - init and predefined.

  Parity test: `port/crates/wavetable/tests/render_parity.rs`.
- **Measured against the official reference, all within the default
  module limits (no new tolerances):**

  | Array | Worst max error | Worst residual |
  |---|---|---|
  | `wave` | 2.8e-5 (`audio_fade_1_phase_0`) | −123.4 dB |
  | `coefficients` | 2.8e-5 | −105.8 dB (`slew_limiter`) |
  | `phase_coefficients` | 7.7e-7 | −123.2 dB |
  | `info` | exact | exact |

- The ill-conditioned cases (frequency interpolation between disjoint
  spectra, folding at ±1, audio fade 3) stay pending; see "Pending owner
  decision: byte-exact vs bounded JSON" in the creator JSON entry.

---

## 2026-10-21: `RouterHooks` forwards idle adds and sets the sample rate (Voice Engine request)

**Changed**

- `port/crates/dsp-core/src/router.rs`: when `RouterHooks::forward_add_remove`
  is set (only `VoiceHandler` uses it), the router now does what upstream's
  `VoiceHandler` overrides do (`voice_handler.cpp:788-800`):
  - `add_idle_processor` is forwarded as well as `add_processor` and
    `remove_processor`;
  - `add_processor` and `add_idle_processor` first call
    `processor.set_sample_rate(self.sample_rate())`.

  This fixes `SynthModule::create_poly_mod_control` on a voice handler: its
  `add_idle_processor(&control_switch)` now lands in the voice router, so
  the switch is polyphonic.
- `tests/graph.rs`: `voice_handler_hooks_forward_to_the_voice_router`
  mirrors the overrides on a `SynthModule` handler. It checks the owner,
  sample rate and `isPolyphonic` of a forwarded add, a forwarded idle add
  and a `create_poly_mod_control` switch, then the removal, and that a
  router without the hook keeps its idle processors. The test fails on
  the previous code.

**Reference capture: none.** No single-module capture can tell the
difference. `ref-module` calls `setSampleRate`/`setOversampleAmount` after
building the module, and `VoiceHandler` propagates both to the voice
router, so the add-time sample rate is overwritten before the first block.
Where an idle processor lives changes only `isPolyphonic` and ownership,
which no output exposes. Voice Engine's `SynthVoiceHandler` captures will
cover it end to end.

---

## 2026-10-21: M5b PR 1, `PhaserFilter` (character filters)

Ports `vital/src/synthesis/filters/phaser_filter.{h,cpp}` to
`port/crates/filters/src/character/phaser_filter.rs`, stacked on the M5a
interface branch (`synth_filter.rs`: `FilterState`, `coefficient_lookup`,
`SynthFilter`). `character::create_filter(Phase)` returns
`PhaserFilter(false)`, as `SynthFilter::createFilter` does. The kernel is
also constructible on its own (`PhaserFilter::new(clean)`) for the phaser
effect, which drives it through `process_with_input`.

- Structure as upstream: the `saturateResonance` / `saturateInput` template
  pair is a function-pointer pair (clean: `futils::tanh` on the resonance
  path; dirty: `futils::hardTanh` on the all-pass input); per-block linear
  ramps of resonance, drive and the three peak amounts; reset masking;
  `hardReset` zeroes the ramp targets.
- Per-ISA sites: only the coefficient lookup's `toInt` index (A1c), inside
  `sas-simd`. No delay read in this filter. No FMA, no estimates.
- `sas-filters` forwards `aarch64-semantics` to `sas-simd` (in the M5a
  Cargo.toml).

Reference: new ref-module `phaser_filter` (`params.clean`), file
`migration/reference/src/modules/filters/phaser_filter.cpp`. Upstream's
`phaser_filter.h` uses `futils` without including it (the unity build
includes it earlier), so the harness file includes `futils.h` first.

Cases (`migration/corpus/modules/filters/phaser_filter.json`, 9 cases, all
under `module_defaults`): both `clean` settings; cutoff sweeps over MIDI
−40..184 (filter cutoff 8..136 plus modulation, and the phaser effect's
center 8..136 ± mod depth 48; the filter clamps the frequency ratio at 1.0);
resonance and blend ramps over 0..1 and 0..2 and beyond (−0.5..1.5,
−1..3, to hit the clamps); drive 0..20 dB and −10..40 dB (clamped to 0..36
by `loadSettings`); style 0, 1 and 9 (inversion); per-lane controls with
per-voice resets; 96 kHz / block 128 and oversample 2. Outputs are audio,
so no normalization is involved.

| Metric (9 cases) | Port vs reference | No-fast-math proxy vs reference |
|---|---|---|
| worst `max_abs_error` | 7.2e-6 (`dirty_out_of_range_controls`) | 7.2e-6 (same case) |
| worst `residual_db` | −115.9 dB (`dirty_inverted_blend_resonance_ramps`) | −115.9 dB (same case) |
| best `residual_db` | −129.6 dB (`clean_oversample2`) | |

Margin ≥ 14× on the absolute gate and ≥ 35.9 dB on the residual gate. No
case is bit-identical to the default (fast-math) reference, but the port is
bit-identical to the `REF_FAST_MATH=OFF` build on all 9 cases. Tests use
the shared filter harness (`tests/support`, `check_file`). Hashes:
x86_64 CI identity (local tree with the identical identity, re-checked
against the PR's CI `reference-renders` artifact).

**Open / next**

- macOS arm64 hashes for `phaser_filter` once PR #2 merges.
- M5b PR 2 (comb filter and comb module), PR 3 (formant filter, formant
  manager, vocal tract, formant module; needs M5a DigitalSvf).

---

## 2026-10-21: owner approval — JSON bound and no-fast-math exact cases

The owner (Salil) approved:

1. "ULP-bounded, not byte-exact: preset and wavetable JSON values must agree
   within a few ULP of float rounding";
2. exact comparison against the no-fast-math build for frequency-mode
   interpolation between keyframes with disjoint spectra, folding at the
   clamp, and frequency-interpolated audio fades.

The specific numbers (4 f32 ULP or 1e-6 absolute, whichever is looser) are
the coordinator's interpretation, pending owner confirmation. The 5 ids
below are the measured instances of the categories in (2). The decisions are
recorded in `migration/decisions/owner-decisions-json-and-morphs.md` (same
wording) and enforced identically by `run_corpus.py` and sas-parity.

- **JSON state is bounded, not byte-exact.**
  - Text artifacts a case declares `"artifacts": {"<name>": "json"}` must
    match structurally: keys (order ignored), types (integer ≠ float),
    strings, integers and array lengths.
  - Floats must agree within 4 f32 ulp or 1e-6 absolute, whichever is
    looser (the coordinator's interpretation of "a few ULP", pending owner
    confirmation); -0.0 equals 0.0. The floor is needed because
    source-formula values near zero differ by up to 96 ulp but only 7.8e-7
    absolute.
  - Below |x| = 4 the floor is the looser term. Samples in [-1, 1] are
    therefore effectively held to 1e-6 absolute, and only magnitudes ≥ 4
    are held to 4 ulp.
  - Keys listed in `base64_f32_keys` are decoded as little-endian f32
    arrays: same length, each element under the same bound. Only the
    pinned `BASE64_F32_KEYS` = `{"wave_data"}` may be declared; anything
    else is rejected at load. Other strings stay exact, including PCM
    `audio_file`.
  - Undeclared artifacts stay byte-exact. The bound is pinned in both
    resolvers (`JSON_BOUND`); `"json_bound"` may only tighten it.
  - It applies in sas-parity (`assert_artifact!` reads the mode from the
    case) and in every cross-build `run_corpus.py compare`, including the
    calibration proxy compare. Determinism (`--exact`) and hashes stay
    byte-exact.
  - This unblocks Preset Registry #7 (22 preset/state artifacts, where the
    proxy's default-wavetable `wave_data` differs by 1 ulp in 4 samples)
    once its case file declares the artifacts. It also unblocks the
    Shared crate's pending `-0.0` `saved.json` cases.
- **Ill-conditioned content is exact against the no-fast-math build.**
  `"reference": "nofastmath"` (case-level only) compares every array and
  artifact bit for bit against the `REF_FAST_MATH=OFF` captures. A case
  qualifies only if:
  - its id is in the decision doc's allowlist (one line per id; the
    resolvers read it from there); and
  - CI's calibration step shows the official reference failing the case's
    normal limits against the no-fast-math build. Normal limits are the
    module defaults plus the declared artifact modes. Run as
    `compare build/ref build/proxy --justify-nofastmath`, this is the
    general rule proposed to the owner (decision pending); both gates are
    on.
  - The case cannot set tolerances or a `tolerance_exception`.
  - Captures: the calibration job uploads `parity-captures-nofastmath`, and
    the parity job reads it through `SAS_PARITY_CAPTURES_NOFASTMATH`.
  - Hashes are pinned per no-fast-math identity, for listed cases only
    (`hashes --reference nofastmath`). That identity is known only as a CI
    identity with just the fast-math setting changed.
  - Allowlist: 5 wavetable ids from Wavetable Creator (corrected from 7;
    `audio_fade_3_phase_1/_2` are well-conditioned after its source-formula
    fixes):
    - wavetable/render/wave_linear_freq_disjoint_spectra
    - wavetable/render/wave_cubic_freq_disjoint_spectra
    - wavetable/render/multi_group_disjoint_spectra
    - wavetable/render/wave_folder_at_clamp
    - wavetable/render/audio_fade_3_phase_0
  - Comb filter, the 120 Hz crossover at 2× and above, oscillator pitch
    sweeps and reverb tails are pending the owner.
- **macOS arm64.** Probe CI run 39521775311 (throwaway branch
  `cursor/macos-nofastmath-probe-f875`, which combines PR #2 and PR #8)
  rendered 633 module outputs with no-fast-math builds:
  - macOS 15: repeat renders are bit-identical.
  - Same binary on macOS 14: identical except `poly_pow` (Apple `powf`,
    1 ulp in 110 values); every allowlisted case is identical.
  - Linux x86_64 vs macOS arm64: 180 outputs differ, including every array
    of the five allowlisted cases (≤ 1.9e-6). So the Linux no-fast-math
    captures cannot be the exact target on macOS. A macOS no-fast-math
    build can be, pinned with its own identity, which includes the macOS
    version.
  - Not yet shown: the arm64 port matching it bit for bit. The port of these
    cases is not on main, and the difference is not attributed yet. Nothing
    is loosened.
  - Found on the way: `line_generator.cpp` and `line_generator_edit.cpp`
    ODR-use `LineGenerator::kDefaultResolution` in `params.value(...)`.
    Apple clang fails to link that (no out-of-line definition); GCC builds
    it. The probe used `static_cast<int>(...)`. The fix belongs with PR #2
    or the Shared crate's harness files.
- **Harness self-test:** `harness/echo/json_state` declares its
  `params.json` as JSON, with base64-f32 `wave_data` and PCM `audio_file`.
  sas-parity checks it against real captures: reformatting and -0.0 pass, a
  1-ulp drift in every base64 float passes, 5 ulp at 6.7 fails, 4 vs 5 ulp
  at 1000.25, a changed PCM byte fails, and 3 vs 3.0 fails. Hashes were
  written from a local tree with the CI build identity.
  - There is no nofastmath self-test case: a harness case would be
    identical in both builds, and the calibration gate would rightly reject
    it. The first end-to-end use is Wavetable Creator's allowlisted cases.
- **Tests**, in run_corpus.py (`test_run_corpus.py`) and in sas-parity:
  - loosening the bound (case or file level) is rejected;
  - `reference: nofastmath` on an unlisted case is rejected, and so are
    tolerances on a listed one;
  - a 5-ulp drift fails while 4 passes (|x| ≥ 4);
  - -0.0 vs 0.0 passes;
  - base64 f32: 1 ulp passes, 5 ulp fails, a length change fails;
  - a changed PCM byte fails;
  - #7's calibration case (nested preset `wave_data`, 4 samples 1 ulp off)
    fails undeclared, passes declared, and stays byte-exact under `--exact`;
  - the justification gate passes and rejects as specified;
  - no-fast-math hashes use their own identity;
  - the bit-exact comparison flags -0.0 vs 0.0 and a 1-ulp change.
- **Parity Reviewer changes on #34:**
  - `base64_f32_keys` is pinned to `{"wave_data"}` in both resolvers. The
    reviewer showed that declaring `audio_file` let int16 PCM `[5, 0,
    1000, 0]` vs `[30000, 0, -1000, 0]` pass, as denormals within 1e-6.
  - The no-fast-math identity must be the exact counterpart of a CI
    identity: `opt_flags` equal to the CI flags minus
    ` -ffast-math -U__FAST_MATH__`, instead of ignoring the flags.
  - A nofastmath case always resolves to the pinned `JSON_BOUND` and cannot
    set `json_bound`, so tightening cannot game the justification.
  - Optional item, done: `run_corpus.py allowlist-approval --base <rev>`
    runs in CI's repo checks. It fails if an id is added to the allowlist
    without a new LEDGER.md section whose heading contains "owner approval"
    and which names the id. This entry approves the 5 initial ids.
  - Each fix has tests in both suites (the guard is Python-only, since it
    runs in CI).

---

## 2026-10-21: `aarch64-semantics` forwarding: dsp-core feature, compile-time guard

Graph Reviewer's follow-up on #22. sas-simd's feature forwards to
sas-common, but not the other way round. A crate that forwards only
`sas-common/aarch64-semantics` (today `sas-wavetable`) could therefore put
sas-common on the NEON column while sas-simd stays on SSE2 in any build that
also contains sas-simd. The mono overloads (sas-common's) and the poly ones
would then use different columns.

**Changed**

- `sas-dsp-core` gains `aarch64-semantics = ["sas-simd/aarch64-semantics"]`,
  which reaches sas-common through sas-simd.
- `sas-simd` refuses to compile when its column differs from sas-common's.
  A const assertion on `AARCH64_SEMANTICS == sas_common::AARCH64_SEMANTICS`
  names the fix in its message. Real aarch64 builds are unaffected, because
  both crates report true there.
- CI's aarch64-semantics job now runs dsp-core's clippy, lib and graph
  tests under the feature. It also checks that
  `cargo check -p sas-simd --features sas-common/aarch64-semantics` fails
  with the guard's message.

**Rule for crates with the feature:** forward `aarch64-semantics` to
`sas-dsp-core/aarch64-semantics`, or to `sas-simd/aarch64-semantics` if you
don't depend on dsp-core. Also forward to every other direct dependency
that has the feature (e.g. `sas-wavetable`). Never forward to `sas-common`
alone if sas-simd is anywhere in your dependency graph.

---

## 2026-10-21: M5b PR 2, `CombFilter` and `CombModule`; 7 cases parked pending the owner

Ports `vital/src/synthesis/filters/comb_filter.{h,cpp}` and
`modules/comb_module.{h,cpp}` to `port/crates/filters/src/character/`
(`comb_filter.rs`, `comb_module.rs`), stacked on M5b PR 1.

- Structure as upstream: the tick template parameter (comb, positive
  flange, negative flange) is a function pointer. The copy constructor's
  partial reset is mirrored by a manual `Clone`: filter state and feedback
  style copied, zeroed memory from `Memory`'s copy constructor, the
  settings it assigns back to their initial values, and `feedback_filter_`
  reset. The members it does not mention are default-initialized, not
  copied: `low_gain_` and `high_gain_` (0) and `feedback_filter2_` (a reset
  `OnePoleFilter`). This was corrected in review (Parity Reviewer, #19). A negative style leaves `process` writing
  nothing, as upstream's `VITAL_ASSERT(false)` path does in release.
- Per-ISA sites: the delay read `Memory::get` (A1a) inside `sas-dsp-core`.
- Upstream's `createFilter(kComb)` builds `CombFilter(1)`. Its memory
  rounds to one sample, and `Memory::get` then reads past the 2-float
  buffer (undefined behaviour); `CombFilter::create(1)` panics on the same
  read. `synth_filter::create_filter(Comb)` returns `None` (M5a follow-up),
  and nothing in upstream's engine calls `createFilter`, so it is not
  captured. The `comb_filter` ref-module refuses `size < 2`.

Reference: ref-modules `comb_filter` (`params.size`, default
`CombModule::kMaxFeedbackSamples` = 25000, rounded up to 32768) and
`comb_module` (the factory calls `init()`).

Cases (19 authored, all under `module_defaults`; 12 active in
`filters/comb_filter` (10) and `filters/comb_module` (2), 7 parked, see below;
3 of the active ones are interim, see "Interim cases"). They cover:

- all six styles, plus 6..9 (`style / 3 >= 2` behaves as low/high blend);
- MIDI cutoff −40..150 (filter cutoff 8..136 plus modulation; below about
  −31 the period hits the 32763-sample memory clamp at 44.1 kHz);
- blend transpose −48..132 (parameter range 0..84), blend −1..3, resonance
  −0.5..1.5;
- per-lane controls with per-voice resets, an impulse at full resonance,
  96 kHz at block 128, and oversample 2.

**Result: the port is bit-identical to the `REF_FAST_MATH=OFF` reference
on all 16 cases**, as it is on the 9 phaser cases. Against the default
fast-math reference, 9 cases pass and 7 fail. The proxy (CI
`tolerance-calibration`) fails the same 7 with exactly the same numbers:

| Case | max_abs_error | residual_db |
|---|---|---|
| `filters/comb_filter/negative_flange_sweep` | 8.0e-4 | **−76.8** |
| `filters/comb_module/comb_style_sweep` | 2.8e-4 | −92.4 |
| `filters/comb_filter/comb_cutoff_sweep` | 2.2e-4 | −95.6 |
| `filters/comb_filter/band_spread_negative_flange` | 1.6e-4 | −84.1 |
| `filters/comb_filter/comb_resonance_blend_ramps` | 1.3e-4 | −92.1 |
| `filters/comb_module/band_spread_full_range` | 1.2e-4 | −93.7 |
| `filters/comb_filter/comb_oversample2` | 1.2e-4 | −88.2 |

Active cases, port vs fast-math reference: worst `max_abs_error` 9.8e-5
(`comb_96k_block128`, a cutoff sweep down to −30 at 96 kHz; it passes with
only 2 % margin by the same mechanism), worst `residual_db` −94.2 dB
(`out_of_range_controls`), best 3.0e-8 / −133.5 dB (`impulse_ringing`).
**At risk on the arm64 reference:** `comb_96k_block128` may miss against
the macOS arm64 reference, whose fast-math rounding differs. If it does, it
is parked under `modules/filters/pending/` like the 7 above (same rule, same
owner decision); its tolerances are not loosened.

Mechanism: every failing case sweeps or modulates the cutoff into long
delay periods. The per-sample period `sample_rate / (base_frequency *
midiOffsetToRatio(offset))` is a large-scale value of up to about 32k
samples. The fast-math reference rounds it differently (reassociation and
reciprocal-math in `futils::exp2` and the division). An ULP-level period
difference of about 2e-3 samples at 30k shifts the Catmull-Rom read by
that amount. The output error is that shift times the signal's slope,
re-circulated by up to unity feedback, so noise and saw inputs show it most.
The captured quantity is audio, so there is no normalized form to capture
instead. Narrowing the cutoff range, the resonance or the input content to
pass would be narrowing a test (rule 6).

**Parked, pending the owner (escalated 2026-10-21).** No tolerance exception
is added. The 7 cases above sit unchanged, inputs kept, in
`migration/corpus/modules/filters/pending/comb_filter.json` (5) and
`pending/comb_module.json` (2). Both loaders read only
`modules/<area>/*.json`, so they are not rendered or tested until they move
back. The recommendation under review is the `reference: nofastmath`
exact-match allowlist, not looser tolerances. An earlier draft of this entry
proposed per-case tolerance exceptions; that proposal is superseded by the
escalation and withdrawn.

**Interim cases (Parity Reviewer, #19).** While the 7 are parked, style 5
(band spread with negative flange), oversampling and the module's style
routing had no active coverage. Three additional well-conditioned cases
were added, each labelled interim in its `description`. They keep the
cutoff at 70..130 (or 100 ± 24), so the delay period stays short. The parked
full-range cases are unchanged; nothing is narrowed or replaced.

| Case | max_abs_error | residual_db |
|---|---|---|
| `filters/comb_filter/interim_band_spread_negative_flange` (style 5) | 6.4e-6 | −118.6 |
| `filters/comb_filter/interim_oversample2` (oversample 2, styles 0..5) | 1.5e-5 | −100.8 |
| `filters/comb_module/interim_style_sweep` (styles 0..5) | 1.8e-5 | −107.5 |

All three also pass the no-fast-math proxy.

**Open / next**

- The owner's decision on the 7 parked cases. Then move them back and apply
  the chosen mechanism.
- macOS arm64 hashes after PR #2.

## 2026-10-21: M5b PR 3, formant family (`FormantFilter`, `FormantManager`, `VocalTract`, `FormantModule`)

Ports `vital/src/synthesis/filters/{formant_filter,formant_manager,vocal_tract}.{h,cpp}`
and `modules/formant_module.{h,cpp}` to `port/crates/filters/src/character/`,
stacked on M5b PR 2. The stages are M5a's `DigitalSvf` at resonance bounds
4..30.

- `FormantManager`: a router of four `DigitalSvf` stages summed by
  `VariableAdd`; resets go through `getLocalProcessor`.
- `FormantFilter`: the router graph from `init` (per formant:
  `cr::BilinearInterpolate` for gain and Q, `BilinearInterpolate` for the
  MIDI cutoff, `Interpolate` toward the 80-semitone center by `spread`,
  `Add` with transpose, `cr::Multiply` with resonance). `setupFilter` is
  ported too, although nothing calls it while processing. Upstream plugs
  every filter into shared static `cr::Value`s (the vowel tables, `k12Db`,
  `kValueOne`). Here each filter owns control-rate `Value` nodes with the
  same constants. They are sources only and never processed, so the
  outputs are identical.
- `VocalTract`: upstream's processing is empty, so the module's shared
  output keeps the last block written by the previously active style. The
  port reproduces that, and `style_switching_with_resets` covers it.
- `FormantModule`: style switching (clamped 0..2), its own
  `<prefix>_formant_*` mod controls, per-voice resets of the active style.
- `character::create_filter(Formant)` is `FormantFilter(0)`. Like
  upstream's, it is a router that needs `init()`.

Reference: ref-modules `formant_filter` (`params.style`; the factory calls
`init()`; x, y and transpose are audio rate, as the module feeds them) and
`formant_module` (`FormantModule("filter_1")`, initialised, with
`params.controls` applied through `getControls()`). The Rust test installs
the five `filter_1_formant_*` parameter details (linear; min, max and
default from `filter_parameter_list`) with `set_parameter_lookup`, because
`sas-preset` has not merged. Switch to the registry when it does.

Cases: `filters/formant_filter` (9) and `filters/formant_module` (4), all
under `module_defaults`. They cover:

- both vowel styles, with x/y sweeps, corners and out-of-range values
  (−1.5..2.5; the audio-rate interpolation clamps to −1..2 and the
  control-rate one does not);
- transpose over ±12 and ±36, spread over ±1 and ±1.5, resonance over
  0.3..1 and 0..1.2;
- per-lane values with per-voice resets, 96 kHz at block 128, and
  oversample 2;
- for the module: defaults, control extremes, and a style ramp from −1 to
  3.5 crossing into the vocal tract.

| Metric (13 cases) | Port vs reference |
|---|---|
| worst `max_abs_error` | 2.9e-5 (`aoie_out_of_range`) |
| worst `residual_db` | −102.5 dB (`aiuo_per_lane_and_resets`) |

Margin ≥ 3.4× and ≥ 22.5 dB. The port is bit-identical to the
`REF_FAST_MATH=OFF` reference on all 13 cases.

**Open / next**

- Switch the test's parameter details to `sas-preset` once it merges.
- macOS arm64 hashes after PR #2.

## 2026-10-21: M5a PR 2, `LinkwitzRileyFilter` and `DcFilter`

Core Filters (M5a), stacked on PR 1. Effects needs `LinkwitzRileyFilter`
for the multiband compressor (crossovers at 120 Hz and 2500 Hz,
`compressor.cpp:167`).

- `linkwitz_riley_filter.rs` ports `linkwitz_riley_filter.{h,cpp}`. As
  upstream, coefficients stay zero until `setSampleRate`/`setOversampleAmount`.
  `dc_filter.rs` ports `dc_filter.{h,cpp}`, including upstream's `reset`,
  which loads `past_out_` from the already-reset `past_in_` (lanes outside
  the mask take `past_in_`; covered by `dc_filter/retrigger_voices`).
- New ref-module `linkwitz_riley` (param `cutoff`, outputs `low` and
  `high`). `dc_filter` already had one; it had no cases until now.
- Cases under `module_defaults`, no tolerance changes. `filters/linkwitz_riley`
  has 13 cases (26 outputs): 120, 2500, 10000 and 20000 Hz with noise and
  impulses, 120 Hz saw at 48 kHz, 120 Hz noise at block size 1, 2500 Hz at
  96 kHz, 2500 Hz at 2× oversampling, and 2500 Hz at 192 kHz × 2.
  `filters/dc_filter` has 7 cases: offset saw, square and noise at 44.1, 48
  and 96 kHz, an impulse, per-voice retriggers, 2× oversampling and block
  size 1.
- Parity, x86_64: 33/33 outputs pass. `dc_filter`: worst max_abs_error
  6.3e-6, residual −105.0 dB, and 2 cases bit-identical. `linkwitz_riley`:
  worst max_abs_error 6.2e-5 (`saw_120hz_48k_block_128:low`), worst
  residual −84.9 dB (`noise_120hz_48k_block_1:high`).

### Open: the 120 Hz crossover at oversampled rates (decision needed)

The port is bit-identical to the no-fast-math reference (`REF_FAST_MATH=OFF`)
on every case checked, including the failing ones below, so these numbers are
the fast-math reference's own noise. The cause is in `computeCoefficients`:
GCC `-ffast-math` reassociates `1.0f + kSqrt2 * warp + warp2` into
`(kSqrt2 * warp + warp2) + 1.0f` (disassembly of
`LinkwitzRileyFilter::computeCoefficients` in `ref-module`). That changes
`mult` by about 1 ULP. At a low cutoff relative to the sample rate, the
poles sit next to the unit circle (warp = 1/tan(π·120/88200) ≈ 234), so the
recursion amplifies the coefficient difference. The macOS clang reference
may reassociate differently (unknown until PR #2).

Reference against no-fast-math proxy (identical to the port), residual and
max_abs error, worst of the two bands, noise and saw:

| cutoff | internal rate | residual | max abs |
|---|---|---|---|
| 120 Hz | 44.1 / 48 kHz | −85.2 dB | 5.2e-5 (pass) |
| 120 Hz | 88.2 / 96 kHz | −72.7 dB | 1.9e-4 (fail) |
| 120 Hz | 176.4 / 192 kHz | −66.4 dB | 5.7e-4 (fail) |
| 120 Hz | 352.8 / 384 kHz | −50.8 dB | 2.2e-3 (fail) |
| 120 Hz | 705.6 / 768 kHz | −36.6 dB | 1.8e-2 (fail) |
| 2500 Hz | up to 384 kHz | −87.3 dB | 1.1e-5 (pass) |
| 2500 Hz | 705.6 kHz | −66.2 dB | 4.7e-4 (fail) |
| 20 Hz (not an upstream cutoff) | 44.1 kHz | −63.9 dB | 5.4e-4 (fail) |

These rates are in the working range. The effect chain runs at the engine's
oversampling amount (`sound_engine.cpp:226-236`), and the default
`oversampling` setting is 2×. So the default patch's compressor runs its
120 Hz crossover at 88.2/96 kHz, which fails `module_defaults` for the
reference itself (the CI no-fast-math calibration job would fail these
cases too). Per AGENTS.md, the cases keep the passing band, and the
full-range numbers are recorded here. No faithful normalized form exists:
the outputs are audio, not a large-scale quantity.

**Question for a human:** approve a per-case tolerance exception for
120 Hz at ≥ 88.2 kHz internal (and 2500 Hz at 705.6 kHz), or decide
another way to cover that range (for example, comparing against the macOS
arm64 reference once PR #2 lands, if clang does not reassociate there).
Until then, those cases are not in the corpus.
