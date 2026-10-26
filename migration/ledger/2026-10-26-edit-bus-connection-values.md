## 2026-10-26: the edit bus sets a connection's values without the host

**Changed.** `EditHandle::set_connection_values(source, destination,
SlotValues)` (and `Batch::set_connection_values`) sets the amount, bipolar,
stereo and bypass of the slot connecting the two, in the engine, as
upstream's `SynthGuiInterface::setModulationValues` does through
`synth_->valueChanged`. Like upstream's `getConnectionIndex` check, it
finds the slot from the mirror when called, under its lock. If the two
aren't connected, it returns `Ok(None)` and changes nothing, so a slot
another producer has freed or reused in the meantime is never written. It sets the controls of the running connection;
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
  connection and its linear curve. The mirror holds them, and an unknown
  destination is refused.
- `connection_values_for_a_disconnected_route_change_nothing`: after a
  disconnect, and again once another route has taken the freed slot, a
  call returns `None`. The mirror's slot values stay, and nothing reaches
  the engine.
- `connection_values_change_a_running_connection_in_place`:
  - The route is one the init patch plays (LFO 1 on oscillator 1's level),
    with stereo on and bypass off.
  - The bus edit mid-note changes the render, and plays bit for bit like
    setting the four controls in place.
  - Mutation checks: with `Synth::set_connection_values` emptied, or with
    stereo and bypass crossed, the test fails.
- `edit_bus_no_alloc`: the producer also sets a route's values every round.
  The route is connected every other round, so the coalesced payload's
  apply is counted in `connection_edits_never_allocate`. All three tests
  pass.
- `cargo clippy --workspace --all-targets -D warnings`, `cargo fmt` and the
  `sas-engine` edit bus tests pass.
