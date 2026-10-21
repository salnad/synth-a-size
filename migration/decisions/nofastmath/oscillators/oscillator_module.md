# No-fast-math exact cases: oscillators/oscillator_module

The module drives `SynthOscillator` with registry-scaled transpose, tune and
detune controls. A constant pitch offset or a large scaled detune is rounded
differently under fast-math and the phase accumulator integrates it (see
`synth_oscillator.md`). The pitch is compared on the official reference in
`oscillator_module_pitch`.

- `oscillators/oscillator_module/transpose_tune` (SAS-35; x86, macos): constant transpose and tune through the module controls: the accumulator integrates the fast-math rounding of the pitch offset
- `oscillators/oscillator_module/transpose_tune_unsnapped` (SAS-35; x86, macos): constant transpose and tune without snapping: the accumulator integrates the fast-math rounding of the pitch offset
- `oscillators/oscillator_module/detune_power_range` (SAS-35; x86, macos): large scaled unison detune: the accumulator integrates the fast-math rounding of each voice's detune ratio
