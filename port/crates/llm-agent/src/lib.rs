// SPDX-License-Identifier: GPL-3.0-or-later

//! The LLM chat loop over the synth's tools (an addition with no upstream
//! counterpart; D12 in `migration/DIVERGENCES.md`).
//!
//! - [`agent`]: [`Agent`], one chat session driving a
//!   `sas_llm_control::SynthSession`, with step, output and session token
//!   limits.
//! - [`provider`]: Anthropic Messages (the default) and OpenAI-compatible
//!   Chat Completions adapters, configurable model and endpoint.
//! - [`transport`]: real HTTPS behind the `http` feature (off by default),
//!   and recorded replays: tests and CI only ever replay.
//! - [`prompt`]: the system prompt and control catalog.
//!
//! The `sas-llm` command line (feature `http`) chats with a preset file.

use std::fmt;

pub mod agent;
pub mod eval;
pub mod message;
pub mod prompt;
pub mod provider;
pub mod settings;
pub mod transport;

pub use agent::{Agent, AgentConfig, AgentEvent, TurnReport};
pub use message::Usage;
pub use provider::{Provider, ProviderConfig, ProviderKind};
pub use settings::ChatSettings;
pub use transport::{RecordingTransport, ReplayTransport, ScriptedTransport, Transport};

#[cfg(feature = "http")]
pub use transport::HttpTransport;

/// Milestone in `migration/PLAN.md` this crate belongs to.
pub const MILESTONE: &str = "M9";

/// Why a chat step failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentError {
    /// The provider answered with an HTTP error status.
    Http(u16, String),
    /// The request could not be sent.
    Transport(String),
    /// The response was not what the provider's format promises.
    Provider(String),
    /// A recording could not be read, or a request did not match it.
    Replay(String),
    /// A session limit was reached.
    Budget(String),
    Tool(String),
}

impl fmt::Display for AgentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AgentError::Http(status, body) => write!(f, "provider returned HTTP {status}: {body}"),
            AgentError::Transport(message) => write!(f, "cannot reach the provider: {message}"),
            AgentError::Provider(message) => write!(f, "unexpected provider response: {message}"),
            AgentError::Replay(message) => write!(f, "replay: {message}"),
            AgentError::Budget(message) => f.write_str(message),
            AgentError::Tool(message) => write!(f, "tool: {message}"),
        }
    }
}

impl std::error::Error for AgentError {}
