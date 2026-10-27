## 2026-10-27: Edit bus: a matrix row sets its own slot's route

**Changed.** The Matrix tab's source and destination menus (G4c-2) need
upstream's `ModulationMatrixRow::sliderValueChanged`. A row is bank slot N
for good. Choosing a source or destination disconnects the slot's old
route (`disconnectModulation(connection)`), and once both are set connects
the new one in that same slot (`connectModulation(connection)`), leaving the
slot's amount, polarity, stereo, bypass, power and remap curve as they are.
The port's connect and disconnect went by route and took the first free
slot, so this adds the slot-specific path.

- **Mirror:** `PresetState::set_slot_route(slot, Option<(source,
  destination)>)`.
  - It frees the slot, then names it.
  - It keeps the slot's controls and curve.
  - A route may repeat another slot's, as upstream's matrix allows.
  - A route to the slot's own amount is refused and leaves the slot free
    (`isInvalidConnection`).
  - It returns whether the slot is connected.
- **Bus:** `Batch::set_slot_route` and `EditHandle::set_slot_route` stage
  `DisconnectSlot` for the old route (with its names from the mirror), then
  `ConnectSlot`, in one batch. The audio thread applies a batch only once
  it has all of it, so the engine never plays the slot half-changed.
- **Engine:**
  - `Synth::connect_bank_slot` names bank slot N, then `connect_slot`, the
    existing `connectModulation(connection)` with its `isInvalidConnection`
    check.
  - `Synth::disconnect_bank_slot` frees slot N if it holds that route. The
    names come with the command, because the bank's `String`s would
    allocate to copy.
  - `EditTarget` gains `connect_slot` and `disconnect_slot`.
- **Editor model:** `EditorModel::set_slot_route`, with the plugin's
  implementation going through its edit handle.
  - It touches no host parameter, so the host hears nothing: upstream sets
    no control here either. D18 holds: one user action is one engine
    change and no partial host gestures.

- **Known quirk, ported as it is.** Upstream's `createModulationChange`
  counts the source's other audio-rate connections (`num_audio_rate`) only
  among those to a *different* destination. With two slots on the same
  route, rewiring one away counts the other as nothing, so the source
  drops to control rate although the other slot still uses it. That lasts
  until a reload. The port keeps this, and duplicates make it reachable
  from the matrix now. Recorded as I21 in `IMPROVEMENTS.md`, from Graph
  Reviewer's review of #185.

**Verified.**

- `a_matrix_row_rewires_its_own_slot_in_one_step` (`tests/edit_bus.rs`)
  checks, against the recorded target:
  - a first connect;
  - a source change arriving as disconnect and connect of slot 3 in one
    drain;
  - a route repeated in slot 5;
  - slot 4's amount untouched in the mirror;
  - the own-amount route refused, leaving only the disconnect;
  - freeing a slot;
  - an out-of-range slot and an unknown source rejected.
- `a_rewired_slot_plays_like_a_disconnect_and_connect`: rewiring slot 0
  from LFO 1 to Env 1 over the bus mid-note renders bit for bit like
  calling the engine's `disconnect_modulation` and `connect_modulation`
  directly (which reuse the freed slot), with the slot's amount kept. It is
  also audibly different from no change.
- `edit_bus_no_alloc`: the producer also rewires slot 41 every edit cycle
  while the engine plays. Nothing allocates on the audio thread.
- **Checks.** `cargo clippy --workspace --all-targets -D warnings` passes,
  as do the `sas-engine`, `sas-preset`, `sas-plugin` and `sas-gui` tests.
