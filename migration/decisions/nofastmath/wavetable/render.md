# No-fast-math exact cases: wavetable/render

Upstream takes interpolated phases from `arg(conj(from)·to)` of noise-level
FFT bins here, so the official reference misses the module limits against
its own no-fast-math build on x86 (−27 to −62 dB, and 2.6e-3 for folding).
`audio_fade_3_phase_1` and `audio_fade_3_phase_2` are well-conditioned once
the source formulas are fixed, so they stay on the official reference.

- `wavetable/render/wave_linear_freq_disjoint_spectra` (SAS-35; x86): frequency-mode linear interpolation between disjoint spectra
- `wavetable/render/wave_cubic_freq_disjoint_spectra` (SAS-35; x86): frequency-mode cubic interpolation between disjoint spectra
- `wavetable/render/multi_group_disjoint_spectra` (SAS-35; x86): a multi-group table containing it
- `wavetable/render/wave_folder_at_clamp` (SAS-35; x86): wave folder at ±1
- `wavetable/render/audio_fade_3_phase_0` (SAS-35; x86): frequency-interpolated audio fade, phase style 0
