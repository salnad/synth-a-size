//! The chat loop: a user message in, model calls and tool calls until the
//! model is done, a reply out. It runs on whatever thread calls it (the
//! plugin's chat worker, the CLI's main thread), never the audio thread.

use serde_json::Value;

use sas_llm_control::{catalog, patch, specs, Controller, SynthSession, ToolOutput};

use crate::message::{Block, ChatRequest, Message, StopReason, Tool, Usage};
use crate::prompt::system_prompt;
use crate::provider::Provider;
use crate::transport::Transport;
use crate::AgentError;

/// Limits of a chat session. Every field is a working default the owner
/// may change.
#[derive(Clone, Debug, PartialEq)]
pub struct AgentConfig {
    /// Model calls per user message before the loop stops.
    pub max_steps: usize,
    /// Output tokens per model call.
    pub max_tokens: u32,
    /// Tokens (all kinds) per session; further messages are refused.
    pub max_session_tokens: u64,
    /// Earlier exchanges kept in the conversation sent to the model.
    pub history_turns: usize,
}

impl Default for AgentConfig {
    fn default() -> Self {
        AgentConfig {
            max_steps: 20,
            max_tokens: 2048,
            max_session_tokens: 1_000_000,
            history_turns: 8,
        }
    }
}

/// What happens during a turn, for a UI to show as it goes.
#[derive(Debug)]
pub enum AgentEvent<'a> {
    Text(&'a str),
    ToolCall {
        name: &'a str,
        input: &'a Value,
    },
    ToolResult {
        name: &'a str,
        output: &'a ToolOutput,
    },
}

/// How a turn ended.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TurnReport {
    /// The model's text, joined.
    pub text: String,
    pub tool_calls: usize,
    pub model_calls: usize,
    /// Set when the loop stopped early (step limit, output limit).
    pub stopped: Option<String>,
    pub usage: Usage,
}

/// A chat session driving one synth session.
pub struct Agent<S: SynthSession> {
    controller: Controller<S>,
    provider: Box<dyn Provider>,
    transport: Box<dyn Transport>,
    config: AgentConfig,
    system: String,
    tools: Vec<Tool>,
    /// Completed exchanges, oldest first, each a list of messages.
    history: Vec<Vec<Message>>,
    /// Text for the model about something the user did between turns.
    note: Option<String>,
    usage: Usage,
}

impl<S: SynthSession> Agent<S> {
    pub fn new(
        controller: Controller<S>,
        provider: Box<dyn Provider>,
        transport: Box<dyn Transport>,
        config: AgentConfig,
    ) -> Agent<S> {
        let tools = specs()
            .into_iter()
            .map(|spec| Tool {
                name: spec.name.to_string(),
                description: spec.description.to_string(),
                input_schema: spec.input_schema,
            })
            .collect();
        Agent {
            controller,
            provider,
            transport,
            config,
            system: system_prompt(),
            tools,
            history: Vec::new(),
            note: None,
            usage: Usage::default(),
        }
    }

    pub fn controller(&self) -> &Controller<S> {
        &self.controller
    }

    pub fn controller_mut(&mut self) -> &mut Controller<S> {
        &mut self.controller
    }

    pub fn into_controller(self) -> Controller<S> {
        self.controller
    }

    /// Tokens used so far in this session.
    pub fn usage(&self) -> Usage {
        self.usage
    }

    /// The user confirmed (`true`) or rejected the edits waiting for them.
    /// The model is told at the start of the next message.
    pub fn resolve_pending(&mut self, accept: bool) -> Result<bool, AgentError> {
        let Some(reason) = self
            .controller
            .pending()
            .map(|pending| pending.reason.clone())
        else {
            return Ok(false);
        };
        if accept {
            self.controller
                .confirm_pending()
                .map_err(|e| AgentError::Tool(e.to_string()))?;
            self.note = Some(format!(
                "The user confirmed the held change ({reason}); it is applied."
            ));
        } else {
            self.controller.reject_pending();
            self.note = Some(format!(
                "The user declined the held change ({reason}); nothing changed."
            ));
        }
        Ok(true)
    }

