//! Model provider adapters: the neutral conversation to and from each wire
//! format. The default is Anthropic's Messages API (owner's working default,
//! 2026-10-22); the OpenAI-compatible Chat Completions adapter covers
//! OpenAI, OpenRouter and local servers (Ollama, LM Studio).

use serde_json::{json, Value};

use crate::message::{Block, ChatRequest, ChatResponse, Message, Role, StopReason, Tool, Usage};
use crate::transport::HttpRequest;
use crate::AgentError;

/// Builds a provider's HTTP request and reads its response.
pub trait Provider: Send {
    fn request(&self, request: &ChatRequest) -> HttpRequest;
    fn response(&self, body: &Value) -> Result<ChatResponse, AgentError>;
}

/// Which wire format, with its defaults.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderKind {
    Anthropic,
    OpenAiCompatible,
}

impl ProviderKind {
    pub fn parse(name: &str) -> Option<ProviderKind> {
        match name {
            "anthropic" | "claude" => Some(ProviderKind::Anthropic),
            "openai" | "openai-compatible" => Some(ProviderKind::OpenAiCompatible),
            _ => None,
        }
    }

    pub fn default_base_url(self) -> &'static str {
        match self {
            ProviderKind::Anthropic => "https://api.anthropic.com",
            ProviderKind::OpenAiCompatible => "https://api.openai.com/v1",
        }
    }

    /// A model id the user can override; providers rename models often.
    pub fn default_model(self) -> &'static str {
        match self {
            ProviderKind::Anthropic => "claude-sonnet-4-5",
            ProviderKind::OpenAiCompatible => "gpt-4.1",
        }
    }

    /// The environment variable the command line reads the key from.
    pub fn key_variable(self) -> &'static str {
        match self {
            ProviderKind::Anthropic => "ANTHROPIC_API_KEY",
            ProviderKind::OpenAiCompatible => "OPENAI_API_KEY",
        }
    }
}

/// Everything needed to talk to one model.
#[derive(Clone)]
pub struct ProviderConfig {
    pub kind: ProviderKind,
    pub model: String,
    pub base_url: String,
    /// Optional for local OpenAI-compatible servers.
    pub api_key: Option<String>,
}

impl std::fmt::Debug for ProviderConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderConfig")
            .field("kind", &self.kind)
            .field("model", &self.model)
            .field("base_url", &self.base_url)
            .field("api_key", &self.api_key.as_ref().map(|_| "<set>"))
            .finish()
    }
}

impl ProviderConfig {
    pub fn new(kind: ProviderKind, api_key: Option<String>) -> ProviderConfig {
        ProviderConfig {
            kind,
            model: kind.default_model().to_string(),
            base_url: kind.default_base_url().to_string(),
            api_key,
        }
    }

    pub fn build(&self) -> Box<dyn Provider> {
        match self.kind {
            ProviderKind::Anthropic => Box::new(Anthropic {
                config: self.clone(),
            }),
            ProviderKind::OpenAiCompatible => Box::new(OpenAiCompatible {
                config: self.clone(),
            }),
        }
    }
}

fn base(url: &str) -> &str {
    url.trim_end_matches('/')
}

/// Anthropic Messages API (`POST /v1/messages`). The system prompt and the
/// tool list are marked for prompt caching: they are the same on every call.
pub struct Anthropic {
    config: ProviderConfig,
}

impl Anthropic {
    fn message(message: &Message) -> Value {
        let content: Vec<Value> = message
            .content
            .iter()
            .map(|block| match block {
                Block::Text(text) => json!({"type": "text", "text": text}),
                Block::ToolUse { id, name, input } => {
                    json!({"type": "tool_use", "id": id, "name": name, "input": input})
                }
                Block::ToolResult {
                    id,
                    content,
                    is_error,
                } => json!({
                    "type": "tool_result",
                    "tool_use_id": id,
                    "content": content,
                    "is_error": is_error,
                }),
            })
            .collect();
        let role = match message.role {
            Role::User => "user",
            Role::Assistant => "assistant",
        };
        json!({"role": role, "content": content})
    }

