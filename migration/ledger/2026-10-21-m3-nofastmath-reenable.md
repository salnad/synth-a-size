## 2026-10-21: M3 parked cases re-enabled against the no-fast-math build (SAS-35)

Re-enables the 11 modulation cases parked in the M3 PRs, under SAS-35
(`migration/decisions/nofastmath/README.md`). Each case moved unchanged
from `migration/corpus/modules/modulation/pending/` into its module file
with `"reference": "nofastmath"`, and the pending files are gone. The ids
are listed in `migration/decisions/nofastmath/modulation/{lfo_module,
random_lfo,random_lfo_module}.md`.

- **Justification on x86** (`compare --justify-nofastmath`, official
  against no-fast-math; every id prints `justified on x86`):
  - `modulation/lfo_module/keytrack_audio`: `value` 4.22e-3, −57.5 dB.
  - `modulation/lfo_module/keytrack_range_audio`: `osc_phase` 0.90,
    −29.6 dB; `value` 4.65e-3, −56.8 dB.
  - `modulation/lfo_module/keytrack_range_control`: `osc_phase` 0.90,
    −29.6 dB; `value` 4.53e-3, −56.7 dB.
  - `modulation/random_lfo/lorenz_mono_audio`: `value` 0.857, −6.0 dB.
  - `modulation/random_lfo/lorenz_mono_control`: `value` 0.878, −5.8 dB.
  - `modulation/random_lfo/lorenz_stereo_audio`: `value` 0.924, −5.7 dB.
  - `modulation/random_lfo/lorenz_stereo_control`: `value` 0.910, −5.5 dB.
  - `modulation/random_lfo/lorenz_frequency_sweep`: `value` 0.921,
    −6.5 dB.
  - `modulation/random_lfo/lorenz_max_frequency`: `value` 0.871, −6.2 dB.
  - `modulation/random_lfo_module/free_lorenz`: `value` 2.27e-4,
    −87.5 dB.
  - `modulation/random_lfo_module/tempo_extremes` (Lorenz style): `value`
    0.187, −28.7 dB.
- **Why:** at high keytrack notes the LFO integrates the reference's
  ~1e-7 relative keytrack-frequency noise into phase drift. The Lorenz
  attractor is chaotic, so a one-ulp difference grows to full scale. The
  keytrack frequency itself stays on the official reference, in consumer
  form, in `lfo_module_frequency`.
- **Port exactness on x86:** all 11 cases (17 outputs) are bit-identical
  to the x86 no-fast-math captures (`SAS_PARITY_CAPTURES_NOFASTMATH`).
- **Justification on macOS** (this PR's Reference ARM run 39667063716,
  macOS 15 job): every id prints `justified on macos`, so the `x86, macos`
  declarations stand. Official against no-fast-math on macOS:
  - the keytrack cases: `value` 4.22e-3 to 4.65e-3 (−56.7 to −57.5 dB), and
    `osc_phase` 0.90 (−29.6 dB) on the two sweeps;
  - the Lorenz cases match x86 exactly: the macOS captures are
    bit-identical to the x86 ones in both builds (pure arithmetic), with
    `value` 0.857 to 0.924 (−5.5 to −6.5 dB), `free_lorenz` 2.27e-4
    (−87.5 dB) and `tempo_extremes` 0.187 (−28.7 dB);
  - the keytrack captures differ bitwise between the platforms, with the
    same miss.
- **Port exactness on macOS:** with `aarch64-semantics`, the port is
  bit-identical to the macOS no-fast-math captures on all 11 cases. The
  macOS job runs the same sas-parity check natively; it is non-gating by
  owner policy.
- **Hashes:** official and no-fast-math on both targets. x86 comes from
  local CI-identity trees. macOS comes from the `ref-arm64-macos-15`
  artifact of run 39667063716 (`pinned`, and `proxy` with
  `--reference nofastmath`).
- `allowlist-approval --base origin/main` lists all 11 ids as
  `added <id> (SAS-35; x86, macos)`, with 0 problems.
