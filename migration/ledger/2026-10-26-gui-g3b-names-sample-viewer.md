## 2026-10-26: GUI G3b (2), the wavetable and sample names and the sample viewer

**Changed.** On #117's read-only accessors:

- The oscillator sections' name strip shows the wavetable's name
  (`EditorModel::wavetable_name`, `FullInterface::getWavetableName`), and
  the sample section's shows the sample's (`sample_name`).
- `SampleViewer` at rest (`sections/sample.rs`): the sample's peaks, each
  of 256 columns' largest sample (signed, as upstream takes it,
  `setLinePositions`), drawn as two filled lines mirrored about the centre,
  2 pixels wide, in the right channel's colours and then the left's (the
  disabled colours while the sample is off), with rounded bottom corners.
  Nothing is drawn without a sample. The peaks are read through
  `with_sample`, copied out of the callback, and cached against
  `content_revision`, read first.
- The gallery's model serves its preset's sample and wavetable names, and
  a new authored view turns the sample on (`gui/presets/sample_on.vital`).
- Still to come: the playing position's boost (with the telemetry), file
  drop (#121) and the browser, the sample's icon buttons, and the
  wavetable view (#127).

**Verified.**

- Pixels, side by side with ref-gui
  (`media/gui/g3b-sample-viewer-side-by-side.png`): with the sample on,
  the name and waveform match; the init view's "White Noise" and "Init"
  names match.
- `peaks_take_each_columns_largest_sample`.
- `cargo clippy --workspace --all-targets -D warnings`, `cargo fmt`, the
  `sas-gui` tests and `generate.py --check` pass.