    fn tools(tools: &[Tool]) -> Vec<Value> {
        let mut values: Vec<Value> = tools
            .iter()
            .map(|tool| {
                json!({"name": tool.name, "description": tool.description, "input_schema": tool.input_schema})
            })
            .collect();
        if let Some(last) = values.last_mut() {
            last["cache_control"] = json!({"type": "ephemeral"});
        }
        values
    }
}

impl Provider for Anthropic {
    fn request(&self, request: &ChatRequest) -> HttpRequest {
        let mut headers = vec![
            ("content-type".to_string(), "application/json".to_string()),
            ("anthropic-version".to_string(), "2023-06-01".to_string()),
        ];
        if let Some(key) = &self.config.api_key {
            headers.push(("x-api-key".to_string(), key.clone()));
        }
        HttpRequest {
            url: format!("{}/v1/messages", base(&self.config.base_url)),
            headers,
            body: json!({
                "model": self.config.model,
                "max_tokens": request.max_tokens,
                "system": [{"type": "text", "text": request.system, "cache_control": {"type": "ephemeral"}}],
                "tools": Anthropic::tools(request.tools),
                "messages": request.messages.iter().map(Anthropic::message).collect::<Vec<_>>(),
            }),
        }
    }

    fn response(&self, body: &Value) -> Result<ChatResponse, AgentError> {
        if body["type"] == "error" {
            return Err(AgentError::Provider(body["error"]["message"].to_string()));
        }
        let blocks = body["content"]
            .as_array()
            .ok_or_else(|| AgentError::Provider(format!("no content in {body}")))?;
        let mut content = Vec::new();
        for block in blocks {
            match block["type"].as_str() {
                Some("text") => content.push(Block::Text(
                    block["text"].as_str().unwrap_or_default().to_string(),
                )),
                Some("tool_use") => content.push(Block::ToolUse {
                    id: block["id"].as_str().unwrap_or_default().to_string(),
                    name: block["name"].as_str().unwrap_or_default().to_string(),
                    input: block["input"].clone(),
                }),
                _ => {}
            }
        }
        let stop = match body["stop_reason"].as_str() {
            Some("end_turn") | Some("stop_sequence") => StopReason::EndTurn,
            Some("tool_use") => StopReason::ToolUse,
            Some("max_tokens") => StopReason::MaxTokens,
            other => StopReason::Other(other.unwrap_or("none").to_string()),
        };
        let usage = &body["usage"];
        let count = |key: &str| usage[key].as_u64().unwrap_or(0);
        Ok(ChatResponse {
            content,
            stop,
            usage: Usage {
                input_tokens: count("input_tokens"),
                output_tokens: count("output_tokens"),
                cache_read_tokens: count("cache_read_input_tokens"),
                cache_write_tokens: count("cache_creation_input_tokens"),
            },
        })
    }
}

/// OpenAI-compatible Chat Completions (`POST {base}/chat/completions`).
pub struct OpenAiCompatible {
    config: ProviderConfig,
}

impl OpenAiCompatible {
    fn messages(system: &str, messages: &[Message]) -> Vec<Value> {
        let mut values = vec![json!({"role": "system", "content": system})];
        for message in messages {
            match message.role {
                Role::Assistant => {
                    let calls: Vec<Value> = message
                        .content
                        .iter()
                        .filter_map(|block| match block {
                            Block::ToolUse { id, name, input } => Some(json!({
                                "id": id,
                                "type": "function",
                                "function": {"name": name, "arguments": input.to_string()},
                            })),
                            _ => None,
                        })
                        .collect();
                    let text = message.text();
                    let mut value = json!({"role": "assistant", "content": if text.is_empty() { Value::Null } else { Value::String(text) }});
                    if !calls.is_empty() {
                        value["tool_calls"] = Value::Array(calls);
                    }
                    values.push(value);
                }
                Role::User => {
                    // Tool results are their own messages, before any text.
                    for block in &message.content {
                        if let Block::ToolResult {
                            id,
                            content,
                            is_error,
                        } = block
                        {
                            let content = if *is_error {
                                format!("error: {content}")
                            } else {
                                content.clone()
                            };
                            values.push(
                                json!({"role": "tool", "tool_call_id": id, "content": content}),
                            );
                        }
                    }
                    let text = message.text();
                    if !text.is_empty() {
                        values.push(json!({"role": "user", "content": text}));
                    }
                }
            }
        }
        values
    }
}

