## 2026-10-22: M7 render coverage 3, per-channel expression

Voice Engine. Third feature-stacking render case (engine-completeness audit,
"full-render coverage"). Stacked on render coverage 2.

**What changed**

- `generate.py`: MIDI helpers (`ramp`, `pitch_bend`), the authored preset
  `mpe_expression` and the MIDI `mpe_expression`.
- `render/mpe_expression__expression`, with its x86 CI-identity hash and a
  `preset/state` case:
  - MIDI: three notes on channels 2–4, each with its own pitch-bend ramp,
    channel-pressure ramp and slide (CC 74) ramp, released with velocities
    100, 15 and 120 (lift); then on channel 1 the sustain pedal holds two
    notes past their note-offs, with polyphonic aftertouch on one and the
    mod wheel rising, until the pedal lifts.
  - Preset: aftertouch → ladder cutoff, slide → wave frame of the authored
    line wavetable, lift → resonance, velocity → level, mod wheel →
    oscillator 2 level, envelope 3 → oscillator 2 tune, pitch-bend range
    24, oscillator 2 FM by oscillator A (the FM-by-A distortion moved here
    from render coverage 2, see there).

Upstream never enables MPE mode (`SynthBase::setMpeEnabled` has no caller),
so `MidiManager` takes the non-MPE paths: per-channel pitch wheel
(`setZonedPitchWheel(value, channel, channel)`), per-channel pressure and
slide, polyphonic aftertouch per note, sustain per channel. That is what
per-note expression from an MPE controller exercises in the shipped plugin,
and what this case renders.

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
