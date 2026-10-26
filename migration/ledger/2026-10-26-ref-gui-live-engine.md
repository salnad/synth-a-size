## 2026-10-26: ref-gui runs the engine before its frame

Until now `ref-gui` never processed audio. Displays that read engine
outputs (the distortion filter response, chorus and flanger taps, phaser
cutoff, meters, the oscilloscope) showed a never-run engine, so the gallery
couldn't show the live state the port's renders can. Tabs Builder reported
that calling `SynthBase::processAudio` in the GUI build crashed on a null
input in `vital::Compressor::processWithInput`.

**The crash doesn't reproduce here.** The same render loop as `ref-render`,
inside the GUI build, ran cleanly with the init preset, `all_effects.vital`
and the four filter presets of #125, on the voice and effects tabs, and on
every placement tried: before the preset loads, before the interface is
shown, and after the tab is selected. An AddressSanitizer build runs the
same cases without a report.

The likely cause is a block longer than `vital::kMaxBufferSize` (128)
passed to `processAudio` in one call. A temporary, uncommitted hook in the
AddressSanitizer build did that with a 512-sample block, and got a
heap-buffer-overflow write in `vital::ModulationSum::process`, under
`SoundEngine::process`. Without the sanitizer, that overflow corrupts
neighbouring heap objects, so a later read of a garbage pointer is how it
shows up, as in the compressor's input. Neither upstream nor the harness is
at fault. `SynthPlugin::processBlock` splits each host block into
sub-blocks of at most `kMaxBufferSize` samples, and `RenderLoop` does the
same.

**Changed**

- **One render loop.** `RenderSynth`'s `prepare` and `renderBlock` move
  unchanged into `harness::RenderLoop<Base>` in
  `migration/reference/src/render_synth.h`. `RenderSynth` is
  `RenderLoop<HeadlessSynth>`, and `ref-gui`'s window is
  `RenderLoop<SynthBase>`, so both drive the engine exactly like
  `SynthPlugin::prepareToPlay` and `processBlock`. The audio reference's
  output is unchanged: every pinned hash still matches (see Checks).
- **`ref-gui --midi file.mid --seconds S --block-size N`.** Loads the MIDI
  with `harness::loadMidi` and renders `S` seconds of audio (default: up
  to the last MIDI event) with `ref-render`'s block loop, at 44.1 kHz and
  120 BPM, before the interface is on screen. No frame is drawn while the
  engine runs. The interface then gets the oscilloscope and audio memory,
  as both upstream editors give it.
- **`--animate on|off`**, default off. #125 turned animation off for every
  frame, because a silent engine's outputs are idle (a filter response at
  cutoff 0). With a running engine, the animated views are meaningful, so
  a view can choose `--animate on`, which needs `--midi` or `--seconds`. The
  default stays #125's.
- **`gui_reference.py frames`** reads `"midi"`, `"seconds"` and `"animate"`
  from each view (`live_args`). The MIDI must be an authored file in
  `migration/corpus/midi/`. Tests are in `test_gui_reference.py`.

`views.json` gets no new views here. Tabs Builder's G6a and GUI Builder's
#125 add the effects and filter views, and can give them these fields.

Built on #125's branch, since it extends #125's animation switch; merge
after #125. It touches different lines of `gui_main.cpp` and
`gui_reference.py` from G6a's `--effects-scroll`.

Checks:

- `test_gui_reference.py` passes (9 tests).
- `ref-gui` renders the live views without a crash, under gdb and under
  AddressSanitizer.
- The audio reference rebuilt with `RenderLoop` matches all pinned render
  and module hashes, with the fast-math and no-fast-math builds.
