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
- `EditorModel::tuning_name` and `EditorModel::load_tuning` in `sas-gui`,
  implemented by `PluginModel`. A full queue drops the load, leaving the
  old name showing.

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

No change to the audio path when no tuning edit arrives; no-edit parity is
untouched.
