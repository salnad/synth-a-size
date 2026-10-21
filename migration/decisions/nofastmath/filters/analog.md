# No-fast-math exact cases: filters/analog

SallenKeyFilter. Resonance 0.95 at 4× amplifies the fast-math reference's per-sample rounding just past the absolute gate (LEDGER 2026-10-21 M5a PR 6). The port is bit-identical to the no-fast-math build on these cases.

- `filters/analog/k12_oversample_4x_resonance_0_95` (SAS-35; x86): resonance 0.95 at 4× amplifies per-sample fast-math rounding
