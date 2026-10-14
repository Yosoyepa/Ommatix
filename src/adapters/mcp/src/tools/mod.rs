//! MCP tool registry. Each submodule owns one capability (its schemas and handlers);
//! this module only lists the tools and parses arguments.

mod actuation;
mod perception;

use ommatix_application::AgentSession;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};

/// Payload of a successful call: `structured` becomes `structuredContent`, `meta` (run-
/// dependent data such as timings) becomes the result's `_meta`.
pub(crate) struct ToolOutput {
    pub structured: Value,
    pub meta: Option<Value>,
}

impl ToolOutput {
    fn new(structured: Value) -> Self {
        Self {
            structured,
            meta: None,
        }
    }
}

/// `Err` messages are reported with `isError: true` so the agent can self-correct (MCP
/// tool-error semantics), as opposed to JSON-RPC errors for protocol misuse.
pub(crate) type ToolResult = Result<ToolOutput, String>;

/// One tool: its name, its MCP definition (everything but the name) and its handler.
pub(crate) struct Tool {
    pub name: &'static str,
    pub spec: fn() -> Value,
    pub run: fn(&mut AgentSession, &Value) -> ToolResult,
}

fn all() -> impl Iterator<Item = &'static Tool> {
    perception::TOOLS.iter().chain(actuation::TOOLS.iter())
}

pub(crate) fn find(name: &str) -> Option<&'static Tool> {
    all().find(|t| t.name == name)
}

/// The `tools/list` catalogue, in registry order.
pub(crate) fn definitions() -> Value {
    all()
        .map(|tool| {
            let mut def = Map::new();
            def.insert("name".into(), tool.name.into());
            if let Value::Object(spec) = (tool.spec)() {
                def.extend(spec);
            }
            Value::Object(def)
        })
        .collect()
}

/// Deserialises tool arguments, which must be an object (serde would otherwise take an
/// array positionally); absent arguments mean `{}`.
fn parse<T: DeserializeOwned>(args: &Value) -> Result<T, String> {
    let args = match args {
        Value::Null => Value::Object(Map::new()),
        Value::Object(_) => args.clone(),
        _ => return Err("invalid arguments: expected an object".into()),
    };
    serde_json::from_value(args).map_err(|e| format!("invalid arguments: {e}"))
}

fn to_value(dto: impl serde::Serialize) -> Result<Value, String> {
    serde_json::to_value(dto).map_err(|e| format!("cannot encode the result: {e}"))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    /// Verification N12: arguments are named, never positional.
    #[test]
    fn arguments_must_be_an_object() {
        #[derive(Debug, serde::Deserialize)]
        struct Point {
            x: u32,
            y: u32,
        }
        let point: Point = parse(&serde_json::json!({ "x": 540, "y": 316 })).unwrap();
        assert_eq!((point.x, point.y), (540, 316));
        for args in [serde_json::json!([540, 316]), serde_json::json!("e1")] {
            let err = parse::<Point>(&args).unwrap_err();
            assert!(err.contains("expected an object"), "{args}: {err}");
        }
    }

    #[test]
    fn names_are_unique_and_definitions_complete() {
        let names: BTreeSet<&str> = all().map(|t| t.name).collect();
        assert_eq!(names.len(), all().count());
        for def in definitions().as_array().unwrap() {
            for field in ["name", "title", "description", "inputSchema", "annotations"] {
                assert!(def.get(field).is_some(), "{def} lacks {field}");
            }
            assert_eq!(def["inputSchema"]["type"], "object");
        }
    }
}
