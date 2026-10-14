use std::io::{BufRead, Write};

use ommatix_application::{ActuationService, PerceptionEngine};
use serde_json::{Value, json};

use crate::tools;

/// Latest MCP revision this server implements. Older revisions requested by a client
/// are echoed back (the subset used here is identical across them).
pub const PROTOCOL_VERSION: &str = "2025-06-18";
const SUPPORTED_VERSIONS: [&str; 3] = ["2024-11-05", "2025-03-26", PROTOCOL_VERSION];

const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;

/// Minimal, synchronous MCP server: one JSON-RPC message per line on `input`,
/// one response per line on `output`. Diagnostics must go to stderr, never stdout.
#[derive(Debug)]
pub struct McpServer {
    perception: PerceptionEngine,
    actuation: ActuationService,
}

impl McpServer {
    /// Both services are injected; the server only translates JSON-RPC to calls on them.
    pub fn new(perception: PerceptionEngine, actuation: ActuationService) -> Self {
        Self {
            perception,
            actuation,
        }
    }

    pub fn perception(&self) -> &PerceptionEngine {
        &self.perception
    }

    /// Serves until `input` reaches EOF.
    pub fn serve(&mut self, input: impl BufRead, mut output: impl Write) -> std::io::Result<()> {
        for line in input.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            if let Some(response) = self.handle_line(&line) {
                serde_json::to_writer(&mut output, &response)?;
                output.write_all(b"\n")?;
                output.flush()?;
            }
        }
        Ok(())
    }

    /// Handles one raw message. Returns `None` for notifications.
    pub fn handle_line(&mut self, line: &str) -> Option<Value> {
        match serde_json::from_str::<Value>(line) {
            Ok(msg) => self.handle(&msg),
            Err(e) => Some(error(
                &Value::Null,
                PARSE_ERROR,
                &format!("parse error: {e}"),
            )),
        }
    }

    fn handle(&mut self, msg: &Value) -> Option<Value> {
        let id = msg.get("id").cloned();
        let Some(method) = msg.get("method").and_then(Value::as_str) else {
            // Responses from the client (we never send requests) and malformed input.
            return id.map(|id| error(&id, INVALID_REQUEST, "missing method"));
        };
        let id = id?; // notifications (e.g. notifications/initialized) get no response
        let params = msg.get("params").cloned().unwrap_or(Value::Null);

        let result = match method {
            "initialize" => Ok(self.initialize(&params)),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({ "tools": tools::definitions() })),
            "tools/call" => self.call_tool(&params),
            other => Err((METHOD_NOT_FOUND, format!("method not found: {other}"))),
        };
        Some(match result {
            Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
            Err((code, message)) => error(&id, code, &message),
        })
    }

    fn initialize(&self, params: &Value) -> Value {
        let requested = params.get("protocolVersion").and_then(Value::as_str);
        let version = requested
            .filter(|v| SUPPORTED_VERSIONS.contains(v))
            .unwrap_or(PROTOCOL_VERSION);
        json!({
            "protocolVersion": version,
            "capabilities": { "tools": { "listChanged": false } },
            "serverInfo": {
                "name": "ommatix",
                "title": "Ommatix",
                "version": env!("CARGO_PKG_VERSION")
            },
            "instructions": format!(
                "Ommatix gives you deterministic screen perception. Call `perceive` after each \
                 action to refresh the scene (only changed regions are re-analysed), then use \
                 `element_at`, `elements_in_rect` or `list_elements` to get exact pixel \
                 coordinates. Act with `click` (prefer element ids), `type_text` and \
                 `press_key`; each returns what changed on screen. Source: {}, detector: {}, \
                 input: {} ({}).",
                self.perception.source_name(),
                self.perception.detector_name(),
                self.actuation.input_name(),
                if self.actuation.is_live() { "live" } else { "not live: actions do not reach the OS" }
            )
        })
    }

    fn call_tool(&mut self, params: &Value) -> Result<Value, (i64, String)> {
        let name = params
            .get("name")
            .and_then(Value::as_str)
            .ok_or((INVALID_PARAMS, "missing tool name".to_string()))?
            .to_owned();
        let args = params.get("arguments").cloned().unwrap_or(Value::Null);
        let mut services = tools::Services {
            perception: &mut self.perception,
            actuation: &mut self.actuation,
        };
        let outcome = tools::call(&mut services, &name, &args)
            .ok_or_else(|| (INVALID_PARAMS, format!("unknown tool: {name}")))?;
        Ok(match outcome {
            Ok(structured) => json!({
                "content": [{ "type": "text", "text": structured.to_string() }],
                "structuredContent": structured,
                "isError": false
            }),
            Err(message) => json!({
                "content": [{ "type": "text", "text": message }],
                "isError": true
            }),
        })
    }
}

fn error(id: &Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}
