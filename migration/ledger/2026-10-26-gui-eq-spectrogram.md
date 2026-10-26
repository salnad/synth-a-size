## 2026-10-26: GUI: the EQ's spectrogram, hertz on effect cutoffs, and the equalizer memory's first block

A follow-up to G6b (#131) and #135.

**EQ**

- The EQ section draws a spectrogram of its own output under the
  response, as upstream's does: `controls::spectrogram` reading the
  equalizer memory in telemetry, from the low cutoff's lowest to highest
  frequency, at the engine's sample rate and oversampling. Its line is the
  lighten colour and its fill the widget background lightened, fully
  opaque (`EqualizerSection::paintBackground` and
  `renderOpenGlComponents`).
- `EqualizerResponse::show` takes what to draw between its frequency lines
  and the response; the reverb draws nothing there.

**Hertz** (#132's `Slider::hz_alternate`, where upstream calls
`setSliderHasHzAlternateDisplay`)

- The distortion filter's cutoff, the flanger's and phaser's centres, the
  EQ's three cutoffs and the reverb's four (pre low and high, and the
  shelves) show hertz when the frequency units are Hz.
- The EQ's and reverb's handle drags show the cutoff's popup in the same
  units.

**Engine (review, Graph, #135)**

- `Synth` takes the equalizer memory when it is built rather than at the
  end of the first block, so the editor's copy holds that block too.
- `the_equalizer_memory_follows_the_eq` checks the first block arrives, and
  compares the copy's latest 256 frames with the equalizer memory's own
  (copying from one sample off fails it).

No audio change.
