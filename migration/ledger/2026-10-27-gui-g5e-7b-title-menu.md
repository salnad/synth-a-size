## 2026-10-27: GUI G5e (7b), the wavetable editor's title menu

The second part of G5e-7, stacked on G5e-7a (#189): the menu beside the
wavetable editor's name (`WavetableEditSection::showPopupMenu`), with
upstream's five items.

- **Save As Wavetable** (`FullInterface::saveWavetable`): the G7 save
  section for the oscillator's wavetable (`SaveTarget::wavetable`), over
  the editor; the interface now draws the save section in editor mode.
- **Import Wavetable** (`importWavetable`): an open dialog for `.vitaltable`,
  `.wav` and `.flac` files (`kWavetableExtensionsList`); the file chosen
  loads through the editor's `loadFile` (`editor_load_wavetable_file`, so a
  `.flac` loads as audio, D32) and becomes the creator's last file.
- **Export Wavetable** (`exportWavetable`): a save dialog with no name in
  it; the wavetable's JSON is written with the `.vitaltable` extension in
  place of any the dialog gave (`withFileExtension`).
- **Export to .wav File** (`exportToWav`, `wavetable_export.rs`): the
  wavetable rendered to 256 frames (`renderToBuffer`), written as JUCE's
  `WavAudioFormat` writer writes a mono 16-bit file at 88.2 kHz: RIFF,
  the 52-byte `JUNK` padding, a 16-byte PCM `fmt `, the `clm ` chunk (its
  text and terminating zero, padded to even) and the samples, converted as
  `AudioData::Int16::setAsFloatLE` converts them (times 32768, halves to
  even, within ±32767). The `clm ` text is upstream's frame-size header with
  this product's name in the brackets where upstream names its site
  (rule 2): `<!>2048 20000000 wavetable (synth-a-size)`.
- **Synthesize Preset to Table** (`resynthesizeToWavetable`): the model's
  existing resynthesis, as the oscillator menu's "Resynthesize Preset to
  Wavetable" runs the same upstream function.

Text to Wavetable stays absent (D20), and mouse-wheel zoom stays out.

**Tests**

- `wavetable_export` `the_file_is_laid_out_as_juces_writer_lays_it_out`:
  every header field, the RIFF size matching the file, and the sample
  conversion's rounding and clipping.
- `tests/wavetable_editor.rs` `the_title_menu_saves_and_resynthesizes` and
  `the_exports_write_the_json_and_the_rendered_frames` (the dialogs are
  native; the writing they lead to is tested directly).

No audio change.
