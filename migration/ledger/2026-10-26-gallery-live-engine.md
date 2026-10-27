## 2026-10-26: The port's gallery runs the engine for live views

Since #129, `ref-gui` can run the engine before its frame (`"midi"`,
`"seconds"`) and draw the animated views from it (`"animate"`). The port's
gallery still drew every view with no engine, so those views compared live
upstream against a static port.

**Changed**

- **`port/crates/gui/examples/gallery.rs`.** A view with `"midi"` or
  `"seconds"` now runs the port's engine the way `ref-gui` runs upstream's:
  - a `Synth` with the view's preset loaded;
  - the MIDI from `sas_engine::midi_file::read_midi_file`;
  - rendered by `sas_engine::synth::render` at 44.1 kHz and 120 BPM in
    64-sample host blocks, which `render_block` splits into engine
    sub-blocks of at most 128 samples;
  - `S` seconds of audio, by default up to the last MIDI event.

  It then writes the synth's telemetry once into its own `Telemetry` pair
  (with the equalizer memory, as the plugin does), and the gallery's
  `EditorModel` returns it from `telemetry()`. `held_notes()` feeds the
  events the render played through the plugin's own keyboard state
  (`sas_engine::keyboard`: per channel, cleared by all-notes-off), as
  upstream's `MidiKeyboardState` shows them (UI Reviewer's nit).
- **Same checks as `gui_reference.py`'s `live_args`.** The MIDI must be in
  `migration/corpus/midi/`, `seconds` must be a number of at least 0, and
  `animate` needs a render that isn't empty.
- **A live view.** `all_effects_live` (the effects tab, `all_effects.vital`,
  2 s of `chord_progression.mid`, animated) joins `views.json`, and the
  description lists the new fields. On both sides it shows chorus taps,
  compressor meters, the EQ spectrum, the oscilloscope, the level meter and
  the held keys, where the static views show them empty. `modulations`
  (`seconds` 0.05, animated) is now live on the port's side too.
- **The `ref-gui` README** no longer says "live upstream, static port".

**Not yet:** the port's editor has no animate switch
(`sections/oscillator.rs` hard-codes upstream's default, on). So a live
view without `"animate": true` still draws its animated views from the
engine on the port's side, while `ref-gui` draws them at rest. The gallery
prints a note for such a view. No current view is like that.

New views with text editors or popups should wait for #157, which gives
`ref-gui` upstream's `DefaultLookAndFeel`. `all_effects_live` uses only the
existing effects components.

Checks:

- The gallery renders all 13 views locally (Mesa lavapipe).
- `ref-gui` renders `all_effects_live`.
- The effects-column crops in the PR match side by side.
