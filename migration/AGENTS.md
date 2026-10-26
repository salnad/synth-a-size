# Rules for agents working on the port

Read this file before changing anything in this repository. These rules
take precedence over convenience. If a task seems to need breaking one,
stop and ask a human.

## The project

synth-a-size is a Rust port of the open-source upstream wavetable synth
vendored in `/vital`, shipped as a nice-plug VST3/CLAP plugin. After the
port it gains an LLM control layer. The plan is in [PLAN.md](PLAN.md),
the work log in [ledger/](ledger/README.md) (one file per entry; history
before 2026-10-21 in the frozen [LEDGER.md](LEDGER.md)), every intentional
behavioural difference from upstream in [DIVERGENCES.md](DIVERGENCES.md), and
the upstream pin in [UPSTREAM.md](UPSTREAM.md).

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
   - A port file starts with the credit lines, then a blank line, then its
     module docs:

     ```
     // Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
     // Portions Copyright (c) 2020 Raw Material Software Limited (JUCE 6: juce_core, ISC).
     // SPDX-License-Identifier: GPL-3.0-or-later
     ```

   - The first line goes on any file translated from `/vital/src`,
     including tables copied from it. The `Portions` line goes on any file
     that reproduces a JUCE function's logic (from
     `vital/third_party/JUCE/modules`), naming each module and the licence
     its file header states: `ISC` for `juce_core`, `juce_audio_basics`
     and the other ISC modules, `GPL-3.0 option` for `juce_dsp`,
     `juce_graphics`, `juce_gui_basics` and the rest. Drawing the same
     result another way (egui instead of `Graphics`), or matching one
     observable behaviour, isn't a translation.
   - A newly translated third-party library, or a JUCE module not yet
     listed, also goes in the bundle's
     `port/crates/plugin/notices/THIRD_PARTY_NOTICES.md`. That file is user
     visible, so it names neither upstream nor its author (rule 2).
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
   authored in `migration/corpus/generate.py`, or as scripted LLM tool
   sessions in `migration/corpus/llm/sessions/`, whose presets the tools
   generate (`migration/corpus/README.md`).
4. **Drop Firebase / auth.** No login, account, Firebase, telemetry,
   license-check or "expired build" code in the port. Port engine
   behaviour as upstream builds it with `NO_AUTH=1`.
5. **Parity first, then optimise.** Until M11 the goal is audio parity
   with the reference. Write the straightforward translation of the
   upstream algorithm. No SIMD tricks, approximations, lookup changes,
   reordered arithmetic or "cleanups" that change numerics unless parity
   tests prove they do not. Performance work starts at M11, and only
   against a passing parity suite.
   - **Intentional divergences are recorded.** Sometimes the port behaves
     differently from upstream on purpose, for example where upstream is
     undefined behaviour, where it has an out-of-range bug, or where the
     host framework forces it.
   - Any PR that introduces such a divergence adds an entry to
     [DIVERGENCES.md](DIVERGENCES.md) giving what differs, upstream's
     behaviour, the port's behaviour, why, and the approving decision or
     PR. Reviewers check for it: they look in the PR for a new `panic!` or
     error on an input upstream handles, a `None` or clamp for upstream UB,
     a formula that differs from the source, or a `#[cfg]` fork with no
     matching A-site.
   - Ideas that go beyond parity (speed, features, nicer handling of bad
     input) go in [IMPROVEMENTS.md](IMPROVEMENTS.md) as parked ideas, not
     in code.
   - Per-target emulation that matches upstream on each target is not a
     divergence. Harness comparison policies go under its "Verification
     exceptions".
6. **Never loosen tests or tolerances to make them pass.** Do not raise
   a limit in `migration/corpus/cases.json`, an `audio-diff` invocation
   or a port test; do not delete, skip, `#[ignore]` or narrow a test; do
   not shorten a render; do not change a reference to match the port. A
   failing parity test means the port is wrong until shown otherwise. If
   you believe a tolerance is wrong, write the evidence in a ledger entry and
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

