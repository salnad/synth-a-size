## 2026-10-26: GUI G5 (4), dropping audio files on the wavetable view

**Changed.** Dropping an audio file on an oscillator's wavetable view loads
it as the oscillator's wavetable, as upstream's `Wavetable3d` does
(`AudioFileDropSource`, `fileDragEnter` / `fileDragMove` / `fileDragExit`,
`audioFileLoaded`, `FullInterface::loadAudioAsWavetable`).

- **Drop targets** (vendored egui-baseview patch 3, extended): like JUCE's
  `FileDragAndDropTarget`s, only places that want a file take it.
  - Each frame the app registers targets (`file_drop::add_drop_target`: a
    rectangle and the extensions it takes), and the window takes them after
    the frame.
  - A drag is answered `AcceptDrop(Copy)` only over a target that takes
    exactly one dragged file with one of its extensions
    (`isInterestedInFileDrag`), matched without case. Otherwise it is
    refused, with no copy badge, and a drop delivers nothing.
  - baseview needs the answer during the event, so it comes from the last
    frame's targets.
- **The view:**
  - It registers itself as a target for the model's audio extensions
    (`EditorModel::audio_file_extensions`).
  - While a file it takes hovers over it, the view picks how it would load
    by height (`updateDraggingPosition`): the top 3/8 splices, the bottom
    3/8 pitch-splices, the middle vocodes. It shows upstream's import
    overlay, the overlay screen with WAVETABLE, VOCODE and PITCH SPLICE,
    the chosen one lit and the others half faded.
  - A drop on the view loads the file with the style under the pointer
    (`EditorModel::load_audio_as_wavetable`).
- **The plugin** (`sas_plugin::audio_file`, `PluginModel`):
  - It reads the file's first channel and sample rate, as
    `loadAudioFile` passes `getReadPointer(0)`.
  - It reads the `clm ` chunk's text (`getWavetableDataString`, walking
    chunks without pad bytes as upstream does), then the fade style
    (`getFadeStyleFromWavetableString`) and author
    (`getAuthorFromWavetableString`) from it.
  - It clears the oscillator's creator, runs `init_from_audio_file` with
    the style and fade, names it after the file and sets it through the
    edit bus.
  - An unreadable file changes nothing.
- **WAV only for now (D23):** the port's own reader covers 8, 16, 24 and
  32-bit PCM and 32 and 64-bit float, plain or extensible, with the same
  sample values as JUCE's WAV reader. The next PR adds symphonia for
  upstream's macOS formats.
- **FL Studio checklist item 5** now says where the copy badge should and
  shouldn't appear.

**Verified.**

- `tests/file_drop.rs` (the adapter):
  - one WAV over a target is accepted, hovers, and drops once;
  - outside the target, two files, another extension, an extension-less
    name, no files, and no targets are all refused and drop nothing;
  - `.WAV` matches `wav`;
  - leaving clears the hover;
  - a frame's targets are taken once.
- `audio_file` unit tests:
  - 16-bit stereo gives the first channel;
  - every supported sample format scales as JUCE's reader does,
    extensible included;
  - non-WAV and compressed formats are refused;
  - the `clm ` chunk gives the fade style and author, with upstream's
    fallbacks.
- sas-plugin, `a_dropped_wav_becomes_the_oscillators_wavetable`: a WAV
  with a `clm ` chunk loads as each style, named after the file, with the
  author, moving the revision. Text, a missing file and an oscillator past
  the last change nothing.
- `tests/controls.rs`:
  - the view registers its rectangle for WAV files;
  - a hovering WAV picks splice, vocode or pitch splice by height, and
    two files, a FLAC or a pointer outside the view give no overlay;
  - drops load with the style where they landed, and files the view
    doesn't take load nothing, with no host edits.
- `cargo clippy --workspace --all-targets -D warnings`, `cargo fmt`, and
  the sas-gui and sas-plugin tests pass.

**Host checks.** Checklist item 5 (FL Studio, with a REAPER note), now with
where the copy badge should appear.
