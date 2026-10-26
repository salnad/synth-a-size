## 2026-10-26: GUI G6c (1): the Advanced tab's settings, built-in tunings and a file dialog

The Advanced tab (`sections::advanced`, upstream's
`MasterControlsInterface`) gains its settings panels and output displays.
The oscillators' advanced sections above them come in part 2.

**Panels** (in `layout.main`, below where the Voice tab's oscillators are,
as upstream's `setOscillatorBounds` puts the advanced sections)

- VOICE (`VoiceSettings`): MPE ENABLED, note priority and voice override
  selectors with upstream's names, the tuning selector, and tune and
  transpose text sliders (transpose with its 0.2 drag sensitivity).
- OVERSAMPLING (`OversampleSettings`): 1x to 8x buttons; a click sets
  `oversampling` in one gesture (`valueChangedInternal`).
- DISPLAY (`DisplaySettings`): FREQUENCY UNITS (Semitones or Hz) and SKIN.
  The units are an editor setting: `EditorModel::set_hz_frequency_display`
  is new, and the plugin keeps it in the config file's `"hz_frequency"` as
  `LoadSave::saveDisplayHzFrequency` does, read once when the editor model
  is made. #132's `Slider::hz_alternate` cutoffs follow it. SKIN lists
  "Default" and "Load Custom Skin..."; custom skins are parked (I17), so
  choosing it does nothing.
- ANALYSIS (`OutputDisplays`): the output's oscilloscope above its
  spectrogram (#135's widget, reading the output memory in telemetry).
- Each panel's tab shadow, horizontal heading and border.
- Editor settings that aren't parameters (the tuning, units and skin)
  draw as `TextSelector`s do and list their options through a new
  `MenuOwner::Setting`, which hands the choice and the egui context to a
  function.

**Tunings** (`TuningSelector`)

- The list has the built-in tunings and "Load Tuning File...". A built-in
  tuning is named after itself and loads its scale, or the default tuning
  (`loadTuning`), into a copy of the synth's tuning that
  `EditorModel::load_tuning` then plays.
- The three built-in scales (7-limit and 5-limit just intonation,
  Pythagorean) are written in `advanced::TUNING_SCALES` from their
  published ratios, not copied from upstream's `.scl` files (rule 3). A
  test loads each into a `Tuning` and checks every MIDI note against
  upstream's file of the same name.
- "Load Tuning File..." opens the file dialog below; the chosen file
  changes the tuning as `SynthBase::loadTuningFile` does.

**File dialog** (`controls::file_dialog`, shared)

- Open and save dialogs from `rfd` (MIT), in place of JUCE's
  `FileChooser`. JUCE's choosers block in a modal loop; here a dialog is a
  future the editor polls each frame without waiting (with a repaint every
  50 ms while one is open), so drawing never stops. `open(ctx, id,
  request)` starts one; `take_result(ctx, id)` returns the chosen path
  once, or `None` for a cancel. Requests name the kind (open or save), the
  title, extension filters (`extensions` turns JUCE wildcard lists into
  them), the start folder and a suggested file name, for G7's browsers to
  reuse.
- On macOS `rfd` shows the dialog as a sheet on the window it is given; the
  editor gives it its own window (baseview's weak `PlatformHandle`, set when
  the window opens), so the sheet attaches to the plugin window rather than
  the host's main window. AppKit runs it on the main thread the editor
  already draws on. On Linux it uses the desktop portal (no GTK or Wayland
  features).
- `sas-gui` type-checks for `aarch64-apple-darwin` with the AppKit backend.
- The FL Studio checklist gets item 8, the file dialog, with a REAPER note
  (after #138's item 7).

**Tests**

- Layout fixtures: 12 `master_controls` paths (the four panels, the voice
  panel's controls and the display panel's selectors) match upstream at all
  six sizes.
- `sections::advanced::the_built_in_scales_match_upstreams_tunings`.
- `controls::file_dialog`: `wildcards_become_extensions` and
  `answers_arrive_once_under_their_id` (a pending dialog is polled without
  waiting, and its answer is taken once, under its own id).
- `sas-plugin` `config::the_hz_frequency_setting_round_trips`: false when
  missing, and saving it keeps the MIDI map beside it.

No audio change.
