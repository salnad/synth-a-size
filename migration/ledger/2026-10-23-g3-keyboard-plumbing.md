## 2026-10-23: G3 on-screen keyboard plumbing (engine and plugin)

The audio-side half of G3's on-screen keyboard; GUI Builder draws the
keyboard against the `EditorModel` hooks below. Ports JUCE's
`MidiKeyboardState` as upstream uses it: `MidiKeyboard` calls `noteOn` and
`noteOff` on channel 1, and `SynthPlugin::processBlock` merges the queued
notes into the block's MIDI (`processKeyboardEvents`, then
`MidiManager::replaceKeyboardMessages`, then `processNextMidiBuffer`).

**Changed**

- `sas_engine::keyboard` (new): `keyboard()` returns a `KeyboardHandle`
  (editor side, `Clone + Send + Sync`) and a `KeyboardConsumer` (audio
  side).
  - `note_on(note, velocity)`: velocity 0 to 1, made a MIDI byte as
    `floatValueToMidiByte` does (JUCE's `roundToInt` rounds half to
    even). Marks the note held on channel 1 at once and queues the message
    with a millisecond timestamp.
  - `note_off(note)`: queued only while the note is held on channel 1, from
    the editor or the host (JUCE's `isNoteOn` guard).
  - `held_notes() -> u128`: bit n while note n is held on any channel
    (`isNoteOnForChannels(0xffff, n)`), read live from per-note atomic
    channel masks (`noteStates`).
  - The consumer keeps `eventsToAdd`: time-ordered, each new message after
    those with the same time, messages more than 500 ms older than the one
    added dropped. It places them as `processNextMidiBuffer` does:
    position = round((t - first) * n / (last + 1 - first)), clamped to the
    block. Host note on, note off (or note on at velocity 0) and all notes
    off (CC 123) update the held notes.
- `BlockProcessor` collects the keyboard's messages every block, schedules
  them over the host block unless bypassed or empty (upstream returns
  before `processKeyboardEvents` when bypassed and skips it for an empty
  block, so they wait), and merges them in the chunk loop, each after any
  host message at the same sample (`MidiBuffer::addEvent`). It copies the
  held notes into the telemetry snapshot before the engine publishes.
- `TelemetrySnapshot::held_notes()` and `note_held(note)`.
- `EditorModel::note_on`, `note_off` and `held_notes` (no-op defaults, the
  same signatures as GUI Builder's G3 branch); `PluginModel` forwards them
  to the keyboard. `SynthPlugin::keyboard_handle()` hands it out.

- The handle sets a note's bit before queueing its note on, and clears it
  before queueing its note off (the clear is the guard, so two releases
  queue one note off). It puts the bit back if the queue refuses the
  message. JUCE does both under one lock; this order means the audio side
  never plays a note the editor doesn't show.

**Divergence (not audible):** the queue holds 256 messages, where JUCE's
`MidiBuffer` grows. It drains only when the host calls `process`. While
the host plays, 256 clicks in half a second fill it (the 500 ms window
applies once collected). While the host isn't calling `process` (stopped
or deactivated), messages pile up, and the 257th click since the last
block is refused: the key doesn't light and nothing plays. Upstream's
`eventsToAdd` would keep only the last 500 ms. Refusing rather than
allocating keeps the audio thread allocation-free.

**Tests**

- `sas-engine` `keyboard` unit tests: the timestamp spread, position and
  velocity rounding (half to even), the note-off guard (including a host
  note on channel 1), held notes from host messages and CC 123, the 500 ms
  drop while messages wait, and a full queue (a refused message leaves the
  held notes as they were).
- `sas-plugin` `tests/keyboard.rs`:
  - keyboard and host notes at the same sample, across 128-sample chunks:
    the host's go first;
  - notes wait through bypassed and empty blocks;
  - the held notes in the snapshot, from the keyboard and host MIDI;
  - the keyboard's own messages don't touch the held notes on the audio
    side: a host note off in the block releases a note the keyboard
    pressed before it, as in `processNextMidiBuffer`;
  - a keyboard note renders bit-identical to the same note from the host.
- `sas-plugin` `edit_bus_no_alloc`: a thread now plays the keyboard while
  the audio callback is counted; still no allocation or free.
- `editor::keyboard_notes_reach_the_keyboard`.
- With no keyboard notes nothing changes: the plugin parity and render
  tests pass unchanged.
