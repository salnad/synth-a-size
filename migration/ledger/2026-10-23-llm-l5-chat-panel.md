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
    API key, keyboard focus). It opens from a header button and draws over
    the right of the editor, as upstream's overlays do, until G7's drawer
    exists.
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
    ends the message after the model call in flight.
  - Saving settings retires the worker: it stops after its model call, and
    the next message starts a new conversation with the new provider and
    key. The synth keeps its changes. A retired worker no longer updates
    the view.
  - Dropping the plugin stops the worker and waits up to 2 s for it to
    finish (a host may unload the library afterwards); a model call in
    flight cannot be interrupted.
  - `keys`: the API key lives in the macOS Keychain (`security-framework`,
    service `synth-a-size chat`, account = provider). `SAS_LLM_API_KEY` or the
    provider's own variable (`ANTHROPIC_API_KEY`, `OPENAI_API_KEY`) takes
    precedence, for development. The key is read once per provider and
    cached, not read every frame.
  - The HTTPS transport (`sas-llm-agent`'s `http` feature) and the Keychain
    are macOS-only dependencies. The Linux build (CI) has no network code;
    there the chat reports that it runs in the macOS plugin.
- `sas-llm-agent`:
  - `Agent::cancel_flag` for Stop.
  - `Transport: Send`, so the agent can move to the worker thread.
  - `ChatSettings` gains `keyboard_focus` (`grab` by default) and
    `preset_dir`. The preset tools use `~/Music/synth-a-size/Presets`
    until the user picks a folder.
- `migration/DIVERGENCES.md` D13 extended.

**Measured**

- `cargo test -p sas-plugin --test chat`: 9 tests, with a fake
  `GuiContext` and scripted provider replies:
  - off by default, sending nothing;
  - a missing key explained;
  - a message that sets controls, then undo;
  - a volume raise held until confirmed;
  - settings saved with the key kept out of the file;
  - the key store read once over 100 frames;
  - non-ASCII tool summaries cut safely;
  - a settings change starting a new conversation;
  - dropping the plugin not hanging.
- `cargo test -p sas-gui --test chat`: 6 `egui_kittest` tests (the header
  button, Enter to send, Send/Stop while busy, undo/A/B/confirm reaching the
  host, the settings form while off, the focus setting).
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
  only changes where `ChatPanel::show` draws.
- Not tested here: the panel inside FL Studio on macOS (typing, a real
  provider, the Keychain prompt). That is the hands-on check for this PR.
- `set_preset_info` still reloads the state through the host. It switches
  to Edit Bus's `EditHandle::set_info` once that PR lands.

**Blocked:** nothing. Merge waits for #100, which waits for #92 and #95.
