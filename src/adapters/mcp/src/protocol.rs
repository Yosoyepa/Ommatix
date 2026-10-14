//! One MCP connection: JSON-RPC envelope validation, profile selection and method routing.
//!
//! The connection is a synchronous state machine over whole messages (one JSON text per
//! line), so it can be driven by any transport and tested without I/O. Its profile is
//! chosen by the first request that selects one and never changes afterwards:
//!
//! | First selecting request | Profile | Rules |
//! |---|---|---|
//! | any request whose `params._meta` declares `io.modelcontextprotocol/protocolVersion` (incl. `server/discover`) | 2026-07-28 | no `initialize`; every request carries `protocolVersion` + `clientCapabilities` in `_meta`; results carry `resultType: "complete"`; `server/discover` and `tools/list` carry `ttlMs`/`cacheScope` |
//! | `initialize`, only when `legacy_enabled` | 2025-11-25 | `initialize` → `notifications/initialized`; legacy result shapes (no `resultType`, no cache hints); `ping` |
//!
//! Error codes (JSON-RPC): `-32700` unparsable message, `-32600` invalid request (not an
//! object, bad `jsonrpc`/`id`, oversized message, repeated `initialize`), `-32601` unknown
//! method (or a method of the other profile), `-32602` invalid params (missing request
//! metadata, unknown tool, arguments that violate the tool's input schema, a legacy
//! session request that declares another protocol version), `-32022` unsupported protocol
//! version (modern profile; also `initialize` when the legacy profile is disabled),
//! `-32603` internal error. Expected domain errors are tool results with `isError: true`.
//! Notifications are never answered.

use std::sync::{Arc, Mutex};

use ommatix_application::Engine;
use rmcp::model::{
    CacheScope, CallToolResult, ContentBlock, DiscoverResult, ErrorCode, ErrorData, Implementation,
    InitializeResult, JsonObject, ListToolsResult, MetaObject, ProtocolVersion, ServerCapabilities,
    Tool, ToolsCapability,
};
use serde::Serialize;
use serde_json::{Map, Value, json};

use crate::config::McpConfig;
use crate::tools::{self, CallError, OBSERVE_SCREEN, ToolOutput};

/// The modern MCP profile, always served.
pub const MODERN_PROTOCOL_VERSION: &str = "2026-07-28";
/// The legacy MCP profile, served only when [`McpConfig::legacy_enabled`] is set.
pub const LEGACY_PROTOCOL_VERSION: &str = "2025-11-25";

const META_PROTOCOL_VERSION: &str = "io.modelcontextprotocol/protocolVersion";
const META_CLIENT_CAPABILITIES: &str = "io.modelcontextprotocol/clientCapabilities";
const META_SERVER_INFO: &str = "io.modelcontextprotocol/serverInfo";

/// Server name reported in `serverInfo`.
pub const SERVER_NAME: &str = "ommatix";

/// The MCP profile a connection settled on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Profile {
    /// MCP 2026-07-28: per-request metadata, no handshake.
    Modern,
    /// MCP 2025-11-25: `initialize`/`initialized` handshake (opt-in).
    Legacy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Unselected,
    Modern,
    Legacy { initialized: bool },
}

/// A successful reply, before it is encoded for the connection's profile.
enum Reply {
    Value(Value),
    Tool(ToolOutput),
}

/// One client connection over a shared engine.
///
/// Feed it each incoming message with [`Connection::handle_message`]; write every returned
/// string as one line. Messages are handled one at a time, in order.
#[allow(missing_debug_implementations)] // holds the engine (trait objects)
pub struct Connection {
    engine: Arc<Mutex<Engine>>,
    config: McpConfig,
    tools: Arc<[Tool]>,
    state: State,
}

impl Connection {
    pub(crate) fn new(engine: Arc<Mutex<Engine>>, config: McpConfig, tools: Arc<[Tool]>) -> Self {
        Self {
            engine,
            config,
            tools,
            state: State::Unselected,
        }
    }

    /// The profile chosen so far, if any.
    pub fn profile(&self) -> Option<Profile> {
        match self.state {
            State::Unselected => None,
            State::Modern => Some(Profile::Modern),
            State::Legacy { .. } => Some(Profile::Legacy),
        }
    }

    /// Whether a legacy client has sent `notifications/initialized`.
    pub fn legacy_initialized(&self) -> bool {
        matches!(self.state, State::Legacy { initialized: true })
    }

