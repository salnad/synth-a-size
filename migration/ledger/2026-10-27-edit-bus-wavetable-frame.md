## 2026-10-27: Edit bus: a drawn wavetable frame (`set_wavetable_frame`)

What G5e-3 needs so that drawing on a wavetable is heard during the drag,
as it is upstream. Upstream's editor renders the drawn frame into the
playing table (`WavetableCreator::render(position)`) on every change, and
the playhead pins the wave frame to that frame. On release it does the full
`render()`. In the port, the full render through `set_wavetable` takes
about 51 ms in release for a table with five keyframes and three modifiers,
which is too slow for every mouse move. This adds a one-frame commit.

**Port** (`port/crates/engine/src/edit_bus.rs`, `synth.rs`,
`port/crates/wavetable/src/wavetable.rs`)

- `EditHandle::set_wavetable_frame(oscillator, creator, position)`:
  - `creator` becomes the document's creator.
  - Frame `position` alone is rendered (`render_frame`, upstream's
    `render(position)`, without the full render's normalisation) into the
    shared table, as upstream renders into the live one. This happens on the
    calling thread, under the mirror lock.
  - The frame travels as data (`WavetableFrame`: the waveform and its three
    frequency arrays, about 33 KB) through a new pending slot per oscillator.
    The audio thread copies it over that frame of its own table
    (`Synth::set_wavetable_frame`, `Wavetable::write_frame`), and the payload
    comes back on the return queue, like the table swaps. Nothing on the
    audio side allocates or frees, and the table's version stays put
    because the frame count doesn't change.
- Newest wins: a frame replaces one for the same oscillator that the audio
  thread hasn't taken, even at another position. In the editor the
  positions are the playhead's, the same through a drag. The queue keeps
  frames in order against whole tables, so the full render on release
  lands after them.
- A position past the table's last frame changes only the creator, as
  `loadWaveFrame` ignores it. A restore drops frames queued before it, as
  it does every edit. Out-of-range oscillators and positions are refused
  and change nothing.
- `Wavetable::frame` and `write_frame`; `WavetableFrame` is exported from
  `sas_wavetable`.

**Tests** (`port/crates/engine/tests/edit_bus.rs`, `edit_bus_no_alloc.rs`)

- `a_drawn_frame_reaches_the_engine_and_the_shared_table`: the shared
  table's frame is `render_frame`'s on the old table, the other frames and
  the frame count stay, the document takes the creator, and the synth's
  table ends with the same frame. Nothing leaks.
- `drawn_frames_coalesce_and_keep_their_order`: two frames before a block
  arrive as the newer one, and a frame between two whole tables lands
  before the surviving whole table.
- `drawn_frames_respect_restores_and_bounds`: a restore drops a queued
  frame; a frame past the end changes only the creator and moves the
  revision; bad indices are refused without moving it.
- The allocation test's producer now also draws a frame every fourth round
  while the synth plays; the audio side still never allocates or frees.

No audio change until the editor uses it (G5e-3).
