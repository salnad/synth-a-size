# No-fast-math exact cases: filters/filter_module

`FilterModule` with its comb model. Sweeping the cutoff into long delay
periods with resonance up to 1 makes the fast-math reference round the
per-sample period differently (reassociation and reciprocal-math in
`futils::exp2` and the division), and unity feedback re-circulates the
shifted Catmull-Rom read, as for the `comb_filter` cases. The port is
bit-identical to the no-fast-math build.

- `filters/filter_module/comb_style1_resonance_drive` (SAS-35; x86, macos): cutoff modulated into long delay periods at resonance up to 1; the fast-math period rounding is re-circulated by the feedback
