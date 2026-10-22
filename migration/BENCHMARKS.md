# Performance benchmark

How fast the Rust engine renders compared with the C++ reference, on the
same machine. This measures; it does not optimise. Performance work starts
at M11 against a passing parity suite ([AGENTS.md](AGENTS.md) rule 5,
[PLAN.md](PLAN.md) M11). Ideas found here are parked in
[IMPROVEMENTS.md](IMPROVEMENTS.md) (I9–I13).

## Run it

```bash
python3 migration/scripts/bench.py            # build everything, run every scenario (about 10 min on 8 cores)
python3 migration/scripts/bench.py --quick    # 48 kHz / 256-sample blocks only, 2 s renders, 1 repeat (about 30 s)
```

The script builds three engines and times each scenario at 44.1 and
48 kHz with 64-, 256- and 512-sample host blocks. Engines run interleaved,
one configuration at a time, and the order rotates between configurations.

| Engine | Binary | Build |
|---|---|---|
| `rust` | `port/target/release/sas-bench` | `cargo build --release -p sas-bench`: the workspace release profile, default target CPU (x86-64 baseline / SSE2, or arm64 NEON) |
| `cpp` | `migration/reference/build/ref-bench` | The official reference: `-O3 -ffast-math` like upstream's release builds, with the determinism pins (`-msse2` baseline on x86, no FP contraction, no approximate reciprocals; [reference/README.md](reference/README.md)) |
| `cpp-nofastmath` | `migration/reference/build-nofastmath/ref-bench` | The same with `REF_FAST_MATH=OFF`, the "correct port, different rounding" proxy. The port is not compiled with fast-math either, so this is the like-for-like compiler comparison |

It writes `build/bench/results.json` (every raw measurement, the machine,
the compilers and flags) and `build/bench/report.md`, and prints the report.

Options:
- `--only <scenario>` (repeatable) and `--list`;
- `--engines rust,cpp` to drop an engine;
- `--rates 48000`, `--blocks 128,256`, `--length 8`, `--repeats 5`, `--warmup 2`;
- `--no-build` to reuse the binaries as built;
- `--rust-bin`, `--cpp-bin` and `--cpp-nofastmath-bin` to time other builds, for example a branch's `sas-bench` against main's.

The harnesses can be run alone. They take `ref-render`'s preset and MIDI
arguments and print JSON:

```bash
cd migration/corpus
../reference/build/ref-bench bench/presets/heavy_fx.vital bench/midi/held_chord_4.mid \
  --sample-rate 48000 --block-size 256 --length 4 --repeats 3
../../port/target/release/sas-bench bench/presets/heavy_fx.vital bench/midi/held_chord_4.mid \
  --sample-rate 48000 --block-size 256 --length 4 --repeats 3
```

## What is timed

`ref-bench` (`reference/src/bench_main.cpp`) runs `ref-render`'s own
block loop (`RenderSynth::renderBlock`, shared through
`reference/src/render_synth.h`). `sas-bench` (`port/crates/bench`) runs
the port's copy of that loop (`Synth::render_block`). For each
configuration, each binary:

1. builds a fresh synth and loads the preset (not timed), with the random
   seed counter reset, so every repeat renders the same audio;
2. renders `--warmup` untimed passes (default 1), then `--repeats` timed
   passes (default 3) of `--length` seconds (default 4);
3. times the block loop: wall-clock and thread CPU time for the whole
   loop, and the wall-clock time of every `renderBlock` call.

The MIDI notes start at t = 0 and are held past the end of the render, so
every scenario measures a steady voice count, plus the note-on work in the
first block.

## Scenarios

Defined in [`corpus/bench/scenarios.json`](corpus/bench/scenarios.json).
The presets and MIDI are authored in
[`corpus/generate.py`](corpus/generate.py) (`bench/`); there is no
factory content.

