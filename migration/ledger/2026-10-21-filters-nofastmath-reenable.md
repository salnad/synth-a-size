## 2026-10-21: filters, 50 parked cases re-enabled as no-fast-math exact (SAS-35)

**Changed.** Every case under `migration/corpus/modules/filters/pending/` moved back into its module file with inputs unchanged and `"reference": "nofastmath"`, following `migration/decisions/nofastmath/README.md`. The `pending/` directory is gone. The cases are:

- Core Filters' 41 parked cases: `linkwitz_riley` (30), `ladder` (7), `analog` (1) and `dirty` (3);
- the macOS-only park `filters/dirty/dual_notch_band_cutoff_sweep`;
- Character Filters' comb parks: `comb_filter` (6, including the macOS-only `comb_96k_block128`) and `comb_module` (2).

Two parked ladder cases collided with active case names and were renamed, with their contents unchanged: `k24_retrigger_voices` became `k24_retrigger_voices_self_oscillating`, and `k12_oversample_4x` became `k12_oversample_4x_resonance_0_95`.

**Allowlists.** New files `migration/decisions/nofastmath/filters/{linkwitz_riley,ladder,analog,dirty,comb_filter,comb_module}.md`, 50 ids in all.

- 45 are declared `x86, macos`, 3 `x86` and 2 `macos`.
- `compare --justify-nofastmath` prints `justified on x86` for all 48 x86-declared ids on the local x86 trees, which have the CI build identity.
- On this PR's `ref-arm64-macos-15` trees (Reference ARM run 39641601849), it prints `justified on macos` for all 47 macOS-declared ids.
- `allowlist-approval --base origin/main` lists 50 additions with 0 problems.

**Measurements.** Worst output of each case, official reference against the same target's no-fast-math build, as max_abs_error / residual (computed from the captures; `compare --justify-nofastmath` gives the authoritative pass/fail above). The limits are 1e-4 and −80 dB.