    /// Handles one complete message (without its line terminator). Returns the response
    /// line to send, or `None` for notifications, client responses and blank input. The
    /// response is never longer than the configured message limit.
    ///
    /// The caller must have checked the incoming size limit (see
    /// [`Connection::oversized_message_response`]).
    pub fn handle_message(&mut self, message: &[u8]) -> Option<String> {
        let message = message.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(message);
        if message.iter().all(u8::is_ascii_whitespace) {
            return None;
        }
        let Ok(value) = serde_json::from_slice::<Value>(message) else {
            return Some(self.encode_error(
                &Value::Null,
                &ErrorData::parse_error("Parse error: the message is not valid JSON", None),
            ));
        };
        let Value::Object(envelope) = value else {
            return Some(self.encode_error(
                &Value::Null,
                &ErrorData::invalid_request(
                    "Invalid request: expected one JSON-RPC object (batches are not supported)",
                    None,
                ),
            ));
        };
        let (id, body) = self.dispatch(&envelope)?;
        Some(match body {
            Ok(reply) => self.encode_reply(&id, reply),
            Err(error) => self.encode_error(&id, &error),
        })
    }

    /// The response to an incoming message larger than the limit, which was discarded
    /// without being parsed (so its `id` is unknown and `null` is used).
    pub fn oversized_message_response(&self) -> String {
        let limit = self.config.message_limit();
        self.encode_error(
            &Value::Null,
            &ErrorData::invalid_request(
                format!("Invalid request: the message exceeds the {limit}-byte limit"),
                Some(json!({ "maxMessageBytes": limit })),
            ),
        )
    }

    // ---- envelope ----

