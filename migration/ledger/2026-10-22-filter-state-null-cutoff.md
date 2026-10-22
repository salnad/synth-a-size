## 2026-10-22: FilterState keeps no null-buffer Rc for an unconnected cutoff

**Why.** Graph Reviewer follow-up to #77. #77 says no graph holds a clone of
dsp-core's thread-local null buffer. But `FilterState::load_settings` stored
`Some(cutoff.buffer())` on the filter, so a filter processed with an
unconnected cutoff kept that clone until its next block. This is not live in
the engine, where every filter's cutoff is connected. Non-blocking.

**Changed** (`port/crates/filters/src/synth_filter.rs`)

- `FilterState::midi_cutoff_buffer` is now a `MidiCutoffBuffer`:
  `Unloaded` (before `load_settings`, upstream `nullptr`), `Null` (the null
  source), or `Source(buffer)`.
- `load_settings` records `Null` when the cutoff's buffer is the null
  buffer. It compares pointers rather than calling `is_null_source`,
  because a connected output can itself be redirected to the null source
  (`ValueSwitch`).
- `midi_cutoff_at` reads zeros for `Null`, with the null source's bounds,
  and still panics on `Unloaded`.
- New `FilterState::midi_cutoff_buffer()` returns the buffer for loops
  that index it directly (`DigitalSvf`, `PhaserFilter`). For `Null` it is
  a transient clone of the calling thread's null buffer, dropped at the end
  of the block.
- No capture changes. An unconnected cutoff still reads zeros.

**Test.** `tests/null_cutoff.rs` runs a lone filter with an unconnected
cutoff for 8 blocks. It covers `DirtyFilter`, which reads through
`midi_cutoff_at`, and `DigitalSvf` and both `PhaserFilter`s, which take the
transient `midi_cutoff_buffer()` path (Graph Reviewer). It checks that:
- the null buffer's strong count is unchanged afterwards (on the previous
  code it is 3 instead of 2);
- the output is nonzero, finite, and bit-identical to the same filter with
  its cutoff plugged into an all-zero audio-rate output.

The test fails on the previous code.

**Parity.** No numeric change. The full `sas-filters` suite passes against
fresh local captures, official and no-fast-math.
