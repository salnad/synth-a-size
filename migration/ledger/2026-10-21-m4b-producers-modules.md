## 2026-10-21: M4b oscillator, sample and producers modules (sas-oscillators, PR D of the M4b stack)

**Added**

- `oscillator_module.rs`, `sample_module.rs`, `producers_module.rs`:
  `OscillatorModule` (the `<prefix>_*` controls through
  `createBaseControl`/`createPolyModControl`, the on switch),
  `SampleModule` (the `sample_*` controls, phase output cleared when
  switched off) and `ProducersModule` (three oscillators and the sampler
  as disabled sub-modules, FM/RM cross-wiring, the dependency-ordered
  processing loop, and routing to filter 1/2, dual, effects or direct
  out with the filter-on switches). Clones keep pointing at the original's
  controls, as upstream's copies do.
- `ref-module` modules `oscillator_module` and `producers_module`
  (`migration/reference/src/modules/oscillators/`, sharing
  `module_content.h`): wavetables, sample and control values from the case
  params, applied after `init()` as a preset load does. Parity test:
  `port/crates/oscillators/tests/module_parity.rs`. Control details come
  from `sas-preset`'s registry (dev-dependency; PR #7 is temporarily
  merged into this branch until it lands).
- `migration/corpus/modules/oscillators/{oscillator_module,producers_module}.json`:
  12 + 16 cases under `module_defaults` (two interim, below). They include a modulation cycle
  (each oscillator modulated by the next), where upstream processes none
  of the three and the outputs stay silent.

**Measured** (x86_64, 88 outputs, default module limits, no tolerance
changes): worst `producers_module/fm_chain:to_filter1` 2.6e-5 /
−111.3 dB; every other output ≤ 7e-6 and ≤ −112.7 dB.

**Parked pending an owner decision** on the noisy-reference proposal
(SAS-35)
(`migration/corpus/modules/oscillators/pending/oscillator_module.json`,
not loaded; port vs no-fast-math bit-exact):

| Case | Official vs no-fast-math (= port vs official) |
|---|---|
| `oscillators/oscillator_module/transpose_tune` | 2.3e-3 / −72.5 dB |
| `oscillators/oscillator_module/transpose_tune_unsnapped` | 2.3e-3 / −72.5 dB |
| `oscillators/oscillator_module/detune_power_range` | 1.0e-3 / −81.8 dB |

A constant nonzero transpose or tune through the module's audio-rate
controls is enough: `midi − base_midi` is exactly 0 in the source
formula but not after fast-math reassociation, and the phase accumulator
integrates the resulting ulp of frequency. **This affects full patches
too** (Voice Engine): any oscillator with a transpose or tune set will
drift against the official reference the same way, while matching the no-fast-math reference.

**Interim cases** (named `interim_*`; the parked cases stay unchanged):
`interim_transpose_quantize` (the snapping control at transpose 0,
2.4e-7 / −142.5 dB) and `interim_detune_power_range` (detune power −2.5
and range 24 with detune 0.5 over 3000 samples, 3.6e-7 / −137.9 dB).
