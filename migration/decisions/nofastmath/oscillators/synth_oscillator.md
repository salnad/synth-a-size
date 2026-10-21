# No-fast-math exact cases: oscillators/synth_oscillator

A pitch offset that is exactly zero in the source formula (`midi - base_midi`,
or the snapped transpose) is a few ulp off under fast-math reassociation. The
32-bit phase accumulator integrates that error over the render, so the
official reference drifts away from its own no-fast-math build. The port
uses the source formulas and matches the no-fast-math build bit for bit. The
three distortion-spread cases miss only on macOS arm64 (max_abs_error over
the module limit, residual −86 to −91 dB); x86's official reference passes
them. The pitch itself is
compared on the official reference in `synth_oscillator_pitch`.

- `oscillators/synth_oscillator/probe_transpose_tune_constant` (SAS-35; x86, macos): constant transpose and tune: the phase accumulator integrates the fast-math rounding of the pitch offset
- `oscillators/synth_oscillator/probe_transpose_constant_negative` (SAS-35; x86, macos): constant negative transpose: the phase accumulator integrates the fast-math rounding of the pitch offset
- `oscillators/synth_oscillator/probe_quantize_local_transpose` (SAS-35; x86, macos): local transpose snapping with a non-zero transpose: the accumulator integrates the fast-math rounding of the snapped offset
- `oscillators/synth_oscillator/probe_quantize_global_transpose` (SAS-35; x86, macos): global transpose snapping with a non-zero transpose: the accumulator integrates the fast-math rounding of the snapped offset
- `oscillators/synth_oscillator/probe_quantize_global_high_note` (SAS-35; x86, macos): global snapping at a high note: the accumulator integrates the fast-math rounding of the snapped offset
- `oscillators/synth_oscillator/note_ramp` (SAS-35; x86, macos): swept MIDI note: the accumulator integrates the fast-math rounding of every non-integer pitch
- `oscillators/synth_oscillator/transpose_quantize_local` (SAS-35; x86, macos): swept transpose with local snapping: the accumulator integrates the fast-math rounding of the pitch offset
- `oscillators/synth_oscillator/transpose_quantize_global` (SAS-35; x86, macos): swept transpose with global snapping: the accumulator integrates the fast-math rounding of the pitch offset
- `oscillators/synth_oscillator/transpose_tune_audio` (SAS-35; x86, macos): audio-rate transpose and tune: the accumulator integrates the fast-math rounding of the pitch offset
- `oscillators/synth_oscillator/distortion_1_spread` (SAS-35; macos): sync distortion with its amount ramped and spread across 4 unison voices; on macOS arm64 the official reference misses max_abs_error against its no-fast-math build (2.2e-3, residual within)
- `oscillators/synth_oscillator/distortion_2_spread` (SAS-35; macos): formant distortion with its amount ramped and spread across 4 unison voices; on macOS arm64 the official reference misses max_abs_error against its no-fast-math build (1.5e-3, residual within)
- `oscillators/synth_oscillator/distortion_3_spread` (SAS-35; macos): quantize distortion with its amount ramped and spread across 4 unison voices; on macOS arm64 the official reference misses max_abs_error against its no-fast-math build (8.8e-4, residual within)
