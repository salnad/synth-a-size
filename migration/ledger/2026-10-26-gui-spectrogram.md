## 2026-10-26: Equalizer memory in telemetry, and the shared Spectrogram widget

Upstream's EQ section draws a spectrogram of the equalizer's output under
its response, and the header can show one of the synth's output. The port
had the output's memory in telemetry but not the equalizer's, and no
spectrogram.

**Engine (Graph review)**

- `Synth` keeps the equalizer module's output memory
  (`ReorderableEffectChain::get_equalizer_memory`, upstream's
  `getEqualizerMemory`) and, at the end of each block, copies what the
  equalizer wrote into a lock-free `AudioMemory` for the editor
  (`Synth::equalizer_memory`). It follows the memory's write offset, so a
  bypassed equalizer adds nothing and the memory holds its last output, as
  upstream's does; samples come at the oversampled rate, as upstream's
  memory holds them. Nothing is allocated on the audio thread.
- `Telemetry::with_equalizer_memory(audio, equalizer)` and
  `Telemetry::equalizer_memory()`. `Telemetry::new` is unchanged (its
  equalizer memory stays silent).
- The snapshot carries `sample_rate` (`SynthBase::getSampleRate`) and
  `oversampling_amount` (`SoundEngine::getOversamplingAmount`, the amount
  the control asks for), which the spectrogram scales by.
  `SoundEngine::oversampling_amount()` reads it.
- The plugin's engine trait gains `equalizer_memory()`, and the plugin
  builds its telemetry with it.

**GUI (UI review)**

- `controls::spectrogram`: `Spectrogram` (settings, `paint_background`,
  `paint`) and `SpectrogramAnalysis` (the smoothed amplitudes and the
  transform's buffers, made once). Ports `Spectrogram`'s
  `updateAmplitudes`, `applyWindow`, `drawWaveform`, `render` and
  `paintBackground`:
  - the latest 16384 samples, ending one before the newest as
    `readSamples` does, through `sas_wavetable`'s `FourierTransform`;
  - upstream's window, which rises from 0 to 1 over the samples (half a
    Hann window);
  - 300 points over log frequency, the 3 dB/octave tilt, and the decay
    that lets peaks fall back slowly;
  - the line width fixed at 2, as `render` sets it.
- The EQ section places it under its response in G6b; the header's
  spectrogram view can use it too.

**Tests**

- `engine/tests/telemetry.rs`: `the_equalizer_memory_follows_the_eq`. With
  the EQ on, ten 64-sample blocks at 2x oversampling add 1280 samples,
  holding a note's sound; with it off they add none. The snapshot's sample
  rate and oversampling are 44100 and 2.
- `controls::spectrogram`: `a_sine_peaks_at_its_frequency` (a 1 kHz sine
  peaks at 1 kHz at about the window's −6 dB gain; silence decays) and
  `the_window_rises_from_zero_to_one`.

No audio change.
