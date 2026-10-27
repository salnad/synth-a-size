## 2026-10-27: GUI G7d (2b), samples in the popup browser

The third G7d PR, stacked on G7d-2a (#173): the popup browser for the
sample, the sample name's arrows, and loading a sample file.

**The sample section** (`port/crates/gui/src/sections/sample.rs`)

- A press on the sample's name opens the popup browser for the user's
  `.wav` and `.flac` samples (`kSampleExtensionsList`), 450 × 300 at the
  size ratio, right of the name and level with it
  (`SampleSection::textMouseDown`).
- The arrows load the sample before or after the current file among the
  user's, wrapping round (`prevClicked`, `nextClicked`, `getShiftedFile`).
- A file chosen in either becomes the current file before it's read, as
  upstream's `setLastBrowsedFile` comes first, and loads through the model.

**Loading** (`EditorModel::load_sample_file`, the plugin's
`load_sample_file`), as `SampleSection::loadFile` does:

- The file is read with up to two channels: a stereo file loads both
  (`loadSample(left, right, …)`), a mono file its one.
- At most `kMaxFileSamples` (17,640,000) frames are read: the reader stops
  there, as JUCE's `read` of `min(length, kMaxFileSamples)` does, rather
  than decoding the whole file and truncating (an hour of 16-bit stereo
  would briefly hold about 1.3 GB). A mono sample then stops at the mono
  `loadSample`'s `kMaxSize` (1,764,000); the stereo one has no cap.
- The sample is named after the file. A file that can't be read changes
  nothing (upstream keeps the old sample when no reader opens it).
- Upstream also remembers the file in the sample's `last_browsed_file`; the
  port's `SampleState` has no such field, so the section keeps it.

**The audio reader** (`port/crates/plugin/src/audio_file.rs`)

- `read_audio_channels(bytes, extension, max_channels, max_frames)` reads
  up to that many channels, in file order, and stops at `max_frames`: the
  WAV reader caps its frame count before reserving, and the symphonia loop
  takes only what's left of a packet and stops (Graph Reviewer, #174).
  `read_audio` and `read_wav` are it with one channel and no frame cap, as
  the wavetable import takes the whole file (`loadAudioFile` reads
  `lengthInSamples`), so it is unchanged.
- One reader serves both, so #154's hardening holds for every channel: the
  bounded zero-fill of a truncated `data` chunk, 0xFFFFFFFF and 0 lengths
  read to the end of the file alike, `try_reserve` before every buffer the
  header sizes, and the refusal of 0- and over-64-bit samples. symphonia's
  decode keeps each channel with `try_reserve` too.

**Popup browser.** The sample is a third owner (`BrowserOwner::Sample`).
Upstream's `visibilityChanged` lists the remembered folder with the
previous owner's extensions before `loadPresets` replaces the folders; the
port sets the new owner's first. Kinds have separate folders, so the
remembered folder is never among the new owner's, and both list every file
of the new owner's types.

**Tests**

- `audio_file`: `up_to_two_channels_read_in_file_order` (stereo, mono, a
  third channel not read) and `two_channel_reads_keep_the_readers_limits`
  (the bounded zero-fill in both channels, 0- and 72-bit refusal), and
  `a_wav_read_stops_at_max_frames`.
- `tests/audio_files.rs` `decoding_stops_at_max_frames`: the stereo FLAC
  and AIFF read to 1,000 frames (part way through a packet) and 4,096 give
  the start of the whole read in both channels.
- `sas-plugin` `a_sample_file_loads_as_upstream_loads_it`: a stereo file
  past the mono cap keeps both channels and its length, a mono one stops at
  1,764,000 frames, both are named after the file, and a missing file
  changes nothing.
- `tests/preset_selector.rs`
  `the_sample_name_opens_the_browser_and_the_arrows_step`, and
  `the_sample_arrows_step_past_a_file_that_doesnt_read` (UI Reviewer's
  suggestion).
- `authored_audio_files_decode_as_juce_reads_them` passes against a current
  reference build.

No audio change: the engine plays the loaded sample as it plays one from a
preset.
