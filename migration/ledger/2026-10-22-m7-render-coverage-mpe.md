## 2026-10-22: M7 render coverage 3, per-channel expression

Voice Engine. Third feature-stacking render case (engine-completeness audit,
"full-render coverage"). Stacked on render coverage 2.

**What changed**

- `generate.py`: MIDI helpers (`ramp`, `pitch_bend`), the authored preset
  `mpe_expression` and the MIDI `mpe_expression`.
- `render/mpe_expression__expression`, with its x86 CI-identity hash and a
  `preset/state` case:
  - MIDI: three notes on channels 2–4, each with its own channel-pressure
    and slide (CC 74) ramps, the first two with their own pitch-bend ramps
    (up and down; channel 4's bend stays centred), released with velocities
    100, 15 and 120 (lift); then on channel 1 the sustain pedal holds two
    notes past their note-offs, with polyphonic aftertouch on one and the
    mod wheel rising, until the pedal lifts.
  - Preset: aftertouch → ladder cutoff, slide → wave frame of the authored
    line wavetable, lift → resonance, velocity → level, mod wheel →
    oscillator 2 level, envelope 3 → oscillator 2 tune, pitch-bend range
    24, oscillator 2 FM by oscillator A (the FM-by-A distortion moved here
    from render coverage 2, see there).

The case renders `MidiManager`'s paths with MPE mode off (`mpe_enabled` 0,
the preset default): per-channel pitch wheel
(`setZonedPitchWheel(value, channel, channel)`), per-channel pressure and
slide, polyphonic aftertouch per note, sustain per channel.

MPE mode itself is reachable upstream but still in no full render.
`mpe_enabled` is a preset parameter, and the plugin's editor applies it
through `SynthBase::setMpeEnabled` (`VoiceSettings` in
`interface/editor_sections/master_controls_interface.cpp`, on every
`setAllValues`, so on preset load, and on the MPE button). With it on, a zone
master channel's pitch bend, pressure, slide, sustain and sostenuto apply to
the whole zone (`setZonedPitchWheel` over the zone, `setChannelRange*`,
`sustainOnRange`), and member-channel bends go through
`setPitchWheel(value, channel)`. `ref-render` loads presets without the
editor, so it never turns MPE mode on, and the port follows it. Whether the
GUI-less port should apply `mpe_enabled` itself is an open question for the
owner.

**Results.** Passes against the official reference; the port is
bit-identical to the `REF_FAST_MATH=OFF` reference.

**Mutation checks** (every render case run per mutation; only this case
fails):

| Mutation | Fails |
|---|---|
| lift ignored (note-off velocity fixed at 0.5) | `mpe_expression__expression` |
| slide ignored | `mpe_expression__expression` |
| channel pressure ignored | `mpe_expression__expression` |
| pitch bend applied to channel 1 only | `mpe_expression__expression` |
| sustain pedal ignored | `mpe_expression__expression` |

**macOS.** No aarch64 hash yet; the advisory job's `macos-hashes` patch adds
it.
