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

Parity is unchanged: no capture input indexes outside its scale (#24
corrected the one authored input that did), and `cargo test -p sas-engine`
passes against the official captures.

**Open:** `migration/DIVERGENCES.md` D6 is updated to "rejected at load"
once #60 (the divergences file) is on main.
