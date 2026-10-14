//! JSON-RPC 2.0 envelope: validation of incoming messages and response building. Knows
//! nothing about MCP methods.

use serde_json::{Map, Value, json};

pub(crate) const PARSE_ERROR: i64 = -32700;
pub(crate) const INVALID_REQUEST: i64 = -32600;
pub(crate) const METHOD_NOT_FOUND: i64 = -32601;
pub(crate) const INVALID_PARAMS: i64 = -32602;

/// A JSON-RPC error a method can return.
pub(crate) type MethodError = (i64, String);

/// A request (`id` present) or notification (`id` absent) with a valid envelope.
pub(crate) struct Call<'a> {
    pub id: Option<&'a Value>,
    pub method: &'a str,
    /// The `params` object, or `Null` when absent.
    pub params: &'a Value,
}

pub(crate) enum Incoming<'a> {
    Call(Call<'a>),
    /// A response from the client. This server never sends requests, so it is ignored.
    Response,
    /// Not a valid JSON-RPC 2.0 message; answered with `INVALID_REQUEST`.
    Invalid {
        id: Value,
        reason: &'static str,
    },
}

/// Classifies one message (one element of a batch, or a whole non-batch message).
pub(crate) fn classify(msg: &Value) -> Incoming<'_> {
    let Some(obj) = msg.as_object() else {
        return invalid(Value::Null, "a message must be a JSON object");
    };
    let id = obj.get("id");
    if id.is_some_and(|id| !(id.is_string() || id.is_number() || id.is_null())) {
        return invalid(Value::Null, "id must be a string, a number or null");
    }
    let echo_id = id.cloned().unwrap_or(Value::Null);
    if obj.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return invalid(echo_id, "jsonrpc must be \"2.0\"");
    }
    match obj.get("method") {
        Some(Value::String(method)) => {
            let params = obj.get("params").unwrap_or(&Value::Null);
            if !(params.is_object() || params.is_null()) {
                return invalid(echo_id, "params must be an object");
            }
            Incoming::Call(Call { id, method, params })
        }
        Some(_) => invalid(echo_id, "method must be a string"),
        None if obj.contains_key("result") || obj.contains_key("error") => Incoming::Response,
        None => invalid(echo_id, "missing method"),
    }
}

fn invalid(id: Value, reason: &'static str) -> Incoming<'static> {
    Incoming::Invalid { id, reason }
}

pub(crate) fn success(id: &Value, result: Value) -> Value {
    let mut response = Map::new();
    response.insert("jsonrpc".into(), "2.0".into());
    response.insert("id".into(), id.clone());
    response.insert("result".into(), result);
    Value::Object(response)
}

pub(crate) fn failure(id: &Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reason(msg: &Value) -> Option<&'static str> {
        match classify(msg) {
            Incoming::Invalid { reason, .. } => Some(reason),
            _ => None,
        }
    }

    #[test]
    fn rejects_malformed_envelopes() {
        assert!(reason(&json!(42)).is_some());
        assert!(reason(&json!({ "jsonrpc": "1.0", "id": 1, "method": "ping" })).is_some());
        assert!(reason(&json!({ "jsonrpc": "2.0", "id": [1], "method": "ping" })).is_some());
        assert!(reason(&json!({ "jsonrpc": "2.0", "id": 1, "method": 7 })).is_some());
        assert!(reason(&json!({ "jsonrpc": "2.0", "id": 1 })).is_some());
        assert!(
            reason(&json!({ "jsonrpc": "2.0", "id": 1, "method": "ping", "params": 3 })).is_some()
        );
    }

    #[test]
    fn tells_requests_notifications_and_responses_apart() {
        let request = json!({ "jsonrpc": "2.0", "id": "a", "method": "ping" });
        assert!(matches!(
            classify(&request),
            Incoming::Call(Call { id: Some(_), .. })
        ));
        let note = json!({ "jsonrpc": "2.0", "method": "notifications/initialized" });
        assert!(matches!(
            classify(&note),
            Incoming::Call(Call { id: None, .. })
        ));
        let response = json!({ "jsonrpc": "2.0", "id": 9, "result": {} });
        assert!(matches!(classify(&response), Incoming::Response));
    }
}
