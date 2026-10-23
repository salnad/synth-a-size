## 2026-10-23: LLM L4, the tools on the running plugin

**Why.** L4 of the owner-approved LLM plan: the LLM tools' session on a
running plugin, built on the G0 edit bus (#94), with undo, A/B compare,
detection of changes made outside the chat, and the off-by-default chat
setting that D13 promises. Stacked on #95 (L3). The editor does not start a
chat yet; L5 adds the chat panel on G1's shell.

**Changed**

- `sas-plugin`:
  - `live::LiveSession` implements `sas_llm_control::SynthSession` the way
    the editor changes the synth:
    - a control changes as one host gesture (begin, set, end) through a
      `HostWriter`, which is `GuiContext` in the plugin, so the host can
      record automation and saves the value;
    - a route, LFO shape or wavetable changes through the edit bus. A created
      slot's final values (upstream's drag values, with the model's explicit
      amount, stereo and bypass on top) travel with the connection
      (`EditHandle::connect_modulation`), and its `parameter_changes` go to
      the host parameters, as #94 asks: no separate sets after a
      connection. Only an explicit power goes after, since it is not a slot
      value. Wavetables are rendered before their batch
      (`PreparedWavetable::render`).
    - A preset file, the init patch, or a change of save info goes through
      `GuiContext::set_state`, as the editor loads a preset.
    - A full queue is `Error::Busy` with nothing changed; the chat worker
      retries every 10 ms up to 50 times (the GUI thread never sleeps).
  - Undo and A/B (`restore`): controls return as gestures, and LFO shapes,
    remap curves and wavetables as one bus batch, which applies within one
    block. A different routing, sample or save info reloads the target state
    whole through the host.
  - Outside changes: the session remembers the values the tools set. A
    control that later reads differently (host automation, or the user
    turning the knob) is reported once (`take_outside_changes`). A
    generation change the session did not cause (a host restore, or a preset
    loaded in the editor) drops the undo history (`take_replaced`). The
    session's own reloads are counted, so they keep the history.
  - `chat_bridge`: `GuiContext` only works on the GUI thread, so the chat
    worker drives the session through `ChatBridge`. The worker's
    `RemoteSession` queues each call, the editor's frame loop runs it
    (`run_pending`, woken by a repaint request), and the worker waits. Each
    call has an atomic state (queued, running, cancelled, done) changed by
    compare-exchange: the GUI runs a call only from queued, and a worker
    whose wait times out cancels it only while queued, otherwise it waits
    for the running call's result. So no edit is applied while the worker
    reports it failed, and a call the editor never picks up (window closed)
    is dropped instead of running late.
- `sas-llm-control`:
  - The offline session uses `sas-preset`'s shared rule
    (`PresetState::dragged_values`, `connect_modulation_from_gui`), and the
    local copy from #92 is deleted. `session::created_slot_values` merges
    the model's explicit settings into the drag values for both sessions.
    The golden preset is unchanged.
  - `SynthSession::snapshot` is fallible. A remote session whose editor
    does not answer fails, and undo steps, checkpoints and batch rollbacks
    are never built from a stale copy.
  - `Error::Busy`: the synth cannot take an edit now and nothing changed.
  - `SynthSession` gains `take_outside_changes` and `take_replaced`, with
    empty defaults. `Controller` gains `check_replaced` (on each turn, undo
    and compare) and `compare_toggle` / `comparing` (A/B against the state
    before the last undo step; an edit or a new turn ends the comparison on
    the side playing).
- `sas-llm-agent`:
  - Each message tells the model about outside changes ("don't fight
    automation") and about a state replaced outside the chat.
  - `settings::ChatSettings` (`chat.json` in
    `~/Library/Application Support/synth-a-size`, or
    `~/.config/synth-a-size` elsewhere): `enabled` is false by default; a
    missing or damaged file leaves the chat off; `provider_config` refuses
    while the chat is off or a hosted provider has no key. The key is never
    stored there.
- `migration/DIVERGENCES.md` D13 extended.

**Measured**

- `cargo test -p sas-plugin --test live_session`: 13 tests (gestures and bus
  edits, live matching offline for the same calls, undo without a reload,
  undo of a route by reload keeping the history, A/B, outside changes, host
  restore, preset loads, the bridge across threads, timeouts dropping stale
  calls, a slow call awaited past the timeout, busy retries, an unreadable
  patch). `sas-llm-control` and `sas-llm-agent`: 64 tests, including 4 for
  the settings and 3 for A/B and replacement. All `sas-plugin` tests pass.
- Clippy `-D warnings` and rustfmt are clean for the workspace, and for
  `sas-llm-agent` with `http`.

**Decisions and follow-ups**

- Renaming the patch (`set_preset_info`) reloads the state through the host,
  because the edit bus has no save-info edit and the name must reach the
  saved state. A mirror-only save-info edit on the bus would avoid the
  reload; that has been asked of Edit Bus.
- A wavetable is rendered on the GUI thread, since the session runs there.
  If FL Studio testing in L5 shows a visible hitch, the render moves to the
  worker and the bridge carries the prepared wavetable.
- The chat panel (L5) reads `ChatSettings` and starts nothing while it is
  off, so the plugin then behaves exactly as without the layer.

**Review (#100).** Parity Reviewer approved at 8ecd3ed with two text
fixes, applied in aa786cd: D13 says "no key for a hosted provider", since a
keyless local server is allowed, and the settings doc says the Keychain
arrives in L5. Graph Reviewer found two blocking items, fixed:

- A bridge race: the GUI checked a deadline before running a call, but a
  call could finish after the worker had given up. With a 200 ms timeout, a
  call picked up at 50 ms and a 300 ms apply was applied while the worker
  returned an error. This is fixed by the atomic call state above, and
  pinned by `a_call_that_starts_before_the_timeout_is_awaited_not_abandoned`.
- `RemoteSession::snapshot` returned the last or init patch on a timeout,
  which undo checkpoints and batch rollback then used. It is now fallible
  (`an_unreadable_patch_blocks_edits_instead_of_saving_a_stale_undo_step`).

Also taken (non-blocking): the backoff on a full queue moved from the GUI
thread to the worker (`busy_edits_are_retried_on_the_worker`). The comment
on `take_replaced` records its edge case: a host restore landing while
one of the session's own reloads is pending counts as the session's own.

**Blocked:** nothing. L5 needs G1's shell (merged in #93).
