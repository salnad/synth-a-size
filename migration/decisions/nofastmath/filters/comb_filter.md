# No-fast-math exact cases: filters/comb_filter

CombFilter. Sweeping or modulating the cutoff into long delay periods (up to about 32k samples) makes the fast-math reference round the per-sample period differently (reassociation and reciprocal-math in `futils::exp2` and the division); the shifted Catmull-Rom read is re-circulated by up to unity feedback (LEDGER 2026-10-21 M5b PR 2). The port is bit-identical to the no-fast-math build on these cases.

- `filters/comb_filter/comb_cutoff_sweep` (SAS-35; x86, macos): cutoff swept into long delay periods; the fast-math period rounding is re-circulated by the feedback
- `filters/comb_filter/comb_resonance_blend_ramps` (SAS-35; x86, macos): cutoff swept into long delay periods; the fast-math period rounding is re-circulated by the feedback
- `filters/comb_filter/negative_flange_sweep` (SAS-35; x86, macos): cutoff swept into long delay periods; the fast-math period rounding is re-circulated by the feedback
- `filters/comb_filter/band_spread_negative_flange` (SAS-35; x86, macos): cutoff swept into long delay periods; the fast-math period rounding is re-circulated by the feedback
- `filters/comb_filter/comb_oversample2` (SAS-35; x86, macos): cutoff swept into long delay periods; the fast-math period rounding is re-circulated by the feedback
- `filters/comb_filter/comb_96k_block128` (SAS-35; macos): cutoff swept into long delay periods; the fast-math period rounding is re-circulated by the feedback
