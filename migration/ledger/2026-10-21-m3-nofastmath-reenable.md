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
- **Hashes:** official and no-fast-math, x86 from local CI-identity trees.
  macOS from this PR's `ref-arm64-macos-15` artifact.
- `allowlist-approval --base origin/main` lists all 11 ids as
  `added <id> (SAS-35; x86, macos)`, with 0 problems.
