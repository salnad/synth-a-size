## 2026-10-26: the edit bus sets a connection's values without the host

**Changed.** `EditHandle::set_connection_values(slot, SlotValues)` (and
`Batch::set_connection_values`) sets a connected modulation slot's amount,
bipolar, stereo and bypass in the engine, as upstream's
`SynthGuiInterface::setModulationValues` does through
`synth_->valueChanged`. It sets the controls of the running connection;
nothing is disconnected or rebuilt. GUI Builder now owns this command,
which drag-to-modulate (G4b-1) needs.

- **Why it travels on the bus.** While a source is dragged over a control,
  upstream connects it and updates its values live, and tells the host
  nothing. The port's slot values are host parameters, so the editor writes
  them to the host once, when the drag ends (D18). The engine needs them
  before then, and a mid-drag modifier key changing the polarity must not
  re-create the slot's processor.
- **Coalescing.** Each slot has its own pending target, newest wins, like
  the shapes. The payload is the `Copy` `SlotValues`, applied with
  `EditTarget::set_connection_values` (`Synth::set_connection_values` sets
  the slot's control values).
- **The mirror** takes the four values too, as a connection's do.

**Verified.**

- `connection_values_coalesce_and_update_the_mirror`: 100 sets before a
  drain reach the target once, with the newest values, after the
  connection and its linear curve. The mirror holds them, and slot 64 is
  refused.
- `connection_values_change_a_running_connection_in_place`: setting the
  values over the bus mid-note plays bit for bit like setting the four
  controls in place.
- `edit_bus_no_alloc`: the producer also sets a slot's values every round.
  Both tests pass, including `shapes_wavetables_and_samples_never_allocate`,
  which counts whole blocks.
- `cargo clippy --workspace --all-targets -D warnings`, `cargo fmt` and the
  `sas-engine` edit bus tests pass.
