## 2026-10-23: LLM L3, providers, agent loop, `sas-llm` and replay tests

**Why.** L3 of the owner-approved LLM plan: the chat loop that lets a
model drive the tools from L1 and L2, with the provider configurable
(working default: Claude through Anthropic's Messages API, plus an
OpenAI-compatible adapter). CI must never call a model API. Stacked on #92
(L2). The plugin does not use any of it yet; L4/L5 wire it in behind a
setting that is off by default.

**Changed**

- New crate `port/crates/llm-agent` (`sas-llm-agent`):
  - `message`: a provider-neutral conversation (text, tool use, tool
    result), `Usage` with cache read and write counts.
  - `provider`: `Anthropic` (`POST /v1/messages`; the system prompt and
    the tool list carry `cache_control`, since they are the same on every
    call) and `OpenAiCompatible` (`POST {base}/chat/completions`; covers
    OpenAI, OpenRouter, Ollama and LM Studio). Model, base URL and key are
    configurable (`ProviderConfig`); `Debug` never prints the key.
  - `agent`: `Agent` runs one user message to completion. Each message
    opens with the current patch description, and each message is one undo
    step (`Controller::begin_turn`). Limits (`AgentConfig`, working
    defaults): 20 model calls per message, 2048 output tokens per call, 1M
    tokens per session, 8 earlier exchanges kept. Held edits are resolved
    with `resolve_pending`, and the model is told at the start of the next
    message; so is an undo the user makes outside the chat.
  - `prompt`: the system prompt, with guidance on working in display
    units and through modulation, the modulation sources, and the L1
    catalog. It does not name the upstream synth (working default; pinned
    by `the_prompt_names_no_upstream_product`). About 15 KB.
  - `transport`: `ReplayTransport` (each request must match the recording's
    conversation, model and tool names), `RecordingTransport` (writes only
    that comparable part plus the response, never headers or keys),
    `ScriptedTransport`, and `HttpTransport` behind the `http` feature.
  - `eval`: checks for the 21-prompt eval set in `evals/sound_design.json`
    (control ranges in display units, routes, edited LFO shapes and
    wavetables; one prompt checks that "+5 dB louder" is held for
    confirmation). The checks are unit tested; running the set needs a real
    model (`sas-llm --eval`), so it is not part of CI.
  - `sas-llm` (requires `http`): a chat REPL or `--prompt` one-shots on a
    preset file, with `/patch`, `/undo`, `/yes`, `/no`, `/usage`;
    `--record` and `--replay`; `--save` to a `.vital` file;
    `--preset-dir` for the preset tools. The key comes from
    `ANTHROPIC_API_KEY`, `OPENAI_API_KEY` or `SAS_LLM_API_KEY`.
- Replay tests (`tests/replay.rs`, recordings in `tests/replays/`):
  an Anthropic session that edits and then undoes, and an OpenAI-compatible
  session where a volume raise waits for the user's confirmation. Also:
  a mismatched request fails, the step limit, the session token limit, and
  recordings hold no credentials. The recordings were written from authored
  responses in each provider's documented format
  (`SAS_LLM_REPLAY_WRITE=1`); no key was available, and none is needed. A
  recording from a real provider (`sas-llm --record`) replays the same way.
- `migration/DIVERGENCES.md` D13 extended for the provider client.

**Network and licences**

- HTTPS is `ureq` 2 with `native-tls`: Security.framework on macOS (the
  shipping target, no OpenSSL), OpenSSL 3 (Apache-2.0) from the system on
  Linux. Both are behind the `http` feature, off by default. The workspace
  build and CI compile no network code, so no test can reach a model API.
- The 54 packages it adds to `Cargo.lock` (optional) are MIT and/or
  Apache-2.0, plus 18 ICU crates (via `url`/`idna`) under Unicode-3.0, a
  permissive licence compatible with GPLv3.

**Measured**

- `cargo test -p sas-llm-agent`: 11 unit and 6 replay tests. Clippy
  `-D warnings` is clean with and without `http`. `sas-llm --replay` plays
  the Anthropic recording end to end.
- Recordings: 7.9 KB and 3.9 KB.

**Decisions**

- Recordings keep only what a replay compares, so a change to the system
  prompt or a tool description does not invalidate them, while a change in
  the conversation the agent sends does.
- The default model ids (`claude-sonnet-4-5`, `gpt-4.1`) are placeholders
  the user overrides; providers rename models often.

**Blocked:** nothing. L4 waits for G0 (#94), L5 for G1 (#93).
