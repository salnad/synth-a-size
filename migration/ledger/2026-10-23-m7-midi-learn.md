## 2026-10-23: MIDI learn (engine, plugin, user config)

Voice Engine. GUI Builder found no MIDI learn in the engine, so the editor
omitted upstream's MIDI-learn menu items. Upstream has it in
`MidiManager`, `SynthBase` and `LoadSave`; the port had none of it.

**What changed**

- `engine/src/midi_manager.rs` (`midi_manager.cpp`):
  - the learn map (controller → parameters by name; a parameter may be
    learned on several controllers);
  - `arm_midi_learn` (also resets bank, folder and preset, as upstream),
    `cancel_midi_learn`, `clear_midi_learn` and `is_midi_mapped`;
  - `midi_input`: an armed parameter learns the controller, then every
    parameter learned on it is set to `value / 127` of its range from the
    minimum, rounded if indexed, through the caller's listener
    (`valueChangedThroughMidi`);
  - the controller dispatch now matches `processMidiMessage`: bank select
    (CC 0), folder select (CC 32), all notes off and all controllers off
    return early; every other controller, handled or not (mod wheel,
    sustain, sostenuto, soft pedal, slide, the LSBs, all sounds off),
    falls through to `midi_input`. Bank and folder are stored, as upstream
    does, though nothing reads them.
- `engine/src/synth.rs` (`synth_base.cpp`): the learn calls, and
  `valueChangedThroughMidi`, which sets the control alone. Unlike
  `set_control` (`valueChangedExternal`), it doesn't fan the mod or pitch
  wheel out to the voices; upstream's `ValueBridge` guard keeps the host
  notification from coming back as an external change. The changes are
  queued for the plugin. An offline render drops them.
- `plugin/src/config.rs` (`load_save.cpp`): the user config file
  (`getConfigFile`, `getConfigJson`, `saveJsonToConfig`, `saveMidiMapConfig`
  and the MIDI map in `loadConfig`) in upstream's schema and JSON
  formatting:
  `"midi_learn": [{"source", "destinations": [{"destination", "min_range", "max_range"}]}]`.
  Saving rewrites only that key. A controller whose parameters were all
  cleared keeps an entry with `null` destinations, which doesn't reload, as
  upstream. The file is JUCE's `PropertiesFile` default location named after
  the product:
  - macOS: `~/Library/Application Support/synth-a-size/synth-a-size.config`;
  - Linux: `~/.synth-a-size/synth-a-size.config`;
  - Windows: `%APPDATA%\synth-a-size\synth-a-size.config`.
- `plugin/src/midi_learn.rs`, `processor.rs`, `lib.rs`, `params.rs`,
  `editor.rs`:
  - the shipped plugin (`Default`) loads the saved map into the engine at
    construction, as `SynthBase`'s constructor does through `Startup`.
    `SynthPlugin::new` keeps it in memory; tests never touch the user's
    config.
  - The editor's arm, cancel and clear requests are queued and applied at
    the start of the next block, bypassed or not.
  - The editor reads a copy of the map that the audio thread publishes
    after each change.
  - A value a learned controller sets lands on its host parameter: the
    editor shows it, saved states keep it, and it counts as set since the
    last restore. It isn't sent back to the engine as a host edit.
- `sas_gui::EditorModel`: `arm_midi_learn`, `cancel_midi_learn`,
  `clear_midi_learn`, `is_midi_mapped` and `midi_learn_ccs`, the last a
  port addition (upstream has only the yes/no query). They have default
  no-op implementations, and the plugin's `PluginModel` implements them.

**Implementation detail: the config write is off the audio thread.**
Upstream writes the config file inside `midiInput`, on the audio thread
(its own TODO says it shouldn't). Here the audio thread asks nice-plug's
background thread to write it (`PluginTask::SaveMidiLearnConfig`) at the end
of the block that learned. The file contents are the same and the write
follows within a block. A clear from the editor saves at once on the
editor's thread, as upstream's `clearMidiLearn` does on its calling thread.
If the editor clears while a block is learning, the next block applies the
clear and publishes and saves again, so the file always ends with the
engine's map.

**Divergences:** D16 (the host isn't sent the parameter edit; nice-plug has
no audio-thread edits) and D17 (the map loads without upstream's
data-directory install check; unreadable entries are skipped).

**Tests.**
- `engine/tests/midi_learn.rs` (7): learn and follow, scaling, indexed
  rounding, one controller to several parameters and the reverse, clear,
  cancel, unknown names, the non-learnable controllers, the fall-through
  controllers, replacing the map.
- `plugin/src/config.rs` (5): the exact upstream JSON, null destinations
  after a clear, keeping other keys, unreadable files and entries, the
  product-named path.
- `plugin/tests/midi_learn.rs` (5): a learned controller moves the host
  parameter without echoing it to the engine, saved state keeps it, stepped
  rounding, cancel, clear and bypass, the exact file written and reloaded by
  a new instance, an editor clear racing a learn.

No reference capture: MIDI learn produces config and parameter changes, not
audio of its own.

**Other user-config items** (Voice Engine's inventory for the coordinator;
none ported):
- read by `loadConfig` besides the MIDI map: the computer-keyboard layout
  (`keyboard_layout`) and its octave keys (`octave_down`, `octave_up`),
  which turn typing in the editor into notes;
- read by `doStartupChecks`: `data_directory` (the install check and where
  presets live) and `synth_version` (`wasUpgraded`, then
  `saveVersionConfig`);
- editor-only reads elsewhere: `window_size`, `loaded_skin`,
  `animate_widgets`, `hz_frequency`, `author` (the save dialog's default),
  `ttwt_language`, additional browser folders;
- network and auth (dropped by rule 4): `check_for_updates`, `work_offline`,
  `authenticated`, `license`, `content_version`, installed/available packs;
- `oversampling_amount` has a reader (`getOversamplingAmount`) but no
  caller.
