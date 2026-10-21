## 2026-10-21: M6 PR 6, EQ and distortion module (`sas-effects`, on `DigitalSvf`)

Ported `modules/equalizer_module.{h,cpp}` (three bands of `DigitalSvf`: low
shelf or high pass, band shelf or notch, high shelf or low pass, plus the
output memory the display reads) and `modules/distortion_module.{h,cpp}`
(`Distortion` with an optional basic `DigitalSvf` before or after it, and a
dry/wet mix). Upstream quirk kept: `EqualizerModule::setSampleRate` reaches
only the six filters, not the module's own controls.

The effect tests' parameter table (`tests/common/params.rs`) had the three
`eq_*_resonance` parameters as linear; upstream scales them quadratically.
A new test, `tests/params_table.rs`, checks every row against
`vital/src/common/synth_parameters.cpp`. The other 54 rows, which the delay,
reverb, chorus, flanger and phaser captures use, already matched.

Captures: `effects/equalizer_module` (10 cases: every gain −15→15, cutoff
8→136 and resonance 0→1 on all three bands, the pass and notch modes, mode
switching at 2–5 Hz, enable toggling, 48/96 kHz, 2× oversampling) and
`effects/distortion_module` (13 cases: every type with drive −30→30 dB,
downsample, filter after and before with cutoff, resonance and blend sweeps,
type and mix sweeps, enable toggling, 96 kHz, 2× oversampling). All under
`module_defaults`.

Metrics (x86_64): EQ 10 outputs pass, worst 4.4e-5 / −99.9 dB
(`pass_and_notch_modes`); distortion module 12 outputs pass, 4
bit-identical, worst 3.0e-5 / −109.4 dB (`rate_96k`). The port is
bit-identical to the `REF_FAST_MATH=OFF` build on all 23 outputs.

Parked under the coordinator's pending no-fast-math proposal (awaiting the
owner): `effects/distortion_module/filter_before`
(proxy 2.7e-4 / −88.5 dB; a sweeping filter into the linear fold), in
`migration/corpus/modules/effects/pending/distortion_module.json`.

**Interim coverage** (cases named `interim_*`, reviewer request): the
filter-before order with a constant filter (linear fold and sine fold) and
a 0.1 s version of the parked sweep. All pass the reference and the proxy.