    /// Returns `None` when nothing must be sent back.
    fn dispatch(
        &mut self,
        envelope: &Map<String, Value>,
    ) -> Option<(Value, Result<Reply, ErrorData>)> {
        let id = envelope.get("id");
        let Some(method) = envelope.get("method") else {
            // A response from the client: this server sends no requests, so drop it.
            if envelope.contains_key("result") || envelope.contains_key("error") {
                return None;
            }
            let id = id.filter(|id| valid_id(id)).cloned().unwrap_or(Value::Null);
            return Some((
                id,
                Err(ErrorData::invalid_request(
                    "Invalid request: no method",
                    None,
                )),
            ));
        };
        let Some(id) = id else {
            // A notification (even a malformed one) never gets a response.
            if let Value::String(method) = method {
                self.notification(method);
            }
            return None;
        };
        if !valid_id(id) {
            return Some((
                Value::Null,
                Err(ErrorData::invalid_request(
                    "Invalid request: id must be a string or an integer",
                    None,
                )),
            ));
        }
        let id = id.clone();
        if envelope.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
            return Some((
                id,
                Err(ErrorData::invalid_request(
                    "Invalid request: jsonrpc must be \"2.0\"",
                    None,
                )),
            ));
        }
        let Value::String(method) = method else {
            return Some((
                id,
                Err(ErrorData::invalid_request(
                    "Invalid request: method must be a string",
                    None,
                )),
            ));
        };
        let empty = Map::new();
        let params = match envelope.get("params") {
            None => &empty,
            Some(Value::Object(params)) => params,
            Some(_) => {
                return Some((
                    id,
                    Err(ErrorData::invalid_params(
                        "Invalid params: params must be an object",
                        None,
                    )),
                ));
            }
        };
        let body = self.request(method, params);
        Some((id, body))
    }

    fn notification(&mut self, method: &str) {
        if method == "notifications/initialized"
            && let State::Legacy { initialized } = &mut self.state
        {
            *initialized = true;
        }
        // Everything else (cancellation, progress, unknown methods) needs no action:
        // requests are handled synchronously, so nothing is ever in flight.
    }

    // ---- profiles ----

    fn request(&mut self, method: &str, params: &Map<String, Value>) -> Result<Reply, ErrorData> {
        let meta = match params.get("_meta") {
            None => None,
            Some(Value::Object(meta)) => Some(meta),
            Some(_) => {
                return Err(ErrorData::invalid_params(
                    "Invalid params: _meta must be an object",
                    None,
                ));
            }
        };
        match self.state {
            State::Unselected => {
                if method == "initialize" {
                    return self.initialize(params);
                }
                if meta.is_some_and(|m| m.contains_key(META_PROTOCOL_VERSION)) {
                    check_modern_meta(meta)?;
                    self.state = State::Modern;
                    return self.modern(method, params);
                }
                if self.config.legacy_enabled && method == "ping" {
                    // The legacy lifecycle allows ping before initialize.
                    return Ok(Reply::Value(json!({})));
                }
                // No protocol version and no handshake: no profile can serve it.
                Err(check_modern_meta(meta)
                    .err()
                    .unwrap_or_else(|| ErrorData::invalid_params("request _meta is missing", None)))
            }
            State::Modern => {
                if method == "initialize" {
                    return Err(initialize_unavailable(params));
                }
                check_modern_meta(meta)?;
                self.modern(method, params)
            }
            State::Legacy { .. } => {
                if let Some(declared) = meta.and_then(|m| m.get(META_PROTOCOL_VERSION))
                    && declared.as_str() != Some(LEGACY_PROTOCOL_VERSION)
                {
                    return Err(ErrorData::invalid_params(
                        "Unsupported protocol version",
                        Some(json!({
                            "supported": [LEGACY_PROTOCOL_VERSION],
                            "requested": declared,
                        })),
                    ));
                }
                self.legacy(method, params)
            }
        }
    }

    fn modern(&self, method: &str, params: &Map<String, Value>) -> Result<Reply, ErrorData> {
        match method {
            "server/discover" => {
                let mut result = DiscoverResult::new(self.supported_versions(), capabilities());
                result.meta = Some(server_meta());
                to_reply(&result)
            }
            "tools/list" => {
                reject_list_cursor(params)?;
                let mut result = ListToolsResult::with_all_items(self.tools.to_vec())
                    .with_ttl_ms(0)
                    .with_cache_scope(CacheScope::Private);
                result.meta = Some(server_meta());
                to_reply(&result)
            }
            "tools/call" => self.call_tool(params),
            _ => Err(method_not_found(method)),
        }
    }

    fn legacy(&self, method: &str, params: &Map<String, Value>) -> Result<Reply, ErrorData> {
        match method {
            "initialize" => Err(ErrorData::invalid_request(
                "Invalid request: the connection is already initialized",
                None,
            )),
            "ping" => Ok(Reply::Value(json!({}))),
            "tools/list" => {
                reject_list_cursor(params)?;
                let mut result = ListToolsResult::with_all_items(self.tools.to_vec());
                result.result_type = None;
                to_reply(&result)
            }
            "tools/call" => self.call_tool(params),
            _ => Err(method_not_found(method)),
        }
    }

    /// `initialize` on a connection that has not chosen a profile yet.
    fn initialize(&mut self, params: &Map<String, Value>) -> Result<Reply, ErrorData> {
        if !self.config.legacy_enabled {
            return Err(initialize_unavailable(params));
        }
        let well_formed = params.get("protocolVersion").is_some_and(Value::is_string)
            && params.get("capabilities").is_some_and(Value::is_object)
            && params.get("clientInfo").is_some_and(Value::is_object);
        if !well_formed {
            return Err(ErrorData::invalid_params(
                "Invalid params: initialize needs protocolVersion, capabilities and clientInfo",
                None,
            ));
        }
        // 2025-11-25 negotiation: echo the requested version when supported, otherwise
        // answer with the one this server supports and let the client decide.
        let result = InitializeResult::new(capabilities())
            .with_protocol_version(ProtocolVersion::V_2025_11_25)
            .with_server_info(server_info());
        self.state = State::Legacy { initialized: false };
        to_reply(&result)
    }

    fn call_tool(&self, params: &Map<String, Value>) -> Result<Reply, ErrorData> {
        let Some(name) = params.get("name").and_then(Value::as_str) else {
            return Err(ErrorData::invalid_params(
                "Invalid params: tools/call needs a tool name",
                None,
            ));
        };
        let empty = Value::Object(Map::new());
        let arguments = params.get("arguments").unwrap_or(&empty);
        match tools::call(&self.engine, name, arguments) {
            Ok(output) => Ok(Reply::Tool(output)),
            Err(CallError::UnknownTool) => Err(ErrorData::invalid_params(
                "Unknown tool",
                Some(json!({ "tool": name })),
            )),
            Err(CallError::InvalidArguments(e)) => Err(ErrorData::invalid_params(
                format!("Invalid arguments for tool {name}: {e}"),
                None,
            )),
        }
    }

    fn supported_versions(&self) -> Vec<ProtocolVersion> {
        let mut versions = vec![ProtocolVersion::V_2026_07_28];
        if self.config.legacy_enabled {
            versions.push(ProtocolVersion::V_2025_11_25);
        }
        versions
    }

    // ---- encoding ----

    fn encode_reply(&self, id: &Value, reply: Reply) -> String {
        let limit = self.config.message_limit();
        match reply {
            Reply::Value(value) => fit(id, &envelope(id, Some(&value), None), limit),
            Reply::Tool(output) => encode_tool(id, output, self.state == State::Modern, limit),
        }
    }

    fn encode_error(&self, id: &Value, error: &ErrorData) -> String {
        fit(
            id,
            &envelope(id, None, Some(error)),
            self.config.message_limit(),
        )
    }
}

