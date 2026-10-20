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
