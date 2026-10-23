//! How requests reach a model: real HTTPS (feature `http`), or recorded
//! exchanges replayed from a file, so tests never call a model API.

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::AgentError;

/// One HTTP POST with a JSON body. Header values can hold the API key, so
/// they are never recorded or printed.
#[derive(Clone)]
pub struct HttpRequest {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Value,
}

impl std::fmt::Debug for HttpRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let names: Vec<&str> = self.headers.iter().map(|(name, _)| name.as_str()).collect();
        f.debug_struct("HttpRequest")
            .field("url", &self.url)
            .field("headers", &names)
            .field("body", &self.body)
            .finish()
    }
}

/// Sends a request and returns the JSON response body.
pub trait Transport {
    fn send(&mut self, request: &HttpRequest) -> Result<Value, AgentError>;
}

/// The part of a request that must match a recording: the conversation
/// (without any system message, so prompt changes do not invalidate
/// recordings), the model, and the offered tool names.
pub fn comparable(body: &Value) -> Value {
    let messages: Vec<Value> = body["messages"]
        .as_array()
        .map(|messages| {
            messages
                .iter()
                .filter(|message| message["role"] != "system")
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    let tools: Vec<Value> = body["tools"]
        .as_array()
        .map(|tools| {
            tools
                .iter()
                .map(|tool| {
                    if tool.is_string() {
                        return tool.clone();
                    }
                    tool.get("name")
                        .or_else(|| tool["function"].get("name"))
                        .cloned()
                        .unwrap_or(Value::Null)
                })
                .collect()
        })
        .unwrap_or_default();
    json!({"model": body["model"], "messages": messages, "tools": tools})
}

/// Path of `url` without scheme and host, so recordings do not depend on
/// the base URL.
fn url_path(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    rest.find('/').map_or("", |index| &rest[index..])
}

/// One recorded exchange, a line of a `.jsonl` recording. Only the part of
/// the request a replay compares is kept ([`comparable`]): no system prompt,
/// tool schemas or headers.
fn exchange(request: &HttpRequest, response: &Value) -> Value {
    json!({"path": url_path(&request.url), "request": comparable(&request.body), "response": response})
}

/// Plays back a recording: each request must match the next recorded one
/// ([`comparable`]), and gets its recorded response.
pub struct ReplayTransport {
    source: String,
    exchanges: Vec<Value>,
    next: usize,
}

impl ReplayTransport {
    /// Reads a `.jsonl` recording.
    pub fn open(path: &Path) -> Result<ReplayTransport, AgentError> {
        let text = fs::read_to_string(path)
            .map_err(|e| AgentError::Replay(format!("{}: {e}", path.display())))?;
        let exchanges = text
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(serde_json::from_str)
            .collect::<Result<Vec<Value>, _>>()
            .map_err(|e| AgentError::Replay(format!("{}: {e}", path.display())))?;
        Ok(ReplayTransport {
            source: path.display().to_string(),
            exchanges,
            next: 0,
        })
    }

    /// Exchanges not yet played.
    pub fn remaining(&self) -> usize {
        self.exchanges.len() - self.next
    }
}

impl Transport for ReplayTransport {
    fn send(&mut self, request: &HttpRequest) -> Result<Value, AgentError> {
        let index = self.next;
        let recorded = self.exchanges.get(index).ok_or_else(|| {
            AgentError::Replay(format!(
                "{}: no recorded exchange {}",
                self.source,
                index + 1
            ))
        })?;
        if recorded["path"] != url_path(&request.url) {
            return Err(AgentError::Replay(format!(
                "{}: exchange {} was recorded for {}, not {}",
                self.source,
                index + 1,
                recorded["path"],
                url_path(&request.url)
            )));
        }
        let expected = comparable(&recorded["request"]);
        let actual = comparable(&request.body);
        if expected != actual {
            return Err(AgentError::Replay(format!(
                "{}: request {} differs from the recording\nrecorded: {expected}\nactual:   {actual}",
                self.source,
                index + 1
            )));
        }
        self.next += 1;
        Ok(recorded["response"].clone())
    }
}

/// Answers each request with the next of a list of responses, without
/// checking requests. Used to write recordings from authored responses.
pub struct ScriptedTransport {
    responses: std::vec::IntoIter<Value>,
}

impl ScriptedTransport {
    pub fn new(responses: Vec<Value>) -> ScriptedTransport {
        ScriptedTransport {
            responses: responses.into_iter(),
        }
    }
}

impl Transport for ScriptedTransport {
    fn send(&mut self, _request: &HttpRequest) -> Result<Value, AgentError> {
        self.responses
            .next()
            .ok_or_else(|| AgentError::Replay("the script has no more responses".to_string()))
    }
}

/// Passes requests to another transport and appends each exchange to a
/// `.jsonl` recording. Headers (and so the API key) are never written.
pub struct RecordingTransport<T: Transport> {
    inner: T,
    path: PathBuf,
}

impl<T: Transport> RecordingTransport<T> {
    /// Records into `path`, replacing any earlier recording there.
    pub fn new(inner: T, path: impl Into<PathBuf>) -> Result<RecordingTransport<T>, AgentError> {
        let path = path.into();
        fs::write(&path, "").map_err(|e| AgentError::Replay(format!("{}: {e}", path.display())))?;
        Ok(RecordingTransport { inner, path })
    }
}

impl<T: Transport> Transport for RecordingTransport<T> {
    fn send(&mut self, request: &HttpRequest) -> Result<Value, AgentError> {
        let response = self.inner.send(request)?;
        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(&self.path)
            .map_err(|e| AgentError::Replay(format!("{}: {e}", self.path.display())))?;
        writeln!(file, "{}", exchange(request, &response))
            .map_err(|e| AgentError::Replay(format!("{}: {e}", self.path.display())))?;
        Ok(response)
    }
}

/// Real HTTPS through the system's TLS.
#[cfg(feature = "http")]
pub struct HttpTransport {
    agent: ureq::Agent,
}

#[cfg(feature = "http")]
impl HttpTransport {
    pub fn new() -> Result<HttpTransport, AgentError> {
        let tls = native_tls::TlsConnector::new()
            .map_err(|e| AgentError::Transport(format!("TLS: {e}")))?;
        let agent = ureq::AgentBuilder::new()
            .tls_connector(std::sync::Arc::new(tls))
            .timeout(std::time::Duration::from_secs(120))
            .build();
        Ok(HttpTransport { agent })
    }
}

#[cfg(feature = "http")]
impl Transport for HttpTransport {
    fn send(&mut self, request: &HttpRequest) -> Result<Value, AgentError> {
        let mut call = self.agent.post(&request.url);
        for (name, value) in &request.headers {
            call = call.set(name, value);
        }
        match call.send_string(&request.body.to_string()) {
            Ok(response) => {
                let text = response
                    .into_string()
                    .map_err(|e| AgentError::Transport(e.to_string()))?;
                serde_json::from_str(&text).map_err(|e| AgentError::Provider(e.to_string()))
            }
            Err(ureq::Error::Status(status, response)) => {
                let body = response.into_string().unwrap_or_default();
                Err(AgentError::Http(status, body))
            }
            Err(error) => Err(AgentError::Transport(error.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_never_prints_header_values() {
        let request = HttpRequest {
            url: "https://example.invalid/v1/messages".into(),
            headers: vec![("x-api-key".into(), "secret-value".into())],
            body: json!({}),
        };
        let text = format!("{request:?}");
        assert!(text.contains("x-api-key"));
        assert!(!text.contains("secret-value"));
    }

    #[test]
    fn comparison_ignores_the_system_prompt_and_tool_details() {
        let a = json!({"model": "m", "messages": [{"role": "system", "content": "A"}, {"role": "user", "content": "hi"}],
                       "tools": [{"type": "function", "function": {"name": "get_patch", "description": "x"}}]});
        let b = json!({"model": "m", "messages": [{"role": "system", "content": "B"}, {"role": "user", "content": "hi"}],
                       "tools": [{"type": "function", "function": {"name": "get_patch", "description": "y"}}]});
        assert_eq!(comparable(&a), comparable(&b));
        // Recordings store the comparable form; comparing it again is stable.
        assert_eq!(comparable(&comparable(&a)), comparable(&a));
        assert_eq!(
            url_path("https://api.example.com/v1/messages"),
            "/v1/messages"
        );
        assert_eq!(
            url_path("http://localhost:11434/v1/chat/completions"),
            "/v1/chat/completions"
        );
    }
}