impl Provider for OpenAiCompatible {
    fn request(&self, request: &ChatRequest) -> HttpRequest {
        let mut headers = vec![("content-type".to_string(), "application/json".to_string())];
        if let Some(key) = &self.config.api_key {
            headers.push(("authorization".to_string(), format!("Bearer {key}")));
        }
        let tools: Vec<Value> = request
            .tools
            .iter()
            .map(|tool| {
                json!({"type": "function", "function": {
                    "name": tool.name, "description": tool.description, "parameters": tool.input_schema,
                }})
            })
            .collect();
        HttpRequest {
            url: format!("{}/chat/completions", base(&self.config.base_url)),
            headers,
            body: json!({
                "model": self.config.model,
                "max_tokens": request.max_tokens,
                "tools": tools,
                "messages": OpenAiCompatible::messages(request.system, request.messages),
            }),
        }
    }

    fn response(&self, body: &Value) -> Result<ChatResponse, AgentError> {
        if let Some(error) = body.get("error") {
            return Err(AgentError::Provider(error["message"].to_string()));
        }
        let choice = &body["choices"][0];
        let message = &choice["message"];
        if message.is_null() {
            return Err(AgentError::Provider(format!("no message in {body}")));
        }
        let mut content = Vec::new();
        if let Some(text) = message["content"].as_str().filter(|text| !text.is_empty()) {
            content.push(Block::Text(text.to_string()));
        }
        for call in message["tool_calls"].as_array().into_iter().flatten() {
            let arguments = call["function"]["arguments"].as_str().unwrap_or("{}");
            let input =
                serde_json::from_str(arguments).unwrap_or_else(|_| json!({"_unparsed": arguments}));
            content.push(Block::ToolUse {
                id: call["id"].as_str().unwrap_or_default().to_string(),
                name: call["function"]["name"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
                input,
            });
        }
        let stop = match choice["finish_reason"].as_str() {
            Some("stop") => StopReason::EndTurn,
            Some("tool_calls") => StopReason::ToolUse,
            Some("length") => StopReason::MaxTokens,
            other => StopReason::Other(other.unwrap_or("none").to_string()),
        };
        let usage = &body["usage"];
        Ok(ChatResponse {
            content,
            stop,
            usage: Usage {
                input_tokens: usage["prompt_tokens"].as_u64().unwrap_or(0),
                output_tokens: usage["completion_tokens"].as_u64().unwrap_or(0),
                cache_read_tokens: usage["prompt_tokens_details"]["cached_tokens"]
                    .as_u64()
                    .unwrap_or(0),
                cache_write_tokens: 0,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tools() -> Vec<Tool> {
        vec![
            Tool {
                name: "get_patch".into(),
                description: "d".into(),
                input_schema: json!({"type": "object", "properties": {}}),
            },
            Tool {
                name: "set_params".into(),
                description: "d".into(),
                input_schema: json!({"type": "object", "properties": {}}),
            },
        ]
    }

    fn conversation() -> Vec<Message> {
        vec![
            Message::user(vec![Block::Text("brighter".into())]),
            Message::assistant(vec![
                Block::Text("Opening the filter.".into()),
                Block::ToolUse {
                    id: "t1".into(),
                    name: "set_params".into(),
                    input: json!({"changes": []}),
                },
            ]),
            Message::user(vec![Block::ToolResult {
                id: "t1".into(),
                content: "{}".into(),
                is_error: false,
            }]),
        ]
    }

    #[test]
    fn anthropic_requests_cache_the_prompt_and_tools() {
        let provider = ProviderConfig::new(ProviderKind::Anthropic, Some("k".into())).build();
        let (tools, messages) = (tools(), conversation());
        let request = provider.request(&ChatRequest {
            system: "sys",
            messages: &messages,
            tools: &tools,
            max_tokens: 100,
        });
        assert!(request.url.ends_with("/v1/messages"));
        assert!(request
            .headers
            .iter()
            .any(|(name, value)| name == "x-api-key" && value == "k"));
        let body = &request.body;
        assert_eq!(body["system"][0]["cache_control"]["type"], "ephemeral");
        assert!(body["tools"][0].get("cache_control").is_none());
        assert_eq!(body["tools"][1]["cache_control"]["type"], "ephemeral");
        assert_eq!(body["messages"][1]["content"][1]["type"], "tool_use");
        assert_eq!(body["messages"][2]["content"][0]["tool_use_id"], "t1");
    }

    #[test]
    fn anthropic_responses_are_read() {
        let provider = ProviderConfig::new(ProviderKind::Anthropic, None).build();
        let response = provider
            .response(&json!({
                "content": [{"type": "text", "text": "ok"}, {"type": "tool_use", "id": "a", "name": "get_patch", "input": {}}],
                "stop_reason": "tool_use",
                "usage": {"input_tokens": 10, "output_tokens": 5, "cache_read_input_tokens": 4000}
            }))
            .unwrap();
        assert_eq!(response.stop, StopReason::ToolUse);
        assert_eq!(response.content.len(), 2);
        assert_eq!(response.usage.cache_read_tokens, 4000);
        assert!(provider
            .response(&json!({"type": "error", "error": {"message": "overloaded"}}))
            .is_err());
    }

    #[test]
    fn openai_requests_split_tool_results_into_tool_messages() {
        let mut config = ProviderConfig::new(ProviderKind::OpenAiCompatible, None);
        config.base_url = "http://localhost:11434/v1/".into();
        let provider = config.build();
        let (tools, messages) = (tools(), conversation());
        let request = provider.request(&ChatRequest {
            system: "sys",
            messages: &messages,
            tools: &tools,
            max_tokens: 100,
        });
        assert_eq!(request.url, "http://localhost:11434/v1/chat/completions");
        assert!(request
            .headers
            .iter()
            .all(|(name, _)| name != "authorization"));
        let messages = request.body["messages"].as_array().unwrap();
        assert_eq!(messages[0]["role"], "system");
        assert_eq!(
            messages[2]["tool_calls"][0]["function"]["arguments"],
            "{\"changes\":[]}"
        );
        assert_eq!(messages[3]["role"], "tool");
        assert_eq!(messages[3]["tool_call_id"], "t1");
        assert_eq!(request.body["tools"][1]["function"]["name"], "set_params");
    }

    #[test]
    fn openai_responses_are_read() {
        let provider = ProviderConfig::new(ProviderKind::OpenAiCompatible, None).build();
        let response = provider
            .response(&json!({
                "choices": [{"message": {"content": null, "tool_calls": [
                    {"id": "c1", "type": "function", "function": {"name": "set_params", "arguments": "{\"changes\":[]}"}}
                ]}, "finish_reason": "tool_calls"}],
                "usage": {"prompt_tokens": 12, "completion_tokens": 3}
            }))
            .unwrap();
        assert_eq!(response.stop, StopReason::ToolUse);
        assert_eq!(
            response.content,
            vec![Block::ToolUse {
                id: "c1".into(),
                name: "set_params".into(),
                input: json!({"changes": []})
            }]
        );
    }

    #[test]
    fn debug_hides_the_key() {
        let config = ProviderConfig::new(ProviderKind::Anthropic, Some("secret".into()));
        assert!(!format!("{config:?}").contains("secret"));
    }
}
