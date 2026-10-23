## 2026-10-23: MIDI learn tells the host through the open editor (D16, option b)

Voice Engine. Follows #99. Salil accepted D16 (no host notification of
learned-controller moves) and asked for option b: while the plugin window is
open, the host hears of each move as a gesture through `GuiContext`.
Option c (from the audio thread, with a framework change) is parked as
`IMPROVEMENTS.md` I19.

**What changed**

- `plugin/src/params.rs`: `SynthParams::set_from_midi` also marks the
  control in a lock-free bit set while the editor is open
  (`set_editor_open`); `drain_midi_moved` hands the marked controls to the
  editor. It counts open windows rather than flagging one, because a host
  may build a new window before the old one's close arrives. The set is
  cleared when the first window opens and when the last one closes, so moves
  while no window was open are never replayed. The audio thread adds one atomic
  load and, while the editor is open, one atomic `fetch_or`; no locks, no
  allocation.
- `sas_gui::EditorModel`: `set_editor_open` and `drain_midi_moved`, with
  no-op defaults; the plugin's `PluginModel` forwards them to the
  parameters.
- `sas_gui` editor (`editor.rs`, `host.rs`): the window tells the model it
  opened (`build`) and closed (`on_close`). Each frame, before drawing,
  `Host::forward_midi_moves` sends one begin, set and end gesture per moved
  control with its current value. A control the user is dragging gets only
  the value, inside the drag's open gesture, so gestures never nest. The
  set carries the value the parameter already holds, so nice-plug applies
  no change and nothing reaches the engine twice.
- D16 is narrowed to the window-closed case, and says what differs while it
  is open: the host hears of the move a frame later, as a gesture where
  upstream sends a bare `setValueNotifyingHost`.

**Tests**

- `plugin/tests/midi_learn.rs`: moves are kept only while an editor window
  is open (a new window opening before the old one closes keeps reporting;
  a close with none open doesn't go below zero), two moves of one control in a block report it once, moves while
  closed are not reported after reopening, and host edits are not learned
  moves.
- `gui/tests/controls.rs` (with the recording `GuiContextInner`): a moved
  control reaches the host as begin, set and end with its value unchanged;
  nothing is sent when nothing moved; a move during a drag of the same
  control sends only the value.
- `plugin/tests/edit_bus_no_alloc.rs`: the editor is open for the whole
  counted run, so the marking path is covered by the no-allocation check,
  and the learned control is reported afterwards.

The window's open and close hooks (`App::build`, `Frame::on_close`) need a
real editor window and are not driven by a test; the model calls behind
them are.