/// Encodes a tool result for the profile, within `limit` bytes: an observation page that
/// does not fit becomes a `LIMIT_EXCEEDED` tool error asking for a smaller request.
fn encode_tool(id: &Value, output: ToolOutput, modern: bool, limit: usize) -> String {
    let observe = output.tool == OBSERVE_SCREEN;
    let line = envelope(id, Some(&call_result(output, modern)), None);
    if line.len() <= limit || !observe {
        return fit(id, &line, limit);
    }
    let reduced = call_result(tools::limit_exceeded(limit), modern);
    fit(id, &envelope(id, Some(&reduced), None), limit)
}

/// Never emits more than the limit: an oversized line becomes an internal error, and if
/// even that does not fit (a huge request id), one without the id.
fn fit(id: &Value, line: &str, limit: usize) -> String {
    if line.len() <= limit {
        return line.to_owned();
    }
    let error = ErrorData::internal_error(
        format!("Internal error: the response exceeds the {limit}-byte limit"),
        None,
    );
    let with_id = envelope(id, None, Some(&error));
    if with_id.len() <= limit {
        with_id
    } else {
        envelope(&Value::Null, None, Some(&error))
    }
}

/// A `CallToolResult`: the Ommatix object as `structuredContent` and as the single text
/// block; `resultType` and `serverInfo` only for the modern profile; run-dependent values
/// in `_meta`.
fn call_result(output: ToolOutput, modern: bool) -> Value {
    let mut result = CallToolResult::success(vec![ContentBlock::text(output.text)]);
    result.structured_content = Some(output.structured);
    result.is_error = Some(output.is_error);
    let mut meta = JsonObject::new();
    if modern {
        meta.insert(META_SERVER_INFO.to_owned(), server_info_value());
    } else {
        result.result_type = None;
    }
    for (key, value) in output.meta {
        meta.insert(key.to_owned(), value);
    }
    if !meta.is_empty() {
        result.meta = Some(MetaObject(meta));
    }
    serde_json::to_value(&result).unwrap_or(Value::Null)
}

#[derive(Serialize)]
struct Envelope<'a> {
    jsonrpc: &'static str,
    id: &'a Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<&'a Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<&'a ErrorData>,
}

fn envelope(id: &Value, result: Option<&Value>, error: Option<&ErrorData>) -> String {
    let message = Envelope {
        jsonrpc: "2.0",
        id,
        result,
        error,
    };
    serde_json::to_string(&message).unwrap_or_else(|_| {
        r#"{"jsonrpc":"2.0","id":null,"error":{"code":-32603,"message":"Internal error"}}"#
            .to_owned()
    })
}

fn to_reply<T: Serialize>(result: &T) -> Result<Reply, ErrorData> {
    serde_json::to_value(result)
        .map(Reply::Value)
        .map_err(|_| ErrorData::internal_error("Internal error: unserialisable result", None))
}

/// MCP request ids are strings or integers.
fn valid_id(id: &Value) -> bool {
    match id {
        Value::String(_) => true,
        Value::Number(n) => n.is_i64() || n.is_u64(),
        _ => false,
    }
}