    /// The user undid steps outside the chat (an Undo button).
    pub fn note_user_undo(&mut self, steps: usize) {
        self.note = Some(format!("The user undid {steps} step(s) of your edits."));
    }

    /// Sends `text`, runs the tool loop, and returns the model's reply.
    pub fn send(
        &mut self,
        text: &str,
        on_event: &mut dyn FnMut(AgentEvent),
    ) -> Result<TurnReport, AgentError> {
        if self.usage.total() >= self.config.max_session_tokens {
            return Err(AgentError::Budget(format!(
                "this chat used {} tokens, its limit; start a new chat",
                self.usage.total()
            )));
        }
        let replaced = self.controller.check_replaced();
        let outside = self.controller.session_mut().take_outside_changes();
        self.controller.begin_turn();
        let mut opening = Vec::new();
        if let Some(note) = self.note.take() {
            opening.push(Block::Text(note));
        }
        if replaced {
            opening.push(Block::Text(
                "The whole patch was replaced outside the chat (a preset load or the host \
                 restoring its state); earlier undo steps are gone."
                    .to_string(),
            ));
        }
        if !outside.is_empty() {
            let changes: Vec<String> = outside
                .iter()
                .filter_map(|change| {
                    let info = catalog::control(change.name)?;
                    Some(format!(
                        "{} is now {} (you set {})",
                        change.name,
                        catalog::value_view(info, change.now),
                        catalog::value_view(info, change.set)
                    ))
                })
                .collect();
            opening.push(Block::Text(format!(
                "Changed outside the chat since your edits (host automation or the user; don't \
                 fight automation): {}.",
                changes.join("; ")
            )));
        }
        let state = self
            .controller
            .session()
            .snapshot()
            .map_err(|error| AgentError::Tool(format!("cannot read the patch: {error}")))?;
        let patch = patch::describe(&state);
        opening.push(Block::Text(format!("Current patch:\n{patch}")));
        opening.push(Block::Text(text.to_string()));
        let mut turn = vec![Message::user(opening)];
        let mut report = TurnReport::default();

        loop {
            if report.model_calls == self.config.max_steps {
                report.stopped = Some(format!(
                    "stopped after {} model calls",
                    self.config.max_steps
                ));
                turn.push(Message::assistant(vec![Block::Text(
                    "(Stopped: too many steps in one reply.)".to_string(),
                )]));
                break;
            }
            let start = self.history.len().saturating_sub(self.config.history_turns);
            let messages: Vec<Message> = self.history[start..]
                .iter()
                .flatten()
                .chain(turn.iter())
                .cloned()
                .collect();
            let request = self.provider.request(&ChatRequest {
                system: &self.system,
                messages: &messages,
                tools: &self.tools,
                max_tokens: self.config.max_tokens,
            });
            let body = self.transport.send(&request)?;
            let response = self.provider.response(&body)?;
            report.model_calls += 1;
            report.usage.add(response.usage);
            self.usage.add(response.usage);

            let mut results = Vec::new();
            for block in &response.content {
                match block {
                    Block::Text(text) => {
                        on_event(AgentEvent::Text(text));
                        if !report.text.is_empty() {
                            report.text.push('\n');
                        }
                        report.text.push_str(text);
                    }
                    Block::ToolUse { id, name, input } => {
                        on_event(AgentEvent::ToolCall { name, input });
                        let output = self.controller.call(name, input);
                        on_event(AgentEvent::ToolResult {
                            name,
                            output: &output,
                        });
                        report.tool_calls += 1;
                        results.push(Block::ToolResult {
                            id: id.clone(),
                            content: output.content,
                            is_error: output.is_error,
                        });
                    }
                    Block::ToolResult { .. } => {}
                }
            }
            let stop = response.stop.clone();
            turn.push(Message::assistant(response.content));
            if results.is_empty() {
                if stop == StopReason::MaxTokens {
                    report.stopped = Some("the reply hit the output token limit".to_string());
                }
                break;
            }
            turn.push(Message::user(results));
        }
        self.history.push(turn);
        Ok(report)
    }
}
