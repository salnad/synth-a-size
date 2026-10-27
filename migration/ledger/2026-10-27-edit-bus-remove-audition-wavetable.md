## 2026-10-27: Edit bus: `audition_wavetable` removed

#180 added a whole-table audition (`EditHandle::audition_wavetable`) for a
background render during a wavetable drag, on the belief that upstream's
editor re-renders the whole table on each drag event. It doesn't:
`WavetableEditSection::frameChanged` calls `render(position)`, which renders
only the playhead's frame, and only `frameDoneEditing` (mouse up) calls the
full `render()`. `set_wavetable_frame` on each drag event and the
`set_wavetable` commit on release already match that, so the whole-table
audition has no caller and no upstream counterpart.

**Port** (`port/crates/engine/src/edit_bus.rs`)

- `audition_wavetable` is gone. `end_audition` keeps its swap (the private
  `swap_audition` folded into it) and still makes the drag's token stale.
- The docs of `set_wavetable_frame` and `audition_token` name the calls that
  remain.

**Tests** (`port/crates/engine/tests/edit_bus.rs`, `edit_bus_no_alloc.rs`)

- `an_audition_swaps_the_whole_table_without_touching_the_document` goes
  with the call it tested.
- `stale_auditions_change_nothing` and
  `ending_an_audition_plays_the_document_again` keep their frame and
  `end_audition` checks; the drawn frames stand in for the whole-table
  auditions they also checked.
- The allocation test still ends a drag every 80 rounds while the synth
  plays.

No audio change.
