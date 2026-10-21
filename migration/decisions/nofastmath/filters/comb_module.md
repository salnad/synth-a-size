# No-fast-math exact cases: filters/comb_module

CombModule. Sweeping or modulating the cutoff into long delay periods (up to about 32k samples) makes the fast-math reference round the per-sample period differently (reassociation and reciprocal-math in `futils::exp2` and the division); the shifted Catmull-Rom read is re-circulated by up to unity feedback (LEDGER 2026-10-21 M5b PR 2). The port is bit-identical to the no-fast-math build on these cases.

- `filters/comb_module/comb_style_sweep` (SAS-35; x86): cutoff swept into long delay periods; the fast-math period rounding is re-circulated by the feedback
- `filters/comb_module/band_spread_full_range` (SAS-35; x86): cutoff swept into long delay periods; the fast-math period rounding is re-circulated by the feedback
