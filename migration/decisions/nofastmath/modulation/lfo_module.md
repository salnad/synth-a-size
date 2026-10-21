# No-fast-math exact cases: modulation/lfo_module

At high keytrack notes the LFO runs at hundreds of Hz to tens of kHz. The
official reference's keytrack frequency (TempoChooser, MIDI → Hz) differs from
its no-fast-math build by about 1e-7 relative, and the LFO integrates it into
phase drift over the render: `value` misses by up to 4.7e-3 (−57 dB) and
`osc_phase` by 0.9 where the encoded phase wraps. The frequency itself passes
as a phase increment and stays on the official reference in
`lfo_module_frequency`.

- `modulation/lfo_module/keytrack_audio` (SAS-35; x86, macos): audio-rate LFO at keytrack notes 60 and 96 integrates fast-math frequency noise into phase drift
- `modulation/lfo_module/keytrack_range_audio` (SAS-35; x86, macos): audio-rate keytrack sweep over MIDI 0..127 (+36, +1 tune) integrates fast-math frequency noise into phase drift
- `modulation/lfo_module/keytrack_range_control` (SAS-35; x86, macos): control-rate keytrack sweep over MIDI 0..127 (+36, +1 tune) integrates fast-math frequency noise into phase drift
