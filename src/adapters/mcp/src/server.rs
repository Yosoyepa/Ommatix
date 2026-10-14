use std::io::{BufRead, Write};

use ommatix_application::AgentSession;
use serde_json::{Value, json};

use crate::jsonrpc::{
    self, Call, INVALID_PARAMS, INVALID_REQUEST, Incoming, METHOD_NOT_FOUND, MethodError,
    PARSE_ERROR,
};
use crate::tools;

/// Latest MCP revision this server implements. Older revisions requested by a client
/// are echoed back (the subset used here is identical across them).
pub const PROTOCOL_VERSION: &str = "2025-06-18";
const SUPPORTED_VERSIONS: [&str; 3] = ["2024-11-05", "2025-03-26", PROTOCOL_VERSION];

/// Minimal, synchronous MCP server: one JSON-RPC message (or batch) per line on `input`,
/// one response per line on `output`. Diagnostics must go to stderr, never stdout.
#[derive(Debug)]
pub struct McpServer {
    session: AgentSession,
}

impl McpServer {
    /// The agent session is injected; the server only translates MCP into calls on it.
    pub fn new(session: AgentSession) -> Self {
        Self { session }
    }

    pub fn session(&self) -> &AgentSession {
        &self.session
    }

    /// Serves until `input` reaches EOF. Malformed lines (including invalid UTF-8) are
    /// answered with JSON-RPC errors; only I/O failures stop the server.
    pub fn serve(
        &mut self,
        mut input: impl BufRead,
        mut output: impl Write,
    ) -> std::io::Result<()> {
        let mut line = Vec::new();
        loop {
            line.clear();
            if input.read_until(b'\n', &mut line)? == 0 {
                return Ok(());
            }
            let message = line.trim_ascii();
            if message.is_empty() {
                continue;
            }
            if let Some(response) = self.handle_message(message) {
                serde_json::to_writer(&mut output, &response)?;
                output.write_all(b"\n")?;
                output.flush()?;
            }
        }
    }

    /// Handles one raw message or batch. Returns `None` when nothing must be sent back
    /// (notifications, client responses, batches of those).
    pub fn handle_message(&mut self, raw: &[u8]) -> Option<Value> {
        match serde_json::from_slice::<Value>(raw) {
            Err(e) => Some(jsonrpc::failure(
                &Value::Null,
                PARSE_ERROR,
                &format!("parse error: {e}"),
            )),
            Ok(Value::Array(batch)) if batch.is_empty() => Some(jsonrpc::failure(
                &Value::Null,
                INVALID_REQUEST,
                "empty batch",
            )),
            Ok(Value::Array(batch)) => {
                let responses: Vec<Value> = batch.iter().filter_map(|m| self.handle(m)).collect();
                (!responses.is_empty()).then_some(Value::Array(responses))
            }
            Ok(message) => self.handle(&message),
        }
    }

    fn handle(&mut self, message: &Value) -> Option<Value> {
        match jsonrpc::classify(message) {
            Incoming::Response => None,
            Incoming::Invalid { id, reason } => {
                Some(jsonrpc::failure(&id, INVALID_REQUEST, reason))
            }
            Incoming::Call(Call { id, method, params }) => {
                let result = self.dispatch(method, params);
                // Notifications are executed but never answered, even on error.
                let id = id?;
                Some(match result {
                    Ok(result) => jsonrpc::success(id, result),
                    Err((code, message)) => jsonrpc::failure(id, code, &message),
                })
            }
        }
    }

    fn dispatch(&mut self, method: &str, params: &Value) -> Result<Value, MethodError> {
        match method {
            "initialize" => Ok(self.initialize(params)),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({ "tools": tools::definitions() })),
            "tools/call" => self.call_tool(params),
            m if m.starts_with("notifications/") => Ok(Value::Null),
            other => Err((METHOD_NOT_FOUND, format!("method not found: {other}"))),
        }
    }

    fn initialize(&self, params: &Value) -> Value {
        let requested = params.get("protocolVersion").and_then(Value::as_str);
        let version = requested
            .filter(|v| SUPPORTED_VERSIONS.contains(v))
            .unwrap_or(PROTOCOL_VERSION);
        let engine = self.session.perception();
        let input = self.session.actuation();
        json!({
            "protocolVersion": version,
            "capabilities": { "tools": { "listChanged": false } },
            "serverInfo": {
                "name": "ommatix",
                "title": "Ommatix",
                "version": env!("CARGO_PKG_VERSION")
            },
            "instructions": format!(
                "Ommatix gives you deterministic screen perception. Call `perceive` first \
                 (only changed regions are re-analysed), then use `element_at`, \
                 `elements_in_rect` or `list_elements` to get exact pixel coordinates. Act \
                 with `click` (prefer element ids), `type_text` and `press_key`; each waits \
                 for the screen to settle and returns what changed. Source: {}, detector: \
                 {}, input: {} ({}).",
                engine.source_name(),
                engine.detector_name(),
                input.input_name(),
                if input.is_live() { "live" } else { "not live: actions do not reach the OS" }
            )
        })
    }

    fn call_tool(&mut self, params: &Value) -> Result<Value, MethodError> {
        let name = params
            .get("name")
            .and_then(Value::as_str)
            .ok_or((INVALID_PARAMS, "missing tool name".to_owned()))?;
        let tool =
            tools::find(name).ok_or_else(|| (INVALID_PARAMS, format!("unknown tool: {name}")))?;
        let args = params.get("arguments").unwrap_or(&Value::Null);
        Ok(match (tool.run)(&mut self.session, args) {
            Ok(output) => {
                let mut result = json!({
                    "content": [{ "type": "text", "text": output.structured.to_string() }],
                    "structuredContent": output.structured,
                    "isError": false
                });
                if let Some(meta) = output.meta {
                    result["_meta"] = meta;
                }
                result
            }
            Err(message) => json!({
                "content": [{ "type": "text", "text": message }],
                "isError": true
            }),
        })
    }
}
