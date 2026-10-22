## 2026-10-22: Benchmark mod_matrix is like for like after #78

**Why.** The benchmark (#85, `ledger/2026-10-22-perf-benchmark.md`) went in
before modulation routing (#78), so the port connected none of
`mod_matrix`'s 16 modulations. Its Output column read **differs** (+1.3 dB),
and its Rust ÷ C++ ratio (1.20 at 48 kHz/256) understated the port's cost.
#78 is now on main (88965b2), and the owner asked for the like-for-like
number.

**Changed**
- `corpus/bench/scenarios.json`: `mod_matrix` drops its
  `"needs": "modulation routing (#78)"` note.
- `BENCHMARKS.md`: the "Modulation routing" section describes the current
  state.

No code changes.

**Measured.** This VM (Intel Xeon, 8 vCPU). `bench.py --only mod_matrix`,
median of 3 timed 4 s renders after 1 warmup. Two runs: one on main plus
#85 before the merge, one on main at 205b44d.

| Configuration | Rust ÷ C++, run 1 | Rust ÷ C++, run 2 | Rust ÷ no-fast-math, run 1 / run 2 |
|---|--:|--:|--:|
| 44.1 kHz / 64 | 1.44 | 1.49 | 1.29 / 1.37 |
| 44.1 kHz / 256 | 1.51 | 1.39 | 1.35 / 1.29 |
| 44.1 kHz / 512 | 1.45 | 1.41 | 1.31 / 1.30 |
| 48 kHz / 64 | 1.50 | 1.53 | 1.35 / 1.40 |
| 48 kHz / 256 (headline) | 1.50 (5.6 vs 8.4 ×RT) | 1.34 (5.2 vs 7.0 ×RT) | 1.33 / 1.29 |
| 48 kHz / 512 | 1.40 | 1.44 | 1.30 / 1.34 |

- **Range and median.** Across both runs Rust ÷ C++ is 1.34–1.53, median
  1.45. That is in line with the other scenarios (median 1.55 on this VM),
  not the 1.20 measured before #78.
- **Noise.** The headline moved from 1.50 to 1.34 between runs because
  the C++ side ran slower in run 2. That is VM noise; read the median.
- **Output.** In all 12 configurations the Rust output is bit-identical to
  the no-fast-math reference, and −61 to −67 dB from the official one.
- **Load.** About 18–21 % of the block budget for the port and 13–14 %
  for the reference.
