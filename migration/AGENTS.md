# Rules for agents working on the port

Read this file before changing anything in this repository. These rules
take precedence over convenience. If a task seems to need breaking one,
stop and ask a human.

## The project

synth-a-size is a Rust port of the open-source upstream wavetable synth
vendored in `/vital`, shipped as a nice-plug VST3/CLAP plugin. After the
port it gains an LLM control layer. The plan is in [PLAN.md](PLAN.md),
the work log in [LEDGER.md](LEDGER.md), and the upstream pin in
[UPSTREAM.md](UPSTREAM.md).

```
/vital/       upstream source, read-only reference
/port/        Rust Cargo workspace (the product)
/migration/   reference harness, comparison tool, corpus, docs, CI glue
```

## Hard rules

1. **License: GPLv3.** Everything stays GPL-3.0-or-later. Do not add
   dependencies whose licenses are incompatible with GPLv3 (proprietary,
   SSPL, non-commercial, GPL-2.0-only, etc.). The port is a derivative
   work of GPLv3 code. Keep upstream copyright notices when you translate
   a file, and say in the module docs which upstream file it ports.
2. **Never use the upstream product names for our product.** Crates,
   binaries, plugin names, vendor strings, bundle IDs, VST3/CLAP IDs,
   window titles, UI text, docs about our product and marketing copy must
   not use "Vital", "Vital Audio", "Tytel" or "Matt Tytel". Exceptions:
   the untouched copy in `/vital/`; factual references to upstream in
   migration docs and code comments (e.g. "ports
   `vital/src/synthesis/filters/digital_svf.cpp`"); and the upstream
   preset file format, which keeps its `.vital` extension for
   compatibility. `migration/scripts/check_names.sh` enforces the
   metadata part in CI.
3. **Never ship factory presets.** Do not commit, download, embed or
   generate from upstream factory presets, banks, wavetables or samples,
   in the corpus, tests, fixtures, docs or the product. Test content is
   authored in `migration/corpus/generate.py`.
4. **Drop Firebase / auth.** No login, account, Firebase, telemetry,
   license-check or "expired build" code in the port. Port engine
   behaviour as upstream builds it with `NO_AUTH=1`.
5. **Parity first, then optimise.** Until M11 the goal is audio parity
   with the reference. Write the straightforward translation of the
   upstream algorithm. No SIMD tricks, approximations, lookup changes,
   reordered arithmetic or "cleanups" that change numerics unless parity
   tests prove they do not. Performance work starts at M11, and only
   against a passing parity suite.
6. **Never loosen tests or tolerances to make them pass.** Do not raise
   a limit in `migration/corpus/cases.json`, an `audio-diff` invocation
   or a port test; do not delete, skip, `#[ignore]` or narrow a test; do
   not shorten a render; do not change a reference to match the port. A
   failing parity test means the port is wrong until shown otherwise. If
   you believe a tolerance is wrong, write the evidence in `LEDGER.md` and
   ask a human. A tolerance may only be loosened in a commit that cites
   the human approval recorded in the ledger. Tightening is always
   allowed.
7. **Never modify `/vital/`.** It must stay byte-identical to upstream
   (CI runs `migration/scripts/verify_upstream.sh`). Build glue, configs
   and instrumentation go in `/migration/reference/`. If you need to
   probe internal state, subclass or wrap from the harness, as
   `ref-render` subclasses `HeadlessSynth`.
8. **No build outputs, large binaries, secrets or env files in git.**
   Renders (`*.wav`, `*.npy`), `target/`, `build/` and plugin bundles are
   git-ignored. Check `git status` before every commit.

## How to port a piece of the engine

1. Pick the next item from the current milestone in PLAN.md. Find the
   upstream files listed in the target crate's `UPSTREAM_SOURCES`.
2. Get reference data before writing Rust:
   - For a single processor, add or extend a `ref-module` registry entry
     and spec (`migration/reference/README.md`), add the case to
     `cases.json`, and capture with
     `run_corpus.py render --only <name>`.
   - For whole-patch behaviour, add a preset/MIDI case via `generate.py`
     and `cases.json`.
3. Port faithfully. Keep upstream's structure where practical, including
   processing order, control-rate vs audio-rate behaviour, sub-block
   splitting at 128 samples, smoothing, denormal handling and the 4-lane
   `poly_float` layout (voice 0 L/R, voice 1 L/R).
4. Compare with `audio-diff` using the case's tolerances, as a Rust test
   that calls the library and/or with `run_corpus.py compare`. Always use
   the same sample rate, block size, seed and length as the reference;
   the block size changes the output.
5. Record what you did, the measured metrics, and any open discrepancy in
   `LEDGER.md`.

## Reference-specific facts that affect parity

- The reference is built with `-O3 -ffast-math` like upstream release
  builds, so bit-exact parity with Rust (no fast-math) is not expected.
  Tolerances cover this. Record a `REF_FAST_MATH=OFF` comparison when it
  helps to tell fast-math noise apart from real bugs.
- Engine randomness is a `std::mt19937` stream through libstdc++'s
  `uniform_real_distribution<float>`, seeded from a global counter in
  construction order. Exact parity for random features needs an exact
  replica. Otherwise compare those cases only with phase-blind metrics
  (`rms_diff_db`, `lsd_db`), and record that choice in the ledger.
- Presets missing controls load parameter defaults. Presets missing
  `wavetables` or `lfos` keep the init state. Presets missing `sample`
  fail to load. The port's loader must match (M2).
- On Linux, `FourierTransform` uses JUCE's built-in FFT in the reference.
  On macOS, upstream uses Accelerate (vDSP). Wavetable renders can differ
  in the last bits between the two.

## Working conventions

- Small PRs, one milestone item each, CI green before review. CI runs
  rustfmt, clippy with `-D warnings`, workspace build and tests, the
  reference build, a corpus render with a bit-exact determinism check,
  the upstream-copy check, the name check and the corpus freshness check.
- Rust toolchain is pinned in `/rust-toolchain.toml`. The C++ reference
  builds with GCC via `migration/scripts/build_reference.sh`.
- Crates live in `port/crates/<name>` and are published as `sas-<name>`.
  Keep `UPSTREAM_SOURCES` in each crate's `lib.rs` up to date; a test
  checks that every listed path exists.
- Add a dated entry to the end of `LEDGER.md` for every work session:
  what changed, the metrics measured, the decisions made, and what is
  blocked.
- Do not open issues or PRs in upstream projects (the synth, JUCE,
  nice-plug) without a human.
