## 2026-10-22: dsp-core null source safe to hand across threads

**Why.** The plugin hands the engine graph to the audio thread
(`SoundEngineHost` in sas-plugin, #75, `unsafe impl Send`). dsp-core's null
source (`NULL_BUFFER`, upstream's `Processor::null_source_`) is a
thread-local `Rc`. Unconnected inputs return clones of it, and an
`Output::set_buffer` redirection to one used to keep that clone. This
happens in `ValueSwitch::set_buffer` and in `FiltersModule`'s routing when
the selected input is unconnected. A graph moved to the audio thread would
then hold, clone and drop the creating thread's `Rc`, with a non-atomic
reference count, from another thread. Graph Reviewer measured 0 such clones
held in today's graphs, so the problem was latent.

**Changed** (`port/crates/dsp-core/src/output.rs`)

- `Output` records a redirection to the null source as a marker (`None` in
  its buffer slot) instead of storing the thread-local `Rc`. `buffer()`,
  `at()` and `set_at()` resolve the marker to the null source of the
  calling thread. `buffer_is_owned()` is false for it, as before.
- No capture changes. The output still points at the null source, which
  reads zeros and is never written. Within a thread, every redirection
  still aliases the same storage, as with upstream's single static. Buffer
  sizes are unchanged too (`NULL_SOURCE_SIZE`).
- The docs of `NULL_BUFFER` and `Input::buffer()` state the contract: an
  unconnected input's buffer is a transient clone; use it, drop it or pass
  it to `Output::set_buffer`, but never keep it in a graph. The only
  places that keep buffers are `Output::set_buffer` calls (`ValueSwitch`,
  `FiltersModule`), and no kernel stores a `Buffer`.
- `Buffer` stays `Rc<[Cell<PolyFloat>]>`, so no API changes for other
  crates.

**Test.** `tests/graph.rs::null_source_redirection_holds_no_thread_local_buffer`
points a `ValueSwitch` at an unconnected input and checks that:
- the null buffer's strong count is unchanged, so the output holds no
  clone (on the previous code it is 3 instead of 2);
- the redirection reads zeros at `NULL_SOURCE_SIZE`;
- after the whole graph is handed to another thread (as `SoundEngineHost`
  does), the output and the unconnected input read that thread's null
  source, and switching back and forth works there;
- back on the first thread, the output reads that thread's null source
  again.

The test fails on the previous code.

**Parity.** No numeric change. The workspace suite passes, including
`FiltersModule` (filters) and the effects modules against fresh local
captures. So does `sas-dsp-core` under `aarch64-semantics`.
