# No-fast-math exact cases: modulation/random_lfo

The Lorenz style integrates a chaotic attractor. The official reference first
differs from its no-fast-math build by one ulp a few hundred frames in, and the
attractor grows that to full scale (residual about −6 dB). Low-frequency and
tempo-synced Lorenz stay on the official reference.

- `modulation/random_lfo/lorenz_mono_audio` (SAS-35; x86, macos): chaotic Lorenz attractor amplifies a one-ulp fast-math difference to full scale
- `modulation/random_lfo/lorenz_mono_control` (SAS-35; x86, macos): chaotic Lorenz attractor amplifies a one-ulp fast-math difference to full scale
- `modulation/random_lfo/lorenz_stereo_audio` (SAS-35; x86, macos): chaotic Lorenz attractor amplifies a one-ulp fast-math difference to full scale
- `modulation/random_lfo/lorenz_stereo_control` (SAS-35; x86, macos): chaotic Lorenz attractor amplifies a one-ulp fast-math difference to full scale
- `modulation/random_lfo/lorenz_frequency_sweep` (SAS-35; x86, macos): chaotic Lorenz attractor amplifies a one-ulp fast-math difference to full scale
- `modulation/random_lfo/lorenz_max_frequency` (SAS-35; x86, macos): chaotic Lorenz attractor amplifies a one-ulp fast-math difference to full scale
