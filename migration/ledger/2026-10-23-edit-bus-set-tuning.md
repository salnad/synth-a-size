## 2026-10-23: G6 prerequisite: tuning edits over the edit bus

For the Advanced tab's `TuningSelector` (G6c), which loads a Scala or
Tun file, or the default tuning, while the synth plays.

**Changed**

- `EditHandle::set_tuning(Tuning)` and `Batch::set_tuning(Tuning)`
  (`TuningSelector::loadTuning`, `SynthBase::loadTuningFile`): the mirror
  takes the tuning, and a new `Target::Tuning` slot carries it to the audio
  thread. Edits coalesce like the other payload slots, and a failed batch
  rolls the tuning back.
- `EditTarget::swap_tuning`, `Synth::swap_tuning`, `SoundEngine::swap_tuning`
  and `VoiceHandler::swap_tuning`: the handler swaps the tuning in place
  through `Rc::get_mut` (it holds the only reference), so applying the edit
  neither allocates nor frees on the audio thread. The old tuning goes back
  to the producer through the retire queue, as wavetables and samples do.
  `Synth` always leaves the handler an unshared tuning, so the allocating
  fallback debug-asserts (Graph Reviewer nit on #110).
- `EditorModel::tuning_name`, `EditorModel::tuning` and
  `EditorModel::load_tuning` in `sas-gui`, implemented by `PluginModel`.
  Upstream changes `SynthBase::tuning_` in place: a `.kbm` keeps the scale,
  a scale keeps the keyboard map, and `TuningSelector`'s built-ins call
  `setName` and then load a scale or `setDefaultTuning` on the same object.
  So the editor edits a copy from `tuning()` the same way and passes it
  back (Graph Reviewer on #110). A full queue drops the load, leaving the
  old tuning in place.

**Tests**

- `sas-engine` `edit_bus::tuning_edits_update_the_mirror_and_coalesce`:
  two loads bump the revision twice and reach the engine once, holding the
  newer tuning; a failed batch rolls back.
- `sas-engine` `edit_bus::a_bus_tuning_plays_like_one_set_in_place`: the
  corpus septimal scale over the bus renders bit for bit like
  `Synth::set_tuning`, and differs from the default tuning.
- `sas-engine` `edit_bus_no_alloc`: tuning loads join the edit stream the
  allocation-counting test applies.
- `sas-plugin` `edit_bus::edits_play_like_direct_engine_edits` adds a
  tuning load, and the saved document keeps it.
- `sas-plugin` `editor::a_loaded_tuning_names_the_tuning`.
- `sas-plugin` `editor::a_keyboard_map_layers_on_the_loaded_scale`: the
  corpus `.scl` and then its `.kbm` through `tuning()` and `load_tuning`
  match one tuning that loaded both, and differ from either alone. It
  fails with `tuning()` returning a fresh tuning.

No change to the audio path when no tuning edit arrives; no-edit parity is
untouched.