| Case | Targets | x86 official vs no-fast-math | macOS official vs no-fast-math |
|---|---|---|---|
| `analog/k12_oversample_4x_resonance_0_95` | x86 | 1.00e-04 / -84.5 dB (fails) | 8.65e-05 / -85.7 dB (passes) |
| `dirty/k12_cutoff_sweep` | x86, macos | 1.38e-04 / -94.6 dB (fails) | 2.27e-04 / -90.2 dB (fails) |
| `dirty/k24_cutoff_sweep` | x86, macos | 1.27e-04 / -90.8 dB (fails) | 1.55e-04 / -88.7 dB (fails) |
| `dirty/k12_oversample_4x_resonance_0_95` | x86, macos | 1.11e-04 / -85.4 dB (fails) | 1.03e-04 / -86.4 dB (fails) |
| `dirty/dual_notch_band_cutoff_sweep` | macos | 9.35e-05 / -90.2 dB (passes) | 3.24e-04 / -81.5 dB (fails) |
| `ladder/k12_cutoff_sweep` | x86, macos | 4.55e-01 / -22.1 dB (fails) | 4.48e-01 / -21.0 dB (fails) |
| `ladder/k24_cutoff_sweep` | x86, macos | 2.82e-01 / -12.1 dB (fails) | 5.74e-04 / -82.3 dB (fails) |
| `ladder/notch_pass_swap_cutoff_sweep` | x86, macos | 4.14e-01 / -5.5 dB (fails) | 1.70e-02 / -52.2 dB (fails) |
| `ladder/dual_notch_band_cutoff_sweep` | x86, macos | 8.56e-01 / -19.8 dB (fails) | 8.32e-01 / -24.4 dB (fails) |
| `ladder/band_peak_notch_cutoff_sweep` | x86, macos | 1.05e+00 / -8.0 dB (fails) | 2.70e-02 / -57.6 dB (fails) |
| `ladder/k24_retrigger_voices_self_oscillating` | x86, macos | 5.99e-04 / -92.3 dB (fails) | 1.00e-03 / -84.5 dB (fails) |
| `ladder/k12_oversample_4x_resonance_0_95` | x86, macos | 1.98e-04 / -82.6 dB (fails) | 1.84e-04 / -83.4 dB (fails) |
| `linkwitz_riley/noise_120hz_44_1k_x2` | x86, macos | 1.30e-04 / -77.7 dB (fails) | 1.30e-04 / -77.1 dB (fails) |
| `linkwitz_riley/saw_120hz_44_1k_x2` | x86, macos | 1.66e-04 / -76.3 dB (fails) | 1.66e-04 / -77.8 dB (fails) |
| `linkwitz_riley/noise_120hz_48k_x2` | x86, macos | 1.43e-04 / -76.5 dB (fails) | 1.50e-04 / -77.0 dB (fails) |
| `linkwitz_riley/saw_120hz_48k_x2` | x86, macos | 1.89e-04 / -72.7 dB (fails) | 2.13e-04 / -76.8 dB (fails) |
| `linkwitz_riley/noise_120hz_44_1k_x4` | x86, macos | 4.14e-04 / -68.8 dB (fails) | 3.43e-04 / -67.7 dB (fails) |
| `linkwitz_riley/saw_120hz_44_1k_x4` | x86, macos | 4.06e-04 / -67.3 dB (fails) | 5.23e-04 / -67.0 dB (fails) |
| `linkwitz_riley/noise_120hz_48k_x4` | x86, macos | 3.47e-04 / -68.4 dB (fails) | 3.70e-04 / -66.1 dB (fails) |
| `linkwitz_riley/saw_120hz_48k_x4` | x86, macos | 5.74e-04 / -66.4 dB (fails) | 5.85e-04 / -66.7 dB (fails) |
| `linkwitz_riley/noise_120hz_88_2k_x2` | x86, macos | 4.14e-04 / -68.8 dB (fails) | 3.43e-04 / -67.7 dB (fails) |
| `linkwitz_riley/saw_120hz_88_2k_x2` | x86, macos | 4.06e-04 / -67.3 dB (fails) | 5.23e-04 / -67.0 dB (fails) |
| `linkwitz_riley/noise_120hz_96k_x2` | x86, macos | 3.47e-04 / -68.4 dB (fails) | 3.70e-04 / -66.1 dB (fails) |
| `linkwitz_riley/saw_120hz_96k_x2` | x86, macos | 5.74e-04 / -66.4 dB (fails) | 5.85e-04 / -66.7 dB (fails) |
| `linkwitz_riley/noise_120hz_176_4k_x1` | x86, macos | 4.14e-04 / -68.8 dB (fails) | 3.43e-04 / -67.7 dB (fails) |
| `linkwitz_riley/saw_120hz_176_4k_x1` | x86, macos | 4.06e-04 / -67.3 dB (fails) | 5.23e-04 / -67.0 dB (fails) |
| `linkwitz_riley/noise_120hz_192k_x1` | x86, macos | 3.47e-04 / -68.4 dB (fails) | 3.70e-04 / -66.1 dB (fails) |
| `linkwitz_riley/saw_120hz_192k_x1` | x86, macos | 5.74e-04 / -66.4 dB (fails) | 5.85e-04 / -66.7 dB (fails) |
| `linkwitz_riley/noise_120hz_88_2k_x4` | x86, macos | 1.21e-03 / -57.6 dB (fails) | 9.06e-04 / -52.6 dB (fails) |
| `linkwitz_riley/saw_120hz_88_2k_x4` | x86, macos | 2.18e-03 / -50.8 dB (fails) | 2.43e-03 / -55.1 dB (fails) |
| `linkwitz_riley/noise_120hz_96k_x4` | x86, macos | 1.14e-03 / -55.0 dB (fails) | 1.19e-03 / -52.3 dB (fails) |
| `linkwitz_riley/saw_120hz_96k_x4` | x86, macos | 2.22e-03 / -50.9 dB (fails) | 2.34e-03 / -56.4 dB (fails) |
| `linkwitz_riley/noise_120hz_176_4k_x2` | x86, macos | 1.21e-03 / -57.6 dB (fails) | 9.06e-04 / -52.6 dB (fails) |
| `linkwitz_riley/saw_120hz_176_4k_x2` | x86, macos | 2.18e-03 / -50.8 dB (fails) | 2.43e-03 / -55.1 dB (fails) |
| `linkwitz_riley/noise_120hz_192k_x2` | x86, macos | 1.14e-03 / -55.0 dB (fails) | 1.19e-03 / -52.3 dB (fails) |
| `linkwitz_riley/saw_120hz_192k_x2` | x86, macos | 2.22e-03 / -50.9 dB (fails) | 2.34e-03 / -56.4 dB (fails) |
| `linkwitz_riley/noise_120hz_176_4k_x4` | x86, macos | 3.05e-03 / -41.7 dB (fails) | 3.38e-03 / -35.5 dB (fails) |
| `linkwitz_riley/saw_120hz_176_4k_x4` | x86, macos | 7.75e-03 / -41.5 dB (fails) | 8.61e-03 / -41.0 dB (fails) |
| `linkwitz_riley/noise_120hz_192k_x4` | x86, macos | 3.21e-03 / -38.2 dB (fails) | 3.59e-03 / -33.3 dB (fails) |
| `linkwitz_riley/saw_120hz_192k_x4` | x86, macos | 1.79e-02 / -36.6 dB (fails) | 4.49e-02 / -29.5 dB (fails) |
| `linkwitz_riley/noise_2500hz_176_4k_x4` | x86 | 4.48e-05 / -66.8 dB (fails) | 3.71e-05 / -90.2 dB (passes) |
| `linkwitz_riley/saw_2500hz_176_4k_x4` | x86 | 4.73e-04 / -66.2 dB (fails) | 6.40e-05 / -89.7 dB (passes) |
| `comb_filter/comb_cutoff_sweep` | x86, macos | 2.17e-04 / -95.6 dB (fails) | 6.08e-04 / -91.5 dB (fails) |
| `comb_filter/comb_resonance_blend_ramps` | x86, macos | 1.29e-04 / -92.2 dB (fails) | 1.01e-04 / -91.6 dB (fails) |
| `comb_filter/negative_flange_sweep` | x86, macos | 8.02e-04 / -76.8 dB (fails) | 5.82e-04 / -79.1 dB (fails) |
| `comb_filter/band_spread_negative_flange` | x86, macos | 1.64e-04 / -84.3 dB (fails) | 1.79e-04 / -86.9 dB (fails) |
| `comb_filter/comb_oversample2` | x86, macos | 1.16e-04 / -88.2 dB (fails) | 1.37e-04 / -86.7 dB (fails) |
| `comb_filter/comb_96k_block128` | macos | 9.77e-05 / -100.9 dB (passes) | 1.52e-04 / -101.6 dB (fails) |
| `comb_module/comb_style_sweep` | x86, macos | 2.82e-04 / -92.4 dB (fails) | 7.84e-04 / -86.6 dB (fails) |
| `comb_module/band_spread_full_range` | x86, macos | 1.17e-04 / -93.7 dB (fails) | 1.16e-04 / -93.5 dB (fails) |

**Port exactness.** On x86, `cargo test -p sas-filters` with both capture trees is bit-identical to the no-fast-math captures on all 50 cases. On macOS arm64, the Reference ARM workflow's Rust parity step on this PR runs the same tests against the macOS no-fast-math captures; its result is recorded in the PR.

**Hashes.** These were added for every re-enabled case, for both builds on both targets: x86 official and no-fast-math from full local renders with the CI identity, and macOS official and no-fast-math from the `ref-arm64-macos-15` artifact. No existing hash changed.

**Coverage restored.** The 120 Hz compressor crossover is covered again at every internal rate from 88.2 kHz to 768 kHz, which includes the default 2× oversampling. The resonant ladder's high-cutoff band is covered again, as are the resonant full-range Dirty sweeps.
