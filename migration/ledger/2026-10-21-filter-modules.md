## 2026-10-21: M5a, `FilterModule` and `FiltersModule`

**Changed.** These complete the core filter milestone.

- `port/crates/filters/src/modules/filter_module.rs` ports `vital/src/synthesis/modules/filter_module.{h,cpp}`:
  - every model is added; `setModel` enables one and hard-resets it through `getLocalProcessor` when it changes;
  - the keytrack `cr::Multiply` is internal modulation of the cutoff;
  - poly or mono mod controls follow `setMono`;
  - `on`/`mix` handling matches upstream, and `clone()` resets `last_model_`.
- `filters_module.rs` ports `filters_module.{h,cpp}`: parallel routing, serial 1→2 and serial 2→1, with the shared input `Output`s re-pointed per block.
- The parameter lookup in the tests is the `sas-preset` registry (dev-dependency).

**Reference.** New ref-modules `filter_module` and `filters_module` in `migration/reference/src/modules/filters/filter_modules.cpp`. They run in a SynthBase-style harness: the module's inputs are forwarded before `init()`, every control is set from its input once per block, and the output is copied. `tests/filter_modules.rs` mirrors the harness.

**Cases**, all under `module_defaults` with no tolerance changes:

- `filters/filter_module`, 29 cases:
  - two per model;
  - keytrack with MIDI, a mix ramp (−0.2..1.2), `on` toggling, model steps 0→7, formant controls and the vocal tract;
  - comb and phaser blend transpose, per-voice retriggers, 2× oversampling, 48 kHz at block 128, and mono controls (`params.mono`).
- `filters/filters_module`, 6 cases: parallel routing, serial in both directions, routing switching, a serial path with filter 1 off, and keytrack at 2×.

**No-fast-math case (SAS-35).** `filters/filter_module/comb_style1_resonance_drive` modulates the comb cutoff into long delay periods at resonance up to 1. This is the same mechanism as the `comb_filter` cases. Official reference against the same target's no-fast-math build:

| Target | max_abs_error | residual | Justification |
|---|---|---|---|
| x86 | 2.09e-4 | −84.4 dB | justified on x86 |
| macOS arm64 | 1.16e-4 | −87.0 dB | justified on macOS (run 39646624559) |

It is listed in `migration/decisions/nofastmath/filters/filter_module.md` as `x86, macos`. `allowlist-approval` reports 0 problems.

**Comb cases near the gate stay on the official reference.** Both pass on both targets:

| Case | x86 official vs no-fast-math | macOS official vs no-fast-math |
|---|---|---|
| `comb_cutoff_blend` | 9.17e-5 / −91.8 dB | 4.43e-5 / −94.5 dB |
| `comb_blend_transpose` | 8.07e-5 / −89.6 dB | 4.55e-5 / −92.2 dB |

**Parity.**
- x86: 35/35 pass. The no-fast-math case is bit-exact. The port is bit-identical to the no-fast-math build on all 35 cases, as Filter Reviewer confirmed.
- Official cases: worst `filter_module` is 9.2e-5 (`comb_cutoff_blend`), and worst `filters_module` is 1.7e-6 / −119.1 dB.
- macOS arm64: official vs no-fast-math, worst `filters_module` is 2.7e-6 / −116.5 dB. The Reference ARM Rust parity step on this PR's head checks the port on arm64.

**Hashes.** Both builds on both targets: x86 from local renders with the CI identity, and macOS from the `ref-arm64-macos-15` artifact of run 39646624559.

**Not exercised.** The harness runs one module without voice clones. So the `getLocalProcessor` mapping, `clone()` with `last_model_ = -1` and `hardReset` resetting the original processors are reviewed but not run. Voice Engine's full-handler captures will cover them.
