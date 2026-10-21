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

**Normalized pitch coverage:** `ref-module` module
`oscillator_module_pitch` (same file as `oscillator_module`, with the
same private-member accessor as `synth_oscillator_pitch`) and
`migration/corpus/modules/oscillators/oscillator_module_pitch.json`
(9 cases) capture the module's oscillator's phase increment × 2^-32
(frequency / sample rate). That gives the `osc_1_transpose`, `osc_1_tune`,
`osc_1_transpose_quantize` and `osc_1_midi_track` controls active
coverage through the registry scaling and the audio-rate modulation
sums. The cases are the three parked cases' inputs plus negative,
maximal and global-snapped transposes, no MIDI tracking, single voice and
48 kHz. Measured: worst `transpose_max_tune_max` 1.3e-7 / −123.5 dB; the
official reference passes against no-fast-math on all nine. Unison
detune is a separate per-voice multiplier, not part of this capture: the
large scaled detune of `detune_power_range` stays covered only by
`interim_detune_power_range` (detune 0.5).

**Interim cases** (named `interim_*`; the parked cases stay unchanged):
`interim_transpose_quantize` (the snapping control at transpose 0,
2.4e-7 / −142.5 dB) and `interim_detune_power_range` (detune power −2.5
and range 24 with detune 0.5 over 3000 samples, 3.6e-7 / −137.9 dB).

**Review fixes (Graph Reviewer, changes requested at `42886d1`):**
the filter-on switches and the sampler's on→off transition were claimed
but never exercised.
- The `producers_module` ref-module now takes `params.filter1_on` and
  `params.filter2_on`. Each is a `Value` plugged in through
  `setFilter1On`/`setFilter2On`, as the engine plugs `filter_N_on`.
  Without them both filters count as on, as before.
- It also takes `params.control_changes` (`[{sample, name, value}]`),
  applied before the first block that starts at or after that
  internal-rate frame, and adds an output `sample_phase` (the sampler's
  phase output after each block).
- The Rust test mirrors this with a probe around the `ProducersModule`
  kernel. `ProducersModule::new`, `submodules` and `add_submodules` split
  the constructor so the probe can wrap it.
- 10 new cases (26 in total):
  - filter 1 off, filter 2 off and both off, across filter 1, filter 2
    and dual routing (`filter1_off`, `filter2_off`, `filters_off`,
    `dual_filter1_off`, `dual_filter2_off`);
  - both filters plugged on (`filters_on_plugged`);
  - both filters switched off mid-run (`filters_switch_off_mid_run`);
  - `sample_on` switched off mid-run (`sample_off_mid_run` with FM from
    the sample, and `sample_off_block_size_17`), and off then back on
    (`sample_off_on`, with RM from the sample).
- Mutations, each now caught:
  - forcing `filter1_on` or `filter2_on` to true (`raw_out` of the off
    cases);
  - dropping the `RAW` clear (the FM/RM-from-sample oscillator's
    output);
  - dropping the `LEVELLED` clear (the routed sample);
  - dropping the phase clear (`sample_phase`).
- Measured: every new output ≤ 3.6e-7 / ≤ −138.7 dB. The official
  reference passes against no-fast-math on all 26 producers cases (130
  outputs).
- `producers_module.hashes.json` gains the new cases' hashes and every
  case's `sample_phase` hash (CI identity). Existing hashes are
  unchanged.
