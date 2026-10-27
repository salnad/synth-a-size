## 2026-10-27: Edit bus: audition-only wavetable swaps (`audition_wavetable`)

The second piece G5e-3 needs for the audible drag. Upstream's editor
re-renders the whole table on each change during a drag, so the frames the
drawn keyframe interpolates into change as it is drawn, not only the drawn
frame. `set_wavetable_frame` covers the drawn frame at once. This adds a
whole-table swap that a worker can render (about 51 ms) and swap in during
the drag. Both are auditions: the drag reaches the document once, in the
commit on release, as under D18.

**Port** (`port/crates/engine/src/edit_bus.rs`)

- `EditHandle::audition_token(oscillator)` and
  `audition_wavetable(oscillator, creator, token)`:
  - The creator is rendered in full on the calling thread, then swapped into
    the shared table and, through the oscillator's existing wavetable slot,
    the engine's. The audio side is the existing table swap, which doesn't
    allocate.
  - The document keeps its creator, nothing is kept for a batch to roll
    back, and the revision doesn't move, so there are no undo entries, no
    document change for a host save to pick up, and no host writes.
  - The token holds the state's generation and the oscillator's commit
    count (`Batch::set_prepared_wavetable` bumps it). An audition whose
    token is out of date returns false and changes nothing, so a render
    that finishes after the drag's commit, or after a restore or preset
    load, can't replace it. Another oscillator's commit doesn't make it
    stale.
  - A frame drawn after an audition lands on the new table (the frame slot
    is separate and ordered). The audition's creator may be older than the
    drawn frame, so the editor sends the current frame again after one.
- `set_wavetable_frame` is now an audition too: it takes the drag's token
  and returns false, changing nothing, when it is stale, so a frame from a
  drag before a restore or commit can't reach the new table. It no longer
  writes the creator into the document or moves the revision. Its doc says
  to drop any table from `wavetable()` before calling, since a held one
  makes the shared table copy (Graph Reviewer's note on #178).
- `EditHandle::end_audition(oscillator, token)`, for a drag that isn't
  committed (cancelled, or the editor closed or failed during it): renders
  the document's creator and swaps it into the shared table and the
  engine's, so what plays is again what a save stores. It leaves the
  document and revision alone and does nothing when the token is stale (a
  commit or restore has already replaced the table). It advances the
  oscillator's count, so the drag's token goes stale and an audition or
  frame still in flight (a worker's render at the cancel) can't swap the
  auditioned table back in. The editor calls it on cancel, close and
  failure.
- `wavetable()`'s lazy render after a restore keeps a table an audition
  stored while it rendered, instead of overwriting it.
- `EditorModel::content_revision`'s doc says auditions don't move it.
- A failed commit batch keeps its commit count increment; that only makes
  auditions in flight stale.
- `push` is split into `queue` (no revision bump) and the revision bump.

**Tests** (`port/crates/engine/tests/edit_bus.rs`, `edit_bus_no_alloc.rs`)

- `an_audition_swaps_the_whole_table_without_touching_the_document`: the
  shared and engine tables become the full render of the creator, the
  document and revision stay, a frame drawn after it lands on the new
  table, and the release's `set_wavetable` then takes the document. Nothing
  leaks.
- `stale_auditions_change_nothing`: after the oscillator's commit, or a
  restore, an audition, a drawn frame and an end of audition each return
  false and queue nothing, and after the restore the table is the new
  document's; another oscillator's commit doesn't make it stale; a bad
  index is refused.
- `ending_an_audition_plays_the_document_again`: after an audition and a
  drawn frame, `end_audition` makes the shared and engine tables
  bit-identical to the committed one, with the document and revision
  unchanged; a late audition, frame or second end with the drag's token
  then returns false and leaves the tables as the document's; and the
  synth plays bit for bit like one that only ever had the committed
  table.
- The frame tests now check that the document keeps its creator and the
  revision stays (they checked the opposite under #178's first contract).
- The allocation test's producer also auditions a whole table every 40
  rounds, and ends every other audition, while the synth plays.

No audio change until the editor uses it (G5e-3).
