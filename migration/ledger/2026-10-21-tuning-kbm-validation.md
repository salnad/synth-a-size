## 2026-10-21: Tuning refuses a keyboard map outside the scale (D6 follow-up)

**Changed**

- `port/crates/engine/src/tuning.rs`: a keyboard mapping that names a scale
  degree outside the scale (≥ the scale's entry count, or negative) is now
  refused with `TuningError::MappingOutsideScale`, and nothing is applied.
  Upstream's `Tuning::loadScale` reads past the scale vector here (undefined
  behaviour, `vital/src/common/tuning.cpp:340`); before this change the
  port's bounds check panicked, which a shipped plugin must not do.
- All three ways a mapping meets a scale are checked before any state
  changes:
  - `load_file` for a `.kbm`: the map is parsed into a local vector, checked
    against the current scale, then applied (start note, reference note,
    table, mapping name);
  - `json_to_state`: the saved scale and mapping are checked before any
    field is assigned;
  - `load_scale`: a new scale shorter than the current mapping is refused.
  `.scl` and `.tun` loading reset or clear the mapping first, so they cannot
  fail.
- Only the mapping positions `loadScale`'s 256-step table walk visits are
  checked (the check replays the walk). A map longer than about 128
  entries has positions the walk never reaches; upstream never reads them,
  so a bad degree there is accepted, as upstream.
- `convert_midi_note` is total. A `.kbm` middle note or a saved
  `scale_start_midi_note` is an arbitrary int, and upstream's
  `convertMidiNote` indexes `tuning_[kTuningCenter + note - start]`
  unchecked (`vital/src/common/tuning.cpp:367`). A start note outside
  0–128 puts some MIDI notes outside the 256-entry table; before this
  change the port panicked there (a `.kbm` with middle note 1000, or a
  state with start note 5000, loaded and then panicked on the first note).
  Refusing such start notes at load would refuse files upstream loads and
  plays correctly for every note that lands inside the table (a start note
  of 200 serves MIDI notes 72–127), so the port loads any start note and a
  note outside the table reads its nearest end. Notes inside the table read
  exactly what upstream reads. The offset is computed in `i64`, so no start
  note or note overflows.
- API: `load_file`, `from_file`, `load_scale` and `json_to_state` return
  `Result<_, TuningError>`. `TuningError` implements `Display` and `Error`.
  A scale of at most one entry gives a constant tuning and reads no mapping,
  so any mapping is accepted with it, as upstream.

**Tests** (`tuning::tests`):

- `keyboard_map_past_the_scale_is_refused`: a 12-key map naming degrees
  0–11 on a 7-degree Scala scale (8 entries) gives
  `MappingOutsideScale { degree: 8, scale_len: 8 }`. The note table, the
  saved state and the name are unchanged.
- `negative_keyboard_map_degree_is_refused`: a map with degree −1.
- `state_with_a_mapping_past_its_scale_is_refused`: the same through
  `json_to_state`.
- `shorter_scale_than_the_mapping_is_refused`: a valid map, then
  `load_scale` with a 4-entry scale.
- `keyboard_map_inside_the_scale_still_loads`: degrees 0–7 with `x` entries
  (degree 7, the period, is valid).
- `long_keyboard_map_checks_only_the_positions_read`: a 300-entry map
  (walk starts at position 172, wraps after 299, ends at 127) with degree 99
  at the unread position 150 loads; at the read position 172 it is refused
  and nothing changes.
- `keyboard_map_middle_note_far_outside_midi_converts_every_note`: middle
  notes 1000 and −1000 load, and every MIDI note reads the table's nearest
  end.
- `start_note_partly_outside_the_table_reads_inside_notes_exactly`: start
  note 200; notes 72–127 read the table exactly, notes below clamp to
  entry 0.
- `state_with_a_start_note_far_outside_midi_converts_every_note`: start
  notes ±5000 and `i32::MIN`/`MAX` through `json_to_state`, converting notes
  from `i32::MIN` to `i32::MAX` without a panic.

Parity is unchanged: no capture input indexes outside its scale (#24
corrected the one authored input that did) or puts a converted note outside
the table, and `cargo test -p sas-engine` passes against the official
captures.

**`migration/DIVERGENCES.md`** (#60 landed first, so this PR applies it):
- D6 is now "A keyboard map that indexes outside the scale is rejected at
  load": a mapping naming a degree outside the scale at a position
  `loadScale` reads is refused with `TuningError::MappingOutsideScale` and
  nothing is applied; unread positions of a long map are accepted, as
  upstream.
- D11 (new), "A note outside the tuning table reads the table's nearest
  end": upstream's `convertMidiNote` reads outside `tuning_` for a note more
  than 128 keys from the start note (undefined behaviour); the port clamps
  to the table. There is no defined output to reproduce, and refusing such
  start notes would refuse files upstream plays correctly for most notes.