/// The per-request metadata the modern profile requires.
fn check_modern_meta(meta: Option<&Map<String, Value>>) -> Result<(), ErrorData> {
    let version = meta.and_then(|m| m.get(META_PROTOCOL_VERSION));
    if let Some(Value::String(version)) = version
        && version != MODERN_PROTOCOL_VERSION
    {
        return Err(unsupported_version(&Value::String(version.clone())));
    }
    let mut missing = Vec::new();
    if !version.is_some_and(Value::is_string) {
        missing.push(META_PROTOCOL_VERSION);
    }
    if !meta
        .and_then(|m| m.get(META_CLIENT_CAPABILITIES))
        .is_some_and(Value::is_object)
    {
        missing.push(META_CLIENT_CAPABILITIES);
    }
    if missing.is_empty() {
        Ok(())
    } else {
        Err(ErrorData::invalid_params(
            format!(
                "request _meta is missing or has malformed required fields: {}",
                missing.join(", ")
            ),
            None,
        ))
    }
}

/// `-32022` with the versions usable without a handshake.
fn unsupported_version(requested: &Value) -> ErrorData {
    ErrorData::new(
        ErrorCode::UNSUPPORTED_PROTOCOL_VERSION,
        "Unsupported protocol version",
        Some(json!({
            "requested": requested,
            "supported": [MODERN_PROTOCOL_VERSION],
        })),
    )
}

/// `initialize` where no handshake version is served (legacy disabled, or a connection
/// already on the modern profile): the modern profile's unsupported-version error.
fn initialize_unavailable(params: &Map<String, Value>) -> ErrorData {
    unsupported_version(params.get("protocolVersion").unwrap_or(&Value::Null))
}

fn method_not_found(method: &str) -> ErrorData {
    ErrorData::new(
        ErrorCode::METHOD_NOT_FOUND,
        "Method not found",
        Some(json!({ "method": method })),
    )
}

/// The tool list is a single page: a cursor can only be one this server never issued.
fn reject_list_cursor(params: &Map<String, Value>) -> Result<(), ErrorData> {
    if params.contains_key("cursor") {
        Err(ErrorData::invalid_params(
            "Invalid params: tools/list has a single page; no cursor was issued",
            None,
        ))
    } else {
        Ok(())
    }
}

fn capabilities() -> ServerCapabilities {
    let mut capabilities = ServerCapabilities::default();
    capabilities.tools = Some(ToolsCapability::default());
    capabilities
}

fn server_info() -> Implementation {
    Implementation::new(SERVER_NAME, env!("CARGO_PKG_VERSION"))
}

fn server_info_value() -> Value {
    serde_json::to_value(server_info()).unwrap_or(Value::Null)
}

fn server_meta() -> MetaObject {
    let mut meta = JsonObject::new();
    meta.insert(META_SERVER_INFO.to_owned(), server_info_value());
    MetaObject(meta)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn observe_output(elements: usize) -> ToolOutput {
        let element = json!({
            "target_id": "e1", "revision": 1, "role": "text_node", "label": "x".repeat(128),
            "label_source": "ocr", "actionability": "unknown", "enabled": null,
            "visible": true, "zone": "unknown"
        });
        let page = json!({
            "schema_version": "1.0", "session_id": "s", "snapshot_id": "s1",
            "geometry_epoch": 1, "frame_seq": 1, "mode": "full", "age_ms": 0,
            "degraded": false, "elements": vec![element; elements],
            "total_elements": elements, "truncated": false
        });
        ToolOutput {
            tool: OBSERVE_SCREEN,
            text: page.to_string(),
            structured: page,
            is_error: false,
            meta: Vec::new(),
        }
    }

    #[test]
    fn ct_10_an_oversized_observation_becomes_limit_exceeded() {
        let limit = 64 * 1024;
        let line = encode_tool(&json!(1), observe_output(200), true, limit);
        assert!(line.len() <= limit);
        let response: Value = serde_json::from_str(&line).unwrap();
        let result = &response["result"];
        assert_eq!(result["isError"], true);
        assert_eq!(
            result["structuredContent"]["error"]["code"],
            "LIMIT_EXCEEDED"
        );
        assert_eq!(
            result["structuredContent"]["error"]["next_step"],
            "reduce_request"
        );
        assert_eq!(result["resultType"], "complete");

        // A page that fits is sent as is.
        let line = encode_tool(&json!(1), observe_output(20), true, limit);
        let response: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(response["result"]["isError"], false);
    }

    #[test]
    fn legacy_tool_results_have_no_modern_fields() {
        let result = call_result(observe_output(1), false);
        assert!(result.get("resultType").is_none());
        assert!(result.get("_meta").is_none());
    }
}
