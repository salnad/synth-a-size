## 2026-10-22: the engine's sample as a `SampleState`

Follow-up for Plugin Wrapper (#79), Voice Engine.

**Changed**

- `Synth::sample_state()` (`port/crates/engine/src/synth.rs`) returns the
  sample the engine plays as a `sas-preset` `SampleState`: name, sample
  rate, and the original left (and right) audio, read from the sample's
  band-limited buffers (the original copy, without its padding).
- `SoundEngineHost::sample_state()` (`sas-plugin`) returns the same owned
  value. The host's `Send` invariant (only plain values cross its surface)
  still holds and its SAFETY comment says so.

**Why:** the engine's default sample is white noise drawn with the next
engine RNG seed when the engine is built (`Sample()`), and that seed cannot
be recovered afterwards. Until a preset loads a sample, an untouched init
state saved by the plugin therefore held different noise than the engine
played; it can now take the engine's. This only matters with the sample
oscillator on.

**Tests:** `synth::tests::sample_state_is_the_engine_noise_until_a_preset_loads_a_sample`:
44100 frames of mono noise in [-0.9, 0.9) named "White Noise" at 44.1 kHz
on a new synth; after loading a preset with an authored stereo sample,
`sample_state()` equals that preset's `SampleState`.
