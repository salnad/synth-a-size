# No-fast-math exact cases: filters/ladder

LadderFilter. With resonance, once the cutoff/rate ratio is high the ladder turns unstable and chaotic, so the fast-math reference's per-sample rounding difference grows to full scale (LEDGER 2026-10-21 M5a PR 4). The port is bit-identical to the no-fast-math build on these cases.

- `filters/ladder/k12_cutoff_sweep` (SAS-35; x86): resonant sweep into the chaotic high-cutoff band amplifies per-sample fast-math rounding
- `filters/ladder/k24_cutoff_sweep` (SAS-35; x86): resonant sweep into the chaotic high-cutoff band amplifies per-sample fast-math rounding
- `filters/ladder/notch_pass_swap_cutoff_sweep` (SAS-35; x86): resonant sweep into the chaotic high-cutoff band amplifies per-sample fast-math rounding
- `filters/ladder/dual_notch_band_cutoff_sweep` (SAS-35; x86): resonant sweep into the chaotic high-cutoff band amplifies per-sample fast-math rounding
- `filters/ladder/band_peak_notch_cutoff_sweep` (SAS-35; x86): resonant sweep into the chaotic high-cutoff band amplifies per-sample fast-math rounding
- `filters/ladder/k24_retrigger_voices_self_oscillating` (SAS-35; x86): self-oscillating lanes amplify per-sample fast-math rounding
- `filters/ladder/k12_oversample_4x_resonance_0_95` (SAS-35; x86): resonance 0.95 at 4× amplifies per-sample fast-math rounding
