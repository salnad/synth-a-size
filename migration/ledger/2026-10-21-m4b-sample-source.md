## 2026-10-21: M4b `Sample` and `SampleSource` (sas-oscillators, PR C of the M4b stack)

**Added**

- `port/crates/oscillators/src/sample_source.rs`: `Sample` (default
  seeded white noise, mono/stereo `loadSample`, the band-limited octave
  copies with upstream's padding and loop wrap, `stateToJson` /
  `jsonToState` with JUCE base64 PCM) and `SampleSource` (keytrack,
  transpose snapping, tune, loop, one-shot, bounce, random start, reset
  offsets, level/pan and the encoded phase output). Cloning draws a new
  seed and shares the sample and phase output, as upstream's copy does.
  `getActiveIndex`'s float→int cast goes through `float_to_i32` (A3).
- `ref-module` module `sample_source`
  (`migration/reference/src/modules/oscillators/sample_source.cpp`):
  `params.sample` loads PCM wave recipes through `Sample::jsonToState`;
  output `phase` is the player's phase output. Parity test:
  `port/crates/oscillators/tests/sample_source_parity.rs`.
- `migration/corpus/modules/oscillators/sample_source.json`: 27 cases
  under `module_defaults`, five of them interim (below).

**Measured** (x86_64, 81 outputs, default module limits, no tolerance
changes)

- Worst: `saw_oversample_2` 8.3e-5 / −101.5 dB, `mix_22050` 2.5e-5 /
  −99.9 dB. The `raw` output of ten cases is bit-exact (default noise,
  every loop/bounce/one-shot variant of the saw, short sample, level ramp,
  note count). The `phase` outputs are within 2.4e-7.
- Against the `REF_FAST_MATH=OFF` reference: `raw` is bit-exact on all
  32 authored cases, the parked ones included.

**Parked pending an owner decision** on the noisy-reference proposal
(SAS-35; exact no-fast-math comparison is an owner decision only for the
wavetable-morph ids in owner-decisions-json-and-morphs.md). The official
reference fails the module limits against its own no-fast-math build;
`migration/corpus/modules/oscillators/pending/sample_source.json`, not
loaded):

| Case | Official vs no-fast-math (= port vs official) |
|---|---|
| `oscillators/sample_source/noise_transpose_40` | 7.9e-4 / −61.9 dB |
| `oscillators/sample_source/noise_transpose_7` | 8.8e-4 / −73.1 dB |
| `oscillators/sample_source/noise_transpose_19` | 5.6e-4 / −74.4 dB |
| `oscillators/sample_source/noise_keytrack_off` | 5.3e-4 / −76.1 dB |
| `oscillators/sample_source/noise_transpose_m13` | 2.5e-4 / −82.7 dB |
| `oscillators/sample_source/long_96k_at_48k` | 1.1e-3 / −92.3 dB |
| `oscillators/sample_source/saw_random_phase` | 1.6e-4 / −98.5 dB |
| `oscillators/sample_source/saw_at_96k` | 1.4e-4 / −99.7 dB |
| `oscillators/sample_source/block_size_17` | 1.3e-4 / −99.8 dB |
| `oscillators/sample_source/stereo` | 1.2e-4 / −108.2 dB |

Mechanism: upstream's `centsToRatio` is `powf(2.0f, cents /
kCentsPerOctave)`. Fast-math turns the division into a reciprocal
multiply, so any playback ratio other than exactly 2 is off by an ulp,
and the sample position integrates it (white noise turns that into the
largest errors). The port uses the source division and equals the
no-fast-math reference bit for bit. They wait for that decision, like the PR A pitch sweeps. The `phase`
output (sample position / length, the normalized form the sample-phase
modulation source reads) stays within the limits on all ten.

**Interim cases** (named `interim_*`, playback ratio exactly 2; the
parked cases stay unchanged): `interim_keytrack_off` (keytrack off at
transpose 0), `interim_stereo` (the stereo sample at 44.1 kHz with the pan
ramp), `interim_long_sample` (20000 samples at 44.1 kHz),
`interim_sample_rate_96000` (a 96 kHz sample at 96 kHz) and
`interim_block_size_17`. `raw` is bit-exact on all five.

**Hashes:** written from this PR's CI `reference-renders` artifact; macOS
arm64 hashes follow once PR #2 merges.