Every module thread follows the same loop. Each step adds files you own;
none of them edits a shared list.

1. **Pick the work.** Take the next item of your milestone (PLAN.md and
   the port plan). The upstream files are listed in your crate's
   `UPSTREAM_SOURCES`.

2. **Add a reference capture** before writing Rust.
   - *Processor* (only if `ref-module --list` does not already have it):
     create `migration/reference/src/modules/<area>/<module>.cpp` and
     register it with `REGISTER_REF_MODULE(refmod::ModuleDef{...})`. Give it
     a unique name, a factory, inputs mapped to the processor's input enum
     (name, index, default value, audio/control/trigger rate) and outputs.
     CMake picks the file up by glob; copy `src/modules/filters/dc_filter.cpp`
     as a template. `<area>` is your crate's name (`dsp-core`, `filters`,
     `effects`, `modulation`, `oscillators`, `wavetable`, `engine`).
   - *Cases*: create `migration/corpus/modules/<area>/<module>.json`:

     ```json
     {
       "description": "Digital SVF (SynthFilter model \"digital\").",
       "module": "filter",
       "params": { "model": "digital" },
       "cases": {
         "lowpass_saw_sweep": {
           "num_samples": 22050,
           "inputs": {
             "audio": { "signal": "saw", "freq_hz": 110.0, "amp": 0.5 },
             "reset": { "trigger": [ { "sample": 0, "value": "voice_on" } ] },
             "midi_cutoff": { "signal": "ramp", "from": 30.0, "to": 110.0 },
             "resonance": 0.7
           }
         }
       }
     }
     ```

     Settings layer as `cases.json` `module_defaults`, then the file's
     top-level keys, then the case's keys. Signal and trigger syntax is in
     `migration/reference/README.md`. The case id is
     `<area>/<module>/<case>`, e.g. `filters/digital_svf/lowpass_saw_sweep`.
     Area (directory), module (file name) and case names may only use
     `[A-Za-z0-9_-]`; anything else is rejected at load.
   - *Tolerances* default to `module_defaults` (`max_abs_error` 1e-4,
     `residual_db` −80 dB). That is the approved floor, which is also pinned
     in code in `run_corpus.py` and `sas-parity`. A case may make its
     tolerances stricter. Anything looser is rejected unless the *case*
     (never the file) carries a structured exception:

     ```json
     "tolerances": { "residual_db": -70 },
     "tolerance_exception": {
       "approved_in": "2026-10-21: approve looser filters/digital_svf/lowpass_saw_sweep",
       "tolerances": { "residual_db": -70 }
     }
     ```

     `approved_in` must be the exact text of a dated ledger heading
     (`YYYY-MM-DD: ...`, in an entry
     file under `migration/ledger/`, or the frozen `LEDGER.md`), and that
     section must name the case id. Every loosened metric must equal its
     approved value, so loosening further needs a new approval. Only a human
     can approve (rule 6).
   - *Large-scale quantities* (Hz, samples, ms): cover the input's full
     working range (the upstream parameter ranges that feed it). Never
     narrow a case's input domain to fit the absolute gate; that is
     narrowing a test (rule 6). Where the reference's own fast-math noise
     on the raw value exceeds `max_abs_error`, capture the value in the
     normalized form its upstream consumer uses, computed the same way in
     the ref-module and the parity test. For example, a keytrack frequency
     is captured as SynthLfo's phase increment `frequency * (1.0f /
     getSampleRate())` (see `op_tempo_chooser_phase_increment`). Keep a raw
     case over the band that passes, and record the raw full-range numbers
     in your ledger entry. If no faithful normalized form exists, record it and
     ask.
   - *Exact-match artifacts* (JSON or text, e.g. a WavetableCreator load →
     save): give the module an artifact hook (the sixth `ModuleDef` field;
     `create` may be `nullptr` for artifact-only modules). Call
     `sink.text("saved.json", json)` for each artifact and, optionally,
     `sink.array(name, data, channels)` for float arrays.
     `context.corpusPath(...)` resolves input files referenced by `params`.
     See `src/modules/harness/echo.cpp`. Artifacts are written to
     `artifact_<name>`, hashed like buffers, and compared byte for byte.
     Numeric data (samples, wave frames, coefficients, parameter values)
     goes through `sink.array`, so the case tolerances apply. A text
     artifact must be upstream's own serialization of the state (for
     example `WavetableCreator::stateToJson().dump()`), never a print you
     formatted yourself. A reduced-precision print such as `%.3f` would
     hide a tolerance inside an "exact" comparison.
   - *JSON state* (presets, wavetable creator JSON): declare the artifact as
     JSON so it is compared structurally, with floats held to the
     owner-approved bound of 4 f32 ulp or 1e-6 absolute, whichever is
     looser. Name the keys whose strings are base64 little-endian f32 data;
     they are decoded and compared element-wise. Only the keys pinned in
     code (`wave_data`) can be declared. Other strings, including PCM
     base64, stay exact:

     ```json
     "artifacts": { "saved.json": "json" },
     "base64_f32_keys": ["wave_data"]
     ```

     This applies in sas-parity and in every cross-build `run_corpus.py
     compare`, including calibration. Determinism compares (`--exact`) and
     hashes stay byte-exact. `"json_bound"` may only tighten the bound.
     Details are in `decisions/owner-decisions-json-and-morphs.md`.
   - *Content where the reference amplifies fast-math noise* (owner
     decision SAS-35), which the official reference cannot meet against its
     own no-fast-math build, may use `"reference": "nofastmath"`. The case
     is then compared bit for bit against the `REF_FAST_MATH=OFF` captures.
     Two conditions apply. First, its id must be listed, citing `SAS-35` and
     the targets where the official reference fails (`x86`, `macos` or
     both), in `decisions/nofastmath/<area>/<module>.md`, your module's own
     file. Second, each declared target's CI calibration step must show the
     official reference failing the case's normal limits against that
     target's no-fast-math build (`compare --justify-nofastmath`). Such
     cases cannot set tolerances. The steps are in
     `decisions/nofastmath/README.md`.
     Their no-fast-math hashes come from the `parity-captures-nofastmath`
     artifact:
     `run_corpus.py hashes <tree> --write --reference nofastmath`.
   - *Render and record hashes*:

     ```bash
     migration/scripts/build_reference.sh
     python3 migration/scripts/run_corpus.py render --out build/parity --only filters/digital_svf
     ```

     Expected hashes must come from a tree rendered with the CI build
     identity. The canonical source is your PR's CI run: download its
     `reference-renders` artifact (which also carries every module capture)
     and write from it:

     ```bash
     gh run download <run-id> -n reference-renders -D build/ci
     python3 migration/scripts/run_corpus.py hashes build/ci/ref --write
     ```

     A local tree works only if its build identity is identical (Ubuntu
     24.04, GCC 13.3, glibc 2.39, same flags). `hashes --write` refuses
     identities that have no committed hashes; `--new-identity` is reserved
     for recorded decisions such as an ARM reference or a runner image bump.
     The command writes `migration/corpus/modules/<area>/<module>.hashes.json`.
     Commit it with the case file: CI fails if a module's hash file lacks
     the CI identity, and `sas-parity` checks every loaded capture against
     these hashes.

     Every hash file also needs the **macOS arm64 identity** (the shipping
     target; see `decisions/reference-arm.md`, "Mac CI policy"). macOS does
     not gate merges. The advisory `Reference ARM (PR)` job renders your
     PR's cases on Apple Silicon and runs `sas-parity` natively for the
     crates you touch:
     - a changed macOS output fails it;
     - a new case without a macOS hash only warns, and the job writes the
       missing entries (new keys only) as the `macos-hashes` artifact.

     Commit them in the same PR:

     ```bash
     migration/scripts/macos_hashes.sh <run-id>
     ```

     On main, the nightly full suite treats a missing macOS hash as an error
     and opens the issue "Mac full suite failing on main". A case that
     passes on x86 but fails the macOS no-fast-math proxy is re-enabled
     under `reference: nofastmath` with a `macos` allowlist target. The PR
     job then runs the macOS justification as evidence.

3. **Port faithfully.** Keep upstream's structure where practical,
   including processing order, control-rate vs audio-rate behaviour,
   sub-block splitting at 128 samples, smoothing, denormal handling and the
   4-lane `poly_float` layout (voice 0 L/R, voice 1 L/R). Use exact
   division and sqrt.

4. **Add a parity test** in your crate. Add a dev-dependency on
   `sas-parity` (`sas-parity.workspace = true` under `[dev-dependencies]`),
   then:

   ```rust
   use sas_parity::{assert_parity, ModuleCase};

   #[test]
   fn digital_svf_lowpass_saw_sweep() {
       let case = ModuleCase::load("filters/digital_svf/lowpass_saw_sweep");
       // Drive your port with exactly what the reference saw.
       let audio = case.input("audio");             // Vec<[f32; 4]>, one per frame
       let cutoff = case.input("midi_cutoff");
       let reset = case.triggers("reset");          // Vec<Trigger { sample, value, lanes }>
       let output: Vec<[f32; 4]> = run_port(&case, &audio, &cutoff, &reset);
       assert_parity!("filters/digital_svf/lowpass_saw_sweep", output);
   }
   ```

   - Process in blocks of `case.block_size()` at
     `case.internal_sample_rate()` for `case.num_samples()` frames, with
     `case.params()` for constructor parameters.
   - For a module with several outputs, name the output:
     `assert_parity!("modulation/envelope/adsr:value", value)`.
   - Full-patch renders use `RenderCase::load("render/<name>")` (preset,
     MIDI and settings) and `assert_parity!("render/<name>", stereo)`.
   - Exact-match artifacts:
     `assert_artifact!("wavetable/creator/init", "saved.json", json)` or
     `case.assert_artifact(name, &bytes)`. These compare byte for byte, with
     no tolerance and no normalisation, unless the case declares the
     artifact `"json"` (JSON bound above); the mode always comes from the
     case file. `case.artifact(name)` returns the reference text.
   - A `"reference": "nofastmath"` case uses the same `assert_parity!` /
     `assert_artifact!` calls. sas-parity reads its captures from
     `SAS_PARITY_CAPTURES_NOFASTMATH` (default `build/parity-nofastmath`;
     rendered on demand from `migration/reference/build-nofastmath`) and
     requires bit equality.
   - Tolerances come only from the case files. Tests cannot pass, scale or
     override them. The hidden `report()` / `parity_report()` helpers are
     for interactive debugging only. A parity test must call
     `assert_parity!` / `assert_artifact!`. Never `#[ignore]` it, and never
     compare against anything but the reference capture.
   - On failure the panic lists every metric. The port output and the
     report are written to `build/parity-failures/<case>/`; in CI they are
     uploaded as the `parity-failures` artifact.

