## 2026-10-21: M4b SAS-35 re-enable and `sample_source_pitch` (sas-oscillators)

**Agent:** Oscillators. **Branch:** `cursor/m4b-sas35-reenable-7d24`.

### Re-enabled under SAS-35

The 22 oscillator cases parked by the M4b PRs (#28, #42, #47), including
the 5 constant-pitch probes, and the 3 macOS-only
`distortion_{1,2,3}_spread` cases parked by #2, are back in their module
files unchanged, with `"reference": "nofastmath"`. They are listed in
`migration/decisions/nofastmath/oscillators/{synth_oscillator,sample_source,oscillator_module}.md`.
Every `pending/` file under `modules/oscillators` is deleted. None of these
cases renders a wavetable with a Phase Shift component (A11).

Targets come from `compare --justify-nofastmath` on x86 (every case
declared `x86` prints `justified on x86`) and from the `Reference ARM`
macOS 15 job for `macos`. The table gives official against no-fast-math,
worst output (max_abs_error / residual):

| Case | Targets | x86 | macOS |
|---|---|---|---|
| `oscillators/synth_oscillator/probe_transpose_tune_constant` | x86, macos | 4.8e-03 / -66.5 dB | MACOS_TBD |
| `oscillators/synth_oscillator/probe_transpose_constant_negative` | x86, macos | 1.6e-03 / -81.5 dB | MACOS_TBD |
| `oscillators/synth_oscillator/probe_quantize_local_transpose` | x86, macos | 5.4e-03 / -66.0 dB | MACOS_TBD |
| `oscillators/synth_oscillator/probe_quantize_global_transpose` | x86, macos | 3.4e-03 / -73.6 dB | MACOS_TBD |
| `oscillators/synth_oscillator/probe_quantize_global_high_note` | x86, macos | 3.5e-03 / -58.4 dB | MACOS_TBD |
| `oscillators/synth_oscillator/note_ramp` | x86, macos | 1.1e-03 / -72.4 dB | MACOS_TBD |
| `oscillators/synth_oscillator/transpose_quantize_local` | x86, macos | 7.3e-04 / -80.9 dB | MACOS_TBD |
| `oscillators/synth_oscillator/transpose_quantize_global` | x86, macos | 2.5e-04 / -93.2 dB | MACOS_TBD |
| `oscillators/synth_oscillator/transpose_tune_audio` | x86, macos | 5.8e-04 / -82.5 dB | MACOS_TBD |
| `oscillators/synth_oscillator/distortion_1_spread` | macos | passes (not gated) | 2.2e-3 (#2) |
| `oscillators/synth_oscillator/distortion_2_spread` | macos | passes (not gated) | 1.5e-3 (#2) |
| `oscillators/synth_oscillator/distortion_3_spread` | macos | passes (not gated) | 8.8e-4 (#2) |
| `oscillators/sample_source/noise_transpose_m13` | x86, macos | 2.5e-04 / -82.7 dB | MACOS_TBD |
| `oscillators/sample_source/noise_transpose_7` | x86, macos | 8.8e-04 / -73.1 dB | MACOS_TBD |
| `oscillators/sample_source/noise_transpose_19` | x86, macos | 5.6e-04 / -74.4 dB | MACOS_TBD |
| `oscillators/sample_source/noise_transpose_40` | x86, macos | 7.9e-04 / -61.9 dB | MACOS_TBD |
| `oscillators/sample_source/noise_keytrack_off` | x86, macos | 5.3e-04 / -76.1 dB | MACOS_TBD |
| `oscillators/sample_source/saw_random_phase` | x86, macos | 1.6e-04 / -98.5 dB | MACOS_TBD |
| `oscillators/sample_source/stereo` | x86, macos | 1.2e-04 / -102.6 dB | MACOS_TBD |
| `oscillators/sample_source/long_96k_at_48k` | x86, macos | 1.1e-03 / -92.3 dB | MACOS_TBD |
| `oscillators/sample_source/saw_at_96k` | x86, macos | 1.4e-04 / -99.7 dB | MACOS_TBD |
| `oscillators/sample_source/block_size_17` | x86, macos | 1.3e-04 / -99.8 dB | MACOS_TBD |
| `oscillators/oscillator_module/transpose_tune` | x86, macos | 2.3e-03 / -72.5 dB | MACOS_TBD |
| `oscillators/oscillator_module/transpose_tune_unsnapped` | x86, macos | 2.3e-03 / -72.5 dB | MACOS_TBD |
| `oscillators/oscillator_module/detune_power_range` | x86, macos | 1.0e-03 / -81.5 dB | MACOS_TBD |

**Port:** bit-exact against the no-fast-math build on all 25 cases on x86
(every output 0.000e0). MACOS_PORT_TBD

**Hashes:** official and no-fast-math entries for both targets in
`synth_oscillator`, `sample_source` and `oscillator_module`; the macOS
entries come from this PR's `ref-arm64-macos-15` artifact.

### `sample_source_pitch`

Addresses Graph Reviewer's non-blocking note on #42: no active
`sample_source` case used a sample-rate ratio that is not a power of two.
The module stays on the official reference.

**Module:** `ref-module` module `sample_source_pitch` (same file as
`sample_source`, private `SampleSource::phase_inc_` read through an
explicit-instantiation member pointer like `synth_oscillator_pitch`)
captures the block's playback increment divided by `upsampleLength()`:
the increment in sample lengths per output sample, normalized like the
phase output. `activeLength()` can't be used after `process()`, because
`process()` ends with `markUnused()`, which nulls the active data.
`SampleSource::phase_inc()` exposes the Rust value.

**Cases:** `migration/corpus/modules/oscillators/sample_source_pitch.json`
(18): the inputs of the ten re-enabled `sample_source` cases, plus 22.05 kHz
at 48 kHz, 44.1 kHz at 48 kHz, 96 kHz at 44.1 kHz, 48 kHz at 96 kHz
(oversample 2), 11.025 kHz keytracked, global transpose snapping and
transpose/tune at ±96/±1. Measured: worst `noise_transpose_40` 4.4e-11 /
−134.4 dB, most cases exact. Values are 3e-5 to 0.09, so the −80 dB
residual gate, not the absolute one, is the effective check.
Official hashes for both targets in `sample_source_pitch.hashes.json`.
