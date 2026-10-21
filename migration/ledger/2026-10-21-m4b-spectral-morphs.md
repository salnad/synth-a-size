## 2026-10-21: M4b spectral morphs (sas-oscillators, PR B of the M4b stack)

**Added**

- `port/crates/oscillators/src/spectral_morph.rs`: the remaining morphs
  of `spectral_morph.h` (vocode and form scale through the even/odd vocode
  morph, harmonic and inharmonic scale, smear, random amplitudes, low and
  high pass, phase disperse, shepard tone, skew), wired into
  `SynthOscillator::setWaveBuffers`. Each writes exactly the regions
  upstream writes in the oscillator's persistent Fourier buffers; the
  inharmonic morph keeps its index table inside the same buffer, as
  upstream does.
- `migration/corpus/modules/oscillators/spectral_morph.json`: 37 cases of
  the `synth_oscillator` module (same reference capture and parity test
  as PR A): every morph at a fixed amount, ramped 0 → 1 and with unison
  spread; spectral unison off; vocode at 96 kHz (sample-rate-dependent
  formant ratio); low pass at 48 kHz; shepard tone with a 0 → 2 ramp and
  spread (wraps). Shepard cases use a shepard creator table; the rest use
  the time-interpolated table of PR A.

**Measured** (x86_64, 74 outputs, default module limits, no tolerance
changes)

- Worst: `spectral_morph_4_ramp` (inharmonic scale) 1.2e-5 / −117.3 dB,
  `spectral_morph_4_spread` 3.4e-6 / −119.3 dB. Every other morph case is
  ≤ 1.2e-6 and ≤ −128 dB.

**Hashes:** written from this PR's CI `reference-renders` artifact; macOS
arm64 hashes follow once PR #2 merges.
