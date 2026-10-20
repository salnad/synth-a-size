# migration/

This folder holds everything needed to port the upstream synth (`/vital`)
to Rust (`/port`) and to prove the port sounds the same.

| Path | What |
|---|---|
| [AGENTS.md](AGENTS.md) | Rules for anyone (human or agent) working on the port. **Read first.** |
| [PLAN.md](PLAN.md) | Milestones M0 to M11 and their exit criteria |
| [LEDGER.md](LEDGER.md) | Append-only work log |
| [UPSTREAM.md](UPSTREAM.md), [UPSTREAM_SHA](UPSTREAM_SHA) | Pinned upstream commit, exclusions, verification |
| [decisions/](decisions/) | Decision records: [plugin framework](decisions/plugin-framework.md), [reference determinism and tolerance calibration](decisions/reference-determinism.md) |
| [reference/](reference/) | C++ reference harness: `ref-render` (preset + MIDI → WAV) and `ref-module` (single processor → NPY) |
| [`port/crates/parity`](../port/crates/parity) | `sas-parity`: Rust test helper, `assert_parity!("<area>/<module>/<case>", output)` against reference captures |
| [tools/audio-diff/](tools/audio-diff/) | Rust comparison tool and library: max abs error, RMS dB difference, null residual, spectral distance, tolerances, exit codes |
| [corpus/](corpus/) | Authored test presets, MIDI files, `cases.json` (renders, defaults) and per-module case files `modules/<area>/<module>.json` |
| [scripts/](scripts/) | `build_reference.sh`, `run_corpus.py` (render / compare / hashes), `verify_upstream.sh`, `check_names.sh`, `check_fp_determinism.sh`, `machine_info.sh` |

## Quick start

```bash
# 1. Build the reference harness (Linux, GCC, CMake) and the comparison tool
migration/scripts/build_reference.sh
cargo build --release --manifest-path migration/tools/audio-diff/Cargo.toml

# 2. Render one preset
migration/reference/build/ref-render migration/corpus/presets/init_saw.vital \
  migration/corpus/midi/single_note_c3.mid -o /tmp/ref.wav

# 3. Render the whole corpus, then compare another tree against it
python3 migration/scripts/run_corpus.py render --out build/ref
python3 migration/scripts/run_corpus.py compare build/ref build/port   # once the port can render

# 4. Compare any two files
migration/tools/audio-diff/target/release/audio-diff ref.wav test.wav --residual-db -60 --lsd-db 0.5
```

CI (`.github/workflows/ci.yml`) runs all of this on Ubuntu 24.04 and
uploads the reference renders as the `reference-renders` artifact.
