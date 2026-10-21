# No-fast-math exact cases: filters/dirty

DirtyFilter. Resonant sweeps to the top of the cutoff range and resonance 0.95 at 4× amplify the fast-math reference's per-sample rounding just past the absolute gate (LEDGER 2026-10-21 M5a PR 7). The port is bit-identical to the no-fast-math build on these cases.

- `filters/dirty/k12_cutoff_sweep` (SAS-35; x86): resonant sweep to the top of the cutoff range amplifies per-sample fast-math rounding
- `filters/dirty/k24_cutoff_sweep` (SAS-35; x86): resonant sweep to the top of the cutoff range amplifies per-sample fast-math rounding
- `filters/dirty/k12_oversample_4x_resonance_0_95` (SAS-35; x86): resonance 0.95 at 4× amplifies per-sample fast-math rounding
- `filters/dirty/dual_notch_band_cutoff_sweep` (SAS-35; macos): resonant sweep to the top of the cutoff range amplifies per-sample fast-math rounding (passes on x86 at 9.4e-5, 6% under the gate)
