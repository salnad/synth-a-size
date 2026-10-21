## 2026-10-21: M8 PR 2, plugin tuning state (`sas-plugin`)

**Changed.** The plugin state's tuning was passed through as raw JSON. It is
now a real `sas_engine::Tuning`, using Voice Engine's port of
`Tuning::stateToJson` / `jsonToState` (M7 PR 2):

- `PresetDocument::tuning` is a `Tuning` (default: `Tuning()`).
  - The plugin state always carries `"tuning"`, as
    `SynthPlugin::getStateInformation` writes `getTuning()->stateToJson()`
    whatever the tuning.
  - Restoring applies `"tuning"` only when the state has one, as upstream's
    `setStateInformation` does.
  - The engine's `serde_json` value is converted through text, so the saved
    JSON uses sas-common's upstream-exact serializer.
- Preset files never change the tuning. `plugin_state_from_file` drops a
  `"tuning"` key, because `SynthBase::loadFromFile` loads only the preset.
  Preset files are still saved without one (`saveToFile`).
- `Engine` gains `set_tuning(&Tuning)` (`SoundEngine::setTuning` with
  `SynthBase::tuning_`). The processor calls it right after every
  `load_preset`, in `setStateInformation`'s order: preset, then tuning.
  `SilentEngine` ignores it. Whoever implements `Engine` on the real
  `SoundEngine` (Voice Engine's slice A) must implement it.
- `serde_json` moves from a dev-dependency to a dependency (`"1"`, already in
  the tree through nice-plug and sas-engine).

**Measured.** 35 `sas-plugin` tests pass. The new ones:
- `the_default_tuning_is_always_saved` pins the saved default tuning to
  upstream's dump of `Tuning()`, byte for byte;
- `tuning_round_trips_through_the_host_state` covers a 19-EDO Scala tuning at
  A4 = 432 Hz through capture → serde → restore, with an identical table,
  name and JSON;
- `states_and_preset_files_without_a_tuning_keep_the_current_one`;
- `a_tuning_that_is_not_an_object_rejects_the_state`.

The Steinberg validator on the Linux dev bundle still gives 537 passed,
0 failed.

**Deliberate difference (for `migration/DIVERGENCES.md` once it is on
main).** A state whose `"tuning"` is not an object is rejected whole, so
nothing changes, consistent with how the port treats an unloadable preset.
Upstream applies the preset, then `jsonToState` throws on the bad tuning and
`setStateInformation` reports the state as corrupted, keeping the preset.
Missing keys inside a tuning object follow `sas_engine::Tuning::json_to_state`,
which reads them as defaults.
