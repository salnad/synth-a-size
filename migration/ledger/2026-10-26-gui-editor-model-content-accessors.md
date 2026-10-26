## 2026-10-26: GUI, read-only wavetable and sample accessors on the editor model

**Changed.** `sas_gui::EditorModel` gains five read-only accessors that
G3b (the oscillator names and the sample viewer) and G5 (`Wavetable3d`
and the wavetable editor) need. Each defaults to nothing, and the
plugin's `PluginModel` implements them all from the edit bus's document
mirror on the GUI thread, so the audio thread does no extra work and
allocates nothing:

- `wavetable(oscillator) -> Option<Arc<Wavetable>>`: the rendered table
  (`SynthBase::getWavetable`), shared from `EditHandle::wavetable`'s cache.
  It is rendered once after each change, on the calling thread.
- `wavetable_name(oscillator)`: the creator's name, as
  `FullInterface::getWavetableName` shows it. This reads the document and
  renders nothing.
- `with_sample(f)`: lends `f` the document's `SampleState` in place, with
  no copy. `SampleViewer` scans the whole buffer once per change, so a
  copy of up to `Sample::kMaxSize` frames per frame drawn would be waste.
- `sample_name()`: `Sample::getName`.
- `content_revision()`: the edit bus revision, which moves on with each
  edit, preset load and state restore, so drawings of the wavetables or
  the sample can be cached against it.

**Verified.**

- `wavetables_and_the_sample_follow_edits` (sas-plugin):
  - the default names are "Init" and empty past the oscillators, and the
    table is shared until it changes;
  - a renamed creator set through the edit bus shows its name on that
    oscillator only;
  - a set sample shows its name and lends its exact buffer;
  - the revision moves on after each edit.
- `cargo clippy --workspace --all-targets -D warnings`, `cargo fmt`, and
  the sas-gui and sas-plugin editor tests pass.

**Host checks.** None: these are reads with no host-visible behaviour, so
nothing needs FL Studio or REAPER.