5. **Run locally.** `cargo test -p <your-crate>` (from `port/`) finds the
   captures in `build/parity`, or wherever `SAS_PARITY_CAPTURES` points.
   It renders missing or stale ones on demand, building the reference
   first if needed (Linux x86_64, GCC, CMake). `SAS_PARITY_NO_BUILD=1`
   turns that into a hard error, as in CI. `SAS_PARITY_REFRESH=1`
   re-renders after you change C++ module code. To compare whole trees:
   `python3 migration/scripts/run_corpus.py compare build/ref build/port`.

6. **CI.** The `reference-harness` job builds the reference once, renders
   every case and uploads `parity-captures`. The `parity` job downloads
   it and runs `cargo test --workspace`, so a new parity test runs as soon
   as it exists. No CI edits are needed.

7. **Record** what you did, the measured metrics (see the report) and any
   open discrepancy in a new ledger entry,
   `migration/ledger/YYYY-MM-DD-<slug>.md` (format in
   `migration/ledger/README.md`). Never edit `LEDGER.md`: it is frozen, and
   CI rejects changes to it.

## Reference-specific facts that affect parity

- The reference is built with `-O3 -ffast-math` like upstream release
  builds (minus approximate reciprocals and libmvec), so bit-exact parity
  with Rust (no fast-math) is not expected. A `REF_FAST_MATH=OFF` build is
  the proxy for "correct port, different rounding": the determinism
  decision record has its per-case noise, and CI checks that this proxy
  passes the tolerances. Use it to tell float-rounding noise apart from
  real bugs.
