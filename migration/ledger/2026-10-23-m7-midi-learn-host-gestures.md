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
  set carries the value the parameter holds when the frame starts.
- **The host's echo** (UI Reviewer): with no audio running, nice-plug
  writes that value to the parameter (unchanged, so nothing reaches the
  engine). During playback it only calls the host's `performEdit`, and the
  host sends the edit back into the next `process`. If the value differs
  from the parameter's current one, the callback marks it and
  `sync_parameters` sends it to the engine. While a learned controller
  keeps moving, the echo can carry a value up to about a frame old, and the
  engine steps back to it until the next controller message; it settles on
  the final value when the controller stops. D16 says so, and the FL Studio
  checklist (`decisions/gui-framework.md`, item 3) asks the owner to listen
  for stepping and to check automation recording and undo with one gesture
  per frame.
- **The echo guard** (Graph Reviewer: upstream never steps back, so parity
  needs a guard, not just docs). All values are the normalized f32 bits
  nice-plug passes to the host, compared in that one representation:
  - `set_from_midi` keeps each control's last MIDI value (an atomic per
    control).
  - Before each forwarding `set`, the editor records the value it sends
    (`EditorModel::midi_move_forwarded` → `SynthParams::record_forwarded`),
    in a ring of four atomic slots per control, since a host block can be
    longer than an editor frame.
  - `set_from_midi` also stamps each control's last MIDI move with the
    processor's running sample count (Graph Reviewer: forwarded values never
    expire, since an echo of the current value fires no callback, and the
    host records exactly those values as automation).
  - At the start of a block, `sync_parameters` asks `take_stale_echo` about
    each host change. A change equal to a recorded forwarded value, within
    0.5 s of the control's last MIDI move (a few frames and a long host
    block), while the last MIDI value differs, is the stale echo: the slot is cleared, the
    parameter takes the last MIDI value back, `set_control` is skipped, and
    the control stays marked moved so the next frame forwards the current
    value. Any other host change is automation and applies; an echo equal
    to the last MIDI value changes nothing and so never arrives.
  - The audio side uses atomic loads, one compare-exchange per slot and
    stores; no locks, no allocation.
- D16 is narrowed to the window-closed case, and says what differs while it
  is open: the host hears of the move a frame later, once per frame, as a
  gesture where upstream sends a bare `setValueNotifyingHost`; and the
  host's echo can briefly step the engine back to a frame-old value.

**Tests**

- `plugin/tests/midi_learn.rs`, `the_hosts_late_echoes_never_step_the_engine_back`:
  a 90-block CC sweep with an editor frame every third block and the host
  echoing each forwarded value two frames late (over 20 stale echoes): no
  host `set_control` for the swept control ever reaches the engine, and the
  parameter ends at the last CC; an echo of the current value changes
  nothing; host automation nobody forwarded applies while the knob is idle.
  Removing the guard fails it at the first stale echo.
- `plugin/tests/midi_learn.rs`, `recorded_automation_of_forwarded_values_plays_back`
  (Graph Reviewer's reproduction): CC 40 and CC 80 are forwarded and
  echoed while current, then, more than the window later, the host plays
  the recorded CC 40 point back and it reaches the engine. Without the
  time bound the point is dropped.

- `plugin/tests/midi_learn.rs`: moves are kept only while an editor window
  is open (a new window opening before the old one closes keeps reporting;
  a close with none open doesn't go below zero), two moves of one control in a block report it once, moves while
  closed are not reported after reopening, and host edits are not learned
  moves.
- `gui/tests/controls.rs` (with the recording `GuiContextInner`): a moved
  control reaches the host as begin, set and end with its value unchanged,
  and the plugin is told the exact normalized value sent;
  nothing is sent when nothing moved; a move during a drag of the same
  control sends only the value.
- `plugin/tests/edit_bus_no_alloc.rs`: the editor is open for the whole
  counted run, and every third block it forwards the learned control while
  the host echoes the value from two frames earlier (over 900 echoes), so
  marking and the echo guard both run under the no-allocation check.

The window's open and close hooks (`App::build`, `Frame::on_close`) need a
real editor window and are not driven by a test; the model calls behind
them are.
