## 2026-10-23: GUI G3a, line renderer, header meters, bend wheels and keyboard

**Changed.** The first half of G3 (the plan allows G3 to split). `sas-gui`
gains upstream's line and fill renderer and the live parts of the header,
the Bend section and the on-screen keyboard, on G0's telemetry (#97) and
keyboard plumbing (#105).

- Line renderer (`primitives/line.rs`, `line.wgsl`): `OpenGlLineRenderer`
  with its vertex generation ported line by line (including the fast
  inverse square root, bit for bit), anti-aliased edges, fills to a centre
  line, boosts, looping lines and `fit`. Drawn through egui-wgpu callbacks
  like the quads.
- Quads: per-quad shader coordinates (`setCoordinates`), the rounded-corner
  masks (`OpenGlCorners`), and the gain meter shader.
- Header: the oscilloscope (`Oscilloscope`, both channels from the
  telemetry's oscilloscope memory) and the volume section (`VolumeSection`:
  peak meters with clip colour and memory marker from `peak_meter` /
  `peak_meter_memory`, ticks, and the volume arrow slider).
- Bend section: the pitch and mod wheels (`ControlWheel` drawing, vertical
  drag at sensitivity 0.5, the pitch wheel springing back after its
  gesture). The engine already sends `pitch_wheel` / `mod_wheel` control
  changes to every channel's voices, so the wheels only set their host
  parameters.
- Keyboard (`KeyboardInterface`, `MidiKeyboard`): key geometry, hover and
  pressed highlights, velocity from the click height, drag across keys.
  Notes go through `EditorModel::note_on` / `note_off`; keys light from
  `held_notes()` (keyboard and host MIDI), plus the key the mouse is
  playing until its press shows there.
- `EditorModel::telemetry()`, implemented by the plugin; the editor repaints
  continuously while telemetry is present.
- Plan: Macros move to G4 (each has a modulation source and an editable
  name); the keyboard plumbing was Edit Bus's #105.

**Verified.**

- Layout: `header_section/VOLUME/volume`, `BEND/pitch_wheel` and
  `BEND/mod_wheel` added to the fixtures; the port's rectangles equal
  upstream's at all six sizes.
- Pixels, side by side with ref-gui: volume section, wheels and keyboard
  match. ref-gui's oscilloscope shows upstream's state with no memory
  attached (its points bunched at the top left), so the oscilloscope was
  checked with fake telemetry instead: both channels, filled.
- Unit tests: line vertices (band width, edge depths, faded end caps),
  fills, boosts, fast inverse square root bits, meter positions, keyboard
  notes, velocities and black key geometry.
- Interaction: the keyboard sends note on, off and on across a drag, then
  off on release.
- `cargo fmt`, `clippy -D warnings`, workspace build, `sas-gui` and
  `sas-plugin` tests.

**Not done (tracked).** The spectrogram view and its toggle, the
oscillators, sample and filter sections with response curves, the Hz
alternate display and the animate switch (G3b); MIDI learn menu items after
#99.
