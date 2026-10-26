## 2026-10-26: GUI the header's modified preset mark

The `*` before the preset's name in the header, which G7a left out
(`SynthPresetSelector::setModified`).

- Upstream sets it from `HeaderSection::notifyChange`, which only
  `SynthBase::ValueChangedCallback` calls. That is a control's value changing
  by any route: host automation, the host's echo of the user's own edits,
  MIDI-learned CCs and the MIDI mod wheel. The pitch wheel is left out by
  name. `notifyFresh` clears it on a load, an init, a save or a MIDI program
  change.
- Edits that change no control (modulation routing, wavetables, LFO
  shapes, samples, tuning, preset info) never set it. So the port reads
  control values, not the edit bus's revision; the revision moves on all of
  those, and on the rename a save makes.
- `preset_selector::Modified` reads every registry control's value each
  frame (`Host::value`), skipping `pitch_wheel`, and marks the preset when
  one changed at the same generation. A new generation (any load, init, host
  restore or chat load: `EditorModel::preset_generation`, the edit bus's
  `generation()`) clears it and takes the values afresh, as does a
  successful Export Preset. Changes in the frame after a new generation are
  ignored, since a host's restore can finish setting values after the
  generation moves. The GUI's own loads need no such care: nice-plug's
  `set_state` blocks until the state is applied.

**Tests** (`tests/preset_selector.rs`)

- `a_control_change_marks_the_preset_modified_but_the_pitch_wheel_does_not`.
- `a_load_clears_the_mark`: a load through Initialize Preset clears the
  mark, values set with the load don't mark it, and a later change does.
  It fails when a new generation doesn't clear the mark.

No audio change.