| Scenario | Patch | Voices |
|---|---|---|
| `init_single_voice` | The init patch (engine defaults, which include 2x oversampling) | 1 |
| `poly16_unison` | `poly_unison`: oscillator 1 with 8 unison voices, oscillator 2 with 4, analog low-pass | 16 |
| `poly32_unison` | The same | 32 (maximum polyphony) |
| `heavy_fx` | 4 unison voices through all nine effects | 4 |
| `oversampling_1x` … `_8x` | Unison, ladder filter and distortion at each oversampling setting | 4 |
| `mod_matrix` | Two oscillators, both filters, 16 modulation connections | 8 |

To add one, add its preset or MIDI to `generate.py` (`BENCH_PRESETS`,
`BENCH_MIDI`), run it, and add an entry to `scenarios.json`.

### Modulation routing (#78)

The port does not connect a preset's modulations until #78 lands. Until
then `mod_matrix` is not like for like: the C++ engine routes 16
connections and the port none, so its report row shows **differs** in the
Output column, and its Rust ÷ C++ ratio understates the port's cost. The
other scenarios have no modulations, so they compare the same work today.

Once #78 is on main, re-run the same command; nothing needs editing. The
Output column then shows `mod_matrix` as `same` (and its `needs` note can
be dropped from `scenarios.json`). The modulation-routing scenarios to
watch are `mod_matrix` and any preset with modulations added later.

## Reading the report

The headline table is the 48 kHz / 256-sample configuration, one row per
scenario:

| Column | Meaning |
|---|---|
| ×RT | Realtime factor: seconds of audio rendered per second of wall-clock time, from the median timed repeat. Higher is faster. 1.0 means just real time on one core |
| Rust ÷ C++ | Rust render time over the official reference's. Above 1 means the port is slower (1.50 = 50 % slower) |
| Rust ÷ C++ no-fast-math | The same against the no-fast-math build, the like-for-like compiler comparison |
| Output | The Rust render against the official reference's: residual in dB (RMS of the difference over RMS of the reference, as `audio-diff`). `same` at or below −40 dB means both engines did the same work, so the timing is a fair comparison; **differs** means it is not (see `mod_matrix`). "bit-exact with no-fast-math" means the Rust output hash equals the no-fast-math build's |

The second table has every configuration and engine:

| Column | Meaning |
|---|---|
| CPU µs/block | Thread CPU time of the timed loop over the number of host blocks (median repeat) |
| % budget | CPU µs/block over the block's real-time duration (256 samples at 48 kHz = 5333 µs). A plugin needs this well under 100 % |
| p99 µs, max µs | Wall-clock time of single blocks, across all timed repeats. Spikes here are what cause dropouts in a host |
| first µs | The first block, where every note starts, averaged over repeats |

Rules of thumb:
- Compare engines within one run, never numbers from different machines or
  runs. The engines run interleaved on the same machine, so the ratios are
  much steadier than the absolute ×RT.
- On shared CI runners, treat differences under about 10 % as noise.
- Smaller blocks cost more per sample on both engines (per-block overhead),
  and 64-sample blocks show it most.

## CI

[`.github/workflows/benchmarks.yml`](../.github/workflows/benchmarks.yml)
never gates a merge. It runs:
- nightly (with the full Mac suite) and on manual dispatch (Actions →
  Benchmarks → Run workflow, with optional `quick` and `only`), on
  `ubuntu-24.04` (x86_64) and `macos-15` (arm64);
- on a pull request only when it changes the benchmark itself (the
  workflow, `bench.py`, `corpus/bench/`, the two harnesses).

Each job uploads `build/bench/` as the `bench-linux-x86_64` or
`bench-macos-arm64` artifact (30 days) and writes the report to the job
summary. A job fails only when a binary fails or renders a non-finite
sample, never because a number is slow.

Recorded results are in the ledger entries that add or change the
benchmark, starting with
[`ledger/2026-10-22-perf-benchmark.md`](ledger/2026-10-22-perf-benchmark.md).
