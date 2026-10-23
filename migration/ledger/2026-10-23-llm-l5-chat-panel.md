## 2026-10-23: LLM L5, the chat panel in the editor

**Why.** L5 of the owner-approved LLM plan: a chat panel on G1's editor shell
(#93) that drives the running synth through L4's live session. Stacked on
#100 (L4). The chat is off until the user turns it on (D13), and the way its
text field gets the keyboard is a setting, since the FL Studio focus check is
still pending.

**Changed**

- `sas-gui`:
  - `chat::ChatPanel` draws the conversation (user, assistant, tool and
    note lines), a text field (Enter sends, Shift+Enter makes a new line),
    Send/Stop, Undo, A/B with the side playing, Yes/No for an edit held for
    confirmation, and a settings form (on/off, provider, model, base URL,
    API key, keyboard focus).
  - The toggle is our logo section (D14's product-name area), with a small
    "chat" caption under the name. Until G7's drawer exists, the panel
    overlays the editor's right side, below the header and inside the
    editor's letterboxed bounds. `Interface::chat_placement` derives both
    from the layout, so they scale and letterbox with the editor; the
    panel's text and spacing follow the editor's scale (clamped to 0.5–2,
    in whole pixels). Recorded in `gui-plan.md` §4 and D13/D14.
  - The panel is a skin-coloured `egui::Area` (`ChatStyle`, from the
    Overlay section: Body, Border, Body Rounding, Body Text, the Text
    Editor colours, the UI Button colours and Widget Primary 1), not a
    stock `egui::Window`. The message field and a held edit's buttons sit
    in a bottom panel, so they always fit; the conversation scrolls above.
  - Plain Enter is taken before the field sees it, so a send never embeds
    a newline; Shift+Enter still adds one.
  - `examples/chat_gallery.rs` renders the panel over the editor offscreen
    (off, a conversation with a held edit, the settings form, a 350×205
    window and a letterboxed 1400×700 one), as `gallery` does for the
    editor.
  - The panel only draws and forwards clicks. The plugin's `ChatHost` runs
    the chat, so `sas-gui` does not depend on the LLM crates.
  - `FocusMode`: `GrabOnClick` (the default) asks the plugin window for
    keyboard focus when the field is clicked; `Plain` leaves focus to the
    host.
  - `EditorModel` gains `attach` (the editor's `GuiContext` and a wake
    function, given when the window opens) and `chat`, both with empty
    defaults.
- `sas-plugin`:
  - `chat::PluginChat` is the `ChatHost`. It lives as long as the plugin
    instance, so the conversation survives closing the editor window; it is
    never saved. While the chat is off it starts no thread and sends
    nothing. The first message starts a worker thread running the agent
    (`sas_llm_agent::Agent`) on a `RemoteSession`. The editor runs the
    queued tool calls on L4's `LiveSession` every frame.
  - Every command (send, undo, A/B, confirm) marks the chat busy until the
    worker has run it, so the panel disables its buttons meanwhile. Stop
    ends the message after the model call in flight. Each message carries
    its own cancel flag (the agent no longer clears it), so a Stop pressed
    before the worker starts the message is kept.
  - Each worker has its own `ChatBridge`. `ChatBridge::close` drops the
    queued calls, so a worker waiting on one wakes at once with an error,
    and refuses later calls. The worker loop has a shutdown flag of its own,
    separate from the messages' cancel flags.
  - Saving settings retires the worker: its bridge closes (no further tool
    call reaches the synth), it stops after its model call, and the next
    message starts a new conversation with the new provider and key. The
    synth keeps its changes. A retired worker's lines and state never reach
    the view.
  - `SynthPlugin::new` and `in_memory()` (the constructors tests use)
    build `PluginChat::in_memory`: no settings file, a key store that holds
    nothing, no transport. Only `Default`, which hosts use, reads the
    user's settings file and the Keychain (`PluginChat::new`).
  - While the chat is off, the key store is never read: neither for the
    provider nor for the settings form's "stored" hint.
  - Dropping the plugin closes the worker's bridge, sets its shutdown and
    cancel flags, and waits up to 2 s for the thread to end, because a host
    may unload the library afterwards. A worker waiting on the editor (whose
    window is closed by then) ends at once. **Residual:** a model request in
    flight cannot be interrupted and holds the thread until it answers or
    the transport's 120 s timeout. If a host unloads the library within
    that window, the thread is still running plugin code. Bounding it needs
    a shorter timeout (long answers can take a minute) or an abortable
    request; that is a follow-up.
  - `keys`: the API key lives in the macOS Keychain (`security-framework`,
    service `synth-a-size chat`, account = provider). `SAS_LLM_API_KEY` or the
    provider's own variable (`ANTHROPIC_API_KEY`, `OPENAI_API_KEY`) takes
    precedence, for development. The key is read once per provider and
    cached, not read every frame.
  - The HTTPS transport (`sas-llm-agent`'s `http` feature) and the Keychain
    are macOS-only dependencies. The Linux build (CI) has no network code;
    there the chat reports that it runs in the macOS plugin.
- `sas-llm-agent`:
  - `Agent::cancel_flag` and `set_cancel_flag` for Stop. `send` no longer
    clears the flag; the caller owns it.
  - `Transport: Send`, so the agent can move to the worker thread.
  - `ChatSettings` gains `keyboard_focus` (`grab` by default) and
    `preset_dir`. The preset tools use `~/Music/synth-a-size/Presets`
    until the user picks a folder.
- Merged main (#98, G2) into this branch: `editor.rs`, `interface.rs` and
  `EditorModel` keep #98's host tracking and close handling, with `attach`
  and the chat added beside them.
- `migration/DIVERGENCES.md` D13 extended, D14 notes the toggle.
- `decisions/gui-framework.md`: FL Studio checklist item 4, the Keychain
  prompt.
- `THIRD_PARTY_NOTICES.md`: Unicode-3.0 (ICU4X crates through `ureq`'s
  `url`/`idna`, macOS build only).

**Measured**

- `cargo test -p sas-plugin --test chat`: 14 tests, with a fake
  `GuiContext` and scripted provider replies:
  - off by default, sending nothing;
  - a missing key explained;
  - a message that sets controls, then undo;
  - a volume raise held until confirmed;
  - settings saved with the key kept out of the file;
  - the key store read once over 100 frames;
  - non-ASCII tool summaries cut safely;
  - a settings change starting a new conversation;
  - dropping the plugin not hanging;
  - an off chat never reading the key store;
  - the in-memory chat staying off and keeping no keys;
  - dropping the plugin while the worker waits on the editor, which ends
    the thread well within the 2 s cap;
  - a Stop right after sending not being lost;
  - a retired worker staying out of the new conversation, its tool call
    unapplied.
- `cargo test -p sas-plugin --test live_session`: a new bridge test
  (`closing_the_bridge_wakes_a_waiting_call_and_refuses_later_ones`).
- `cargo test -p sas-gui --test chat`: 7 `egui_kittest` tests (the logo
  toggle, Enter to send, Enter mid-text and Shift+Enter, Send/Stop while
  busy, undo/A/B/confirm reaching the host, the settings form while off,
  the focus setting).
- `cargo test -p sas-gui --test controls`: two composed-editor tests. The
  toggle and panel stay clear of upstream's header components and inside
  the editor at 1400×820, 700×410, 350×205, 1400×700 and 1000×820. With
  `Interface` and `ChatPanel` composed as `App::ui` does, the tabs still
  switch, a drag on the portamento time knob under the open panel reaches
  no parameter, and the same drag with the panel closed does.
- `sas-llm-agent`: a new replay test (`a_cancelled_message_stops_after_the_call_in_flight`).
- Clippy `-D warnings` and rustfmt are clean for the workspace. Clippy is
  also clean for `sas-plugin` built for `aarch64-apple-darwin` (Keychain
  and HTTPS code included), and for `sas-llm-agent` with `http`.
- Workspace tests pass, except parity tests that need local no-fast-math
  reference captures. CI renders those captures.

**Decisions and follow-ups**

- The FL Studio keyboard-focus result is pending, so the approach is a
  setting. When it is known, the default can change or the setting can be
  removed.
- The panel is an overlay until G7 builds the right-side drawer. Moving it
  only changes `ChatPlacement::panel`.
- Two `GuiContext` users now share the parameters: the knobs and the live
  session. A test of a knob drag while a chat edit is queued is worth
  adding on the LLM side (UI Reviewer's suggestion).
- Not tested here: the panel inside FL Studio on macOS (typing, a real
  provider, the Keychain prompt). That is the hands-on check for this PR.
- `set_preset_info` still reloads the state through the host. It switches
  to Edit Bus's `EditHandle::set_info` once that PR lands.

**Review (#101).** Parity Reviewer requested changes at 0c6b1e5:
`SynthPlugin::new`, behind both the test constructors and `Default`, built
`PluginChat::new`, so every plugin test read the real `chat.json` and could
reach the Keychain, undoing #99's hermetic tests. Fixed as asked: `new` and
`in_memory()` use the in-memory chat, and only `Default` uses the user's
files. This follows #99's constructor pattern (`new` / `in_memory` /
`Default`), so the two PRs conflict only mechanically in `lib.rs`. The 16
test sites that built the plugin through `Default` (`tests/state.rs`, 15;
`tests/params.rs`, 1) use `SynthASize::in_memory()`, the same conversion #99
makes. Whichever of #99 and #101 merges second takes the union:
`new`/`in_memory()` get in-memory MIDI learn and `PluginChat::in_memory`,
and `Default` gets config-file MIDI learn and `PluginChat::new`. Also
taken (non-blocking): `provider()` checks `enabled` before reading the key,
so an off chat never touches the Keychain.

Graph Reviewer requested changes at 0c6b1e5 on threading. Blocking: at
drop the editor is closed, so no bridge call runs, and a worker waiting on
one sat in 10 s timeouts while drop gave up at 2 s, leaving the thread
running plugin code (reproduced). Fixed with `ChatBridge::close`, a worker
shutdown flag separate from the agent's cancel flag, and a test with a
blocked worker; the in-flight HTTP residual is documented above. Also taken
(non-blocking): a Stop is no longer lost to the agent clearing its flag,
and a retired worker's lines stay out of the new conversation.

UI Reviewer requested changes at 0c6b1e5 on the GUI side. Blocking, all
fixed: the toggle covered upstream's oscilloscope at 1400×820 and hung
outside the header at small and letterboxed sizes (now in the logo
section, per the coordinator's decision); the panel used stock egui
visuals (now the Overlay skin colours); it overflowed the editor at
350×205 (now inside it at every size); and there was no test of the
composed editor (now two). Also taken (non-blocking): Enter is consumed
before the field, #98 is merged, the Keychain prompt is on the FL Studio
checklist, and Unicode-3.0 is in the notices.

**Blocked:** nothing. Merge waits for #100, which waits for #92 and #95.
