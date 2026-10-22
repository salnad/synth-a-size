## 2026-10-22: Performance benchmark, Rust engine vs the C++ reference

**Why.** The owner (Salil) asked for a performance benchmark of the Rust
engine side by side with the C++ reference, separate from the
modulation-routing work (#78). This entry measures only; there are no
performance changes (rule 5: optimisation is M11). The ideas found are
parked as I9–I13 in `IMPROVEMENTS.md`.

**Changed**

- `migration/reference/src/bench_main.cpp`, a new `ref-bench`. It times
  `ref-render`'s own block loop:
  - each repeat builds a fresh synth and loads the preset (not timed);
  - it times the loop's wall-clock and thread CPU time, plus the
    wall-clock time of every `renderBlock`;
  - it prints JSON.

  `RenderSynth` and `loadMidi` moved unchanged from `render_main.cpp`
  into `render_synth.h`, which both tools share. `ref-render` output is
  unchanged: a local render with the CI build identity (Ubuntu 24.04, GCC
  13.3, glibc 2.39) reproduces all 2,746 committed hashes
  (`hashes --check --require-known-identity`, 0 problems).
- `port/crates/bench` (`sas-bench`): the port's side, with the same
  options and JSON. It runs `Synth::render_block` and hashes with the
  harness's FNV basis. Unit tests cover percentiles, the hash, and
  identical timed repeats.
- `corpus/generate.py` writes authored presets and held-note MIDI to
  `corpus/bench/`. `corpus/bench/scenarios.json` defines 9 scenarios:
  - `init_single_voice`;
  - `poly16_unison` and `poly32_unison`;
  - `heavy_fx` (all nine effects);
  - `oversampling_1x` / `_2x` / `_4x` / `_8x`;
  - `mod_matrix` (16 connections).

  None of it is factory content or uses product names. These are not
  parity cases: no hashes, no `state.json` cases.
- `migration/scripts/bench.py` is the one command. It builds the official
  reference, the no-fast-math reference and `sas-bench`, then runs every
  scenario at 44.1/48 kHz × 64/256/512 with the engines interleaved. It
  writes `build/bench/{results.json,report.md}`.
- `.github/workflows/benchmarks.yml` runs nightly (03:23, the full Mac
  suite's slot) and on dispatch, on `ubuntu-24.04` and `macos-15`. On a
  pull request it runs only when the benchmark itself changes. It never
  gates a merge. Results go to the `bench-<platform>` artifacts and the
  job summary.
- `migration/BENCHMARKS.md` explains how to run and read it.
  `IMPROVEMENTS.md` gains I9–I13.

**Measured** (this VM: Intel Xeon, 8 vCPU, Linux 6.12, rustc 1.99.1, GCC
13.3). Each figure is the median of 3 timed 4 s renders after 1 warmup.
Headline at 48 kHz, 256-sample blocks (second full run):

| Scenario | Rust ×RT | C++ ×RT | C++ no-fast-math ×RT | Rust ÷ C++ | Rust ÷ no-fast-math |
|---|--:|--:|--:|--:|--:|
| `init_single_voice` | 49.2 | 80.6 | 72.4 | 1.64 | 1.47 |
| `poly16_unison` | 3.4 | 5.2 | 4.6 | 1.52 | 1.34 |
| `poly32_unison` | 1.7 | 2.4 | 2.2 | 1.39 | 1.30 |
| `heavy_fx` | 9.2 | 13.0 | 12.8 | 1.41 | 1.38 |
| `oversampling_1x` | 25.4 | 38.7 | 34.7 | 1.52 | 1.36 |
| `oversampling_2x` | 15.4 | 24.7 | 23.8 | 1.60 | 1.55 |
| `oversampling_4x` | 10.7 | 16.3 | 13.8 | 1.53 | 1.29 |
| `oversampling_8x` | 6.5 | 10.4 | 9.6 | 1.61 | 1.49 |
| `mod_matrix` (not like for like) | 6.9 | 8.2 | 7.1 | 1.20 | 1.04 |

- **Across all 48 like-for-like configurations**, the port takes 1.38–1.91×
  the official reference's render time in the first run and 1.09–2.23× in
  the second, with a median of 1.53 and 1.55. Against the no-fast-math
  build it is a median of 1.39 and 1.38. Block size barely matters: the
  medians are 1.59 at 64, 1.52 at 256 and 1.50 at 512.
- **Same work.** In those 48 configurations the Rust output is
  bit-identical to the no-fast-math reference (equal hashes). Against the
  official reference it is −50 to −135 dB, which is that build's own
  fast-math noise.
- **`mod_matrix` differs** by +1.3 dB, as expected: main does not connect
  modulations yet (#78), so its ratio understates the port's cost.
- **Load.** At 32 voices the port uses 60 % of a 256-sample block's budget
  at 48 kHz; the reference uses 40 %.
- **CI runners** (the PR's `Benchmarks` run 39862920769, commit `df40fe8`,
  same matrix):

  | 48 kHz / 256 | macOS 15 arm64 (M1, 3 vCPU) Rust ÷ C++ | Linux x86_64 (AMD EPYC 9V74, 4 vCPU) Rust ÷ C++ |
  |---|--:|--:|
  | `init_single_voice` | 1.64 (40.4 vs 66.2 ×RT) | 1.81 (37.6 vs 68.1) |
  | `poly16_unison` | 1.78 (2.1 vs 3.7) | 2.11 (2.2 vs 4.7) |
  | `poly32_unison` | 1.77 (1.3 vs 2.3) | 2.09 (1.1 vs 2.4) |
  | `heavy_fx` | 1.64 (4.4 vs 7.2) | 1.78 (6.5 vs 11.5) |
  | `oversampling_1x` | 1.72 (19.2 vs 32.9) | 1.80 (20.5 vs 36.9) |
  | `oversampling_2x` | 1.64 (12.9 vs 21.2) | 1.93 (13.2 vs 25.4) |
  | `oversampling_4x` | 1.78 (6.5 vs 11.5) | 2.00 (7.8 vs 15.6) |
  | `oversampling_8x` | 1.67 (3.8 vs 6.3) | 2.08 (4.2 vs 8.8) |
  | `mod_matrix` (not like for like) | 1.52 | 1.52 |

  - On macOS, across all 48 like-for-like configurations, the median is
    1.71× the official build (range 1.30–2.38) and 1.67× the no-fast-math
    build. The output is bit-identical to the macOS no-fast-math reference
    in all 48.
  - On the CI Linux runner the median is 2.00× the official build (range
    1.77–2.12): the gap depends on the CPU (AMD there, Intel on this VM).
  - At 32 voices the port reaches about 76 % (macOS) and 88 % (Linux
    runner) of the block budget, where the reference uses about 42 %.

**Hotspots** (`perf` on this VM, Rust and C++ side by side)

1. **The oscillator's wavetable interpolation** is the biggest gap. In
   `poly16_unison` the oscillator outside its FFT costs about 2.4× the C++
   time. `poly_utils::get_value_matrix_separate` and
   `get_catmull_interpolation_matrix` are out-of-line cross-crate calls
   (I10).
2. **The ladder filter's saturator.** `futils::algebraic_sat` is an
   out-of-line call: 17 % of the run at 8x oversampling. The Rust ladder
   costs about 1.6× upstream's (I11).
3. **The first block after building the synth** takes about 3–5.6 ms
   against 0.6–0.8 ms in C++ on the init patch at 44.1 kHz/64, and 13 ms
   against 3.6 ms at 8x. It is the same with no notes in that block, and
   later note-ons do not spike. 83 % of it is page faults:
   `ReorderableEffectChain` → `enable` → `hard_reset` →
   `DelayMemory::clear_all` writes `calloc`'d memory for the first time.
   Upstream's `make_unique<poly_float[]>` touches those pages at
   construction (I12). It is Linux only (glibc `calloc`). On the macOS CI
   runner the port's first block is 0.4 ms against the reference's
   0.6 ms (init patch, 44.1 kHz/64).
4. **The oscillators' per-block inverse real FFT** is the largest single
   cost in both engines (34 % Rust, about 45 % C++ in `poly16_unison`).
   The port's is about 1.1× the C++ one (I13).
5. **Graph dispatch** (`router_process`, `with_kernel`) is under 2 % in
   steady state; it is not where the gap is.

**LTO probe** (not adopted). `CARGO_PROFILE_RELEASE_LTO=fat`,
`CODEGEN_UNITS=1`: Rust ÷ C++ improves by 0–12 % per scenario (for
example `poly16_unison` 1.51 → 1.33), and every output hash is unchanged
(I9).

**Caveats**

- **One machine per platform.** Shared cloud VMs and CI runners are noisy:
  single configurations moved up to ±20 % between runs, though the medians
  held. Compare engines within one run.
- **The C++ side is the reference build, not upstream's shipped binary.**
  - It has the determinism pins: SSE2 baseline (as upstream's Linux
    `-msse2` and Rust's default), `-mno-recip`, `-ffp-contract=off`, and
    baseline glibc libm (`GLIBC_TUNABLES`).
  - On macOS it uses JUCE's fallback FFT, not vDSP, and no FMA
    contraction, so upstream's real Mac plugin is somewhat faster than
    the `cpp` column there.
- **The Rust side is the default workspace release profile** (no LTO,
  `debug = 1`).
- **The first render in a process differs** from later ones in both
  engines, from process-wide state. The harnesses' one untimed warmup
  makes every timed repeat identical.
- **Until #78 lands, `mod_matrix` is the only scenario with
  modulations.** Re-run the same command afterwards; its Output column
  should then read `same`.

**Blocked / follow-ups.** None blocking. When #78 lands, re-run and drop
`mod_matrix`'s `needs` note. I12 is arguably real-time safety rather than
speed. It does not affect the shipped macOS plugin today, but would affect
a Linux build (owner call).
