//! MCP tool catalogue. Each submodule owns one capability (its schemas and handlers);
//! this module only aggregates and dispatches.

mod actuation;
mod perception;

use ommatix_application::{ActuationService, PerceptionEngine};
use serde::Deserialize;
use serde_json::{Value, json};

/// Outcome of a tool call: `Ok` payloads become `structuredContent`, `Err` messages are
/// reported with `isError: true` so the agent can self-correct (MCP tool-error semantics).
pub type ToolResult = Result<Value, String>;

/// The application services a tool may use, injected by the server.
pub struct Services<'a> {
    pub perception: &'a mut PerceptionEngine,
    pub actuation: &'a mut ActuationService,
}

pub fn definitions() -> Value {
    let mut all = perception::definitions();
    all.extend(actuation::definitions());
    Value::Array(all)
}

/// Dispatches a tool by name. `None` means the tool does not exist.
pub fn call(services: &mut Services<'_>, name: &str, args: &Value) -> Option<ToolResult> {
    perception::call(services.perception, name, args)
        .or_else(|| actuation::call(services.perception, services.actuation, name, args))
}

fn parse<T: for<'de> Deserialize<'de>>(args: &Value) -> Result<T, String> {
    let args = if args.is_null() {
        json!({})
    } else {
        args.clone()
    };
    serde_json::from_value(args).map_err(|e| format!("invalid arguments: {e}"))
}