- Reference output is bit-identical across x86_64 CPUs for the same build
  identity (`migration/decisions/reference-determinism.md`). CI enforces
  this with committed hashes. If the hash check fails while the identity
  matches, a CPU-dependent path has crept in: investigate it, and do not
  just re-baseline the hashes. Re-baseline (`run_corpus.py hashes <tree>
  --write`) only for intentional reference changes, and say why in the
  ledger.
- Never reintroduce CPU-dependent floating point in the reference or the
  port: approximate reciprocal/rsqrt instructions, vendor SIMD math
  libraries (libmvec, SVML), or FMA in one build but not another.
  `check_fp_determinism.sh` guards the reference.
- Upstream computes different results on arm64 and x86 at a few
  enumerated sites (`utils::toInt` rounding vs truncation, undefined
  float→int conversions in random phase). The shipped plugin is arm64
  macOS, so its parity is checked against the macOS arm64 reference.
  Mirror those sites per target, exactly as listed in
  `decisions/reference-arm.md`, and keep everything else ISA-independent.
- Engine randomness is a `std::mt19937` stream through the C++ library's
  `uniform_real_distribution<float>`, seeded from a global counter in
  construction order. That library is libstdc++ in the x86 reference and
  libc++ in the macOS arm64 reference. The two agree except at the
  `generate_canonical` round-to-1.0 edge, about 3e-8 per draw, where
  libc++ returns exactly `max` (A9 in `decisions/reference-arm.md`,
  mirrored per target in `sas-common`). Exact parity for random features
  needs an exact replica, including A9. Otherwise compare those cases only
  with phase-blind metrics (`rms_diff_db`, `lsd_db`), and record that
  choice in the ledger.
