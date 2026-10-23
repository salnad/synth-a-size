## 2026-10-23: LLM follow-up, stoppable model requests

**Why.** The residual left in #101 (L5): a model request in flight could
not be interrupted. It kept the chat thread alive until the provider
answered or the 120 s timeout, after plugin drop's 2 s wait, and a host
unloading the library in that window would leave the thread running
plugin code. The user's Stop also had to wait for the request. A small
follow-up stacked on #102 (L6), as the coordinator asked.

**Changed**

- `sas-llm-agent`:
  - `stop::StoppableStream` wraps a connection and reads with a 250 ms
    socket timeout (`POLL`), asking a `StopCheck` between attempts. A
    stopped read fails at once with "the request was stopped". The old
    overall 120 s limit becomes an idle limit (`IDLE`: no bytes for
    120 s), so a slow answer still arrives.
  - `HttpTransport::stoppable(stop)`: HTTPS connections go through the
    stream (a `ureq` TLS connector over the system's TLS), with 10 s
    connect and write timeouts. A stopped request returns `AgentError::Stopped`, both
    while waiting for the response and while reading its body.
    `HttpTransport::new` keeps the old behaviour.
- `sas-plugin`:
  - The transport factory gets the worker's stop check, the same one the
    preview engine uses (`StopFlags`: the worker's shutdown, or the
    current message's cancel flag). The shipped transport is
    `HttpTransport::stoppable`.
  - So both the user's Stop and the plugin closing end a request within
    about 250 ms, and plugin drop's 2 s wait now covers a request in
    flight. A stopped request shows "(stopped by the user)" as a note, not
    an error.

**Measured**

- `cargo test -p sas-llm-agent --test stop`: 4 tests over real local
  sockets:
  - a stop ends a read the server never answers, within 2 × `POLL`;
  - an answer slower than the poll still arrives;
  - a quiet server hits the idle limit;
  - a stop before reading returns at once.
- `cargo test -p sas-plugin --test chat`: 2 new tests with a provider that
  never answers until stopped. Stop ends the message with the note, and
  dropping the plugin returns (with the thread ended) in well under 1 s.
- With `--features http`, a fifth test sends a stoppable HTTPS request to
  a local listener that accepts and never answers the TLS handshake (no
  TLS server, no real API) and checks it returns `Stopped` well under 2 s
  after a stop at 200 ms. It runs in the arm64 macOS VST3 job
  (`plugin.yml`), the only job that compiles `http`, so the Linux jobs keep
  building no network code. Graph Reviewer measured 188 ms from stop to
  return against a silent TLS listener.
- Clippy `-D warnings` and rustfmt are clean for the workspace, for
  `sas-llm-agent` with `http`, and for `sas-plugin` on
  `aarch64-apple-darwin`.

**Decisions and follow-ups**

- Phases that still can't be stopped:
  - resolving the host name, which takes as long as the system's resolver
    (unbounded);
  - connecting, at most 10 s;
  - sending the request, at most 10 s;
  - plain-HTTP requests (a local OpenAI-compatible server): `ureq` 2
    offers a hook for TLS connections only, so they end at the provider's
    answer or the 120 s idle limit. Hosted providers use HTTPS.
- The FL Studio check covers the transport with a real provider: press
  Stop while a reply is pending.

**Blocked:** nothing. Merges after #102, in the held stack.