- Presets missing controls load parameter defaults. Presets missing
  `wavetables` or `lfos` keep the init state. Presets missing `sample`
  fail to load. The port's loader must match (M2).
- On Linux, `FourierTransform` uses JUCE's built-in FFT in the reference.
  On macOS, upstream uses Accelerate (vDSP). Wavetable renders can differ
  in the last bits between the two.

## Working conventions

- Small PRs, one milestone item each, CI green before review. CI runs
  rustfmt, clippy with `-D warnings`, workspace build and tests, the
  reference build with the CPU-dependent-FP guard, a corpus render with a
  bit-exact repeat check and a committed-hash cross-machine check, the
  tolerance-calibration proxy, `cargo test --workspace` against the
  reference captures (parity), the upstream-copy check, the name check
  and the corpus freshness check.
- Rust toolchain is pinned in `/rust-toolchain.toml`. The C++ reference
  builds with GCC via `migration/scripts/build_reference.sh`.
- Crates live in `port/crates/<name>` and are published as `sas-<name>`.
  Keep `UPSTREAM_SOURCES` in each crate's `lib.rs` up to date; a test
  checks that every listed path exists.
- Add a file under `migration/ledger/` (`YYYY-MM-DD-<slug>.md`) for every
  PR or work session: what changed, the metrics measured, the decisions
  made, and what is blocked. One file per entry means concurrent PRs never
  conflict. `python3 migration/scripts/ledger.py` prints the whole log in
  date order.
- Do not open issues or PRs in upstream projects (the synth, JUCE,
  nice-plug) without a human.
