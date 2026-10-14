use ommatix_application::PerceptionEngine;
use ommatix_domain::{Point, Rect};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::dto::{ReportDto, elements};

/// Outcome of a tool call: `Ok` payloads become `structuredContent`, `Err` messages are
/// reported with `isError: true` so the agent can self-correct (MCP tool-error semantics).
pub type ToolResult = Result<Value, String>;

pub fn definitions() -> Value {
    let point = json!({
        "type": "object",
        "properties": {
            "x": { "type": "integer", "minimum": 0, "description": "Screen x in pixels" },
            "y": { "type": "integer", "minimum": 0, "description": "Screen y in pixels" }
        },
        "required": ["x", "y"],
        "additionalProperties": false
    });
    let rect = json!({
        "type": "object",
        "properties": {
            "x": { "type": "integer", "minimum": 0 },
            "y": { "type": "integer", "minimum": 0 },
            "w": { "type": "integer", "minimum": 1 },
            "h": { "type": "integer", "minimum": 1 }
        },
        "required": ["x", "y", "w", "h"],
        "additionalProperties": false
    });
    let none = json!({ "type": "object", "properties": {}, "additionalProperties": false });
    json!([
        {
            "name": "perceive",
            "title": "Perceive screen",
            "description": "Capture one frame, re-analyse only the regions that changed and return what was added, updated or removed. Call this after every action you perform.",
            "inputSchema": none,
            "annotations": { "readOnlyHint": true, "idempotentHint": false }
        },
        {
            "name": "list_elements",
            "title": "List elements",
            "description": "Return every element currently on screen in reading order, with bounds and a click target (center).",
            "inputSchema": none,
            "annotations": { "readOnlyHint": true, "idempotentHint": true }
        },
        {
            "name": "element_at",
            "title": "Element at point",
            "description": "Return the elements under a screen coordinate, most specific (smallest) first.",
            "inputSchema": point,
            "annotations": { "readOnlyHint": true, "idempotentHint": true }
        },
        {
            "name": "elements_in_rect",
            "title": "Elements in rectangle",
            "description": "Return the elements intersecting a screen rectangle, in reading order.",
            "inputSchema": rect,
            "annotations": { "readOnlyHint": true, "idempotentHint": true }
        }
    ])
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PointArgs {
    x: u32,
    y: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RectArgs {
    x: u32,
    y: u32,
    w: u32,
    h: u32,
}

/// Dispatches a tool by name. `None` means the tool does not exist.
pub fn call(engine: &mut PerceptionEngine, name: &str, args: Value) -> Option<ToolResult> {
    let result = match name {
        "perceive" => perceive(engine),
        "list_elements" => Ok(json!({
            "frame_no": engine.frame_no(),
            "elements": elements(&engine.elements()),
        })),
        "element_at" => parse::<PointArgs>(args).map(|a| {
            json!({
                "frame_no": engine.frame_no(),
                "elements": elements(&engine.elements_at(Point::new(a.x, a.y))),
            })
        }),
        "elements_in_rect" => parse::<RectArgs>(args).and_then(|a| {
            if a.w == 0 || a.h == 0 {
                return Err("w and h must be >= 1".into());
            }
            Ok(json!({
                "frame_no": engine.frame_no(),
                "elements": elements(&engine.elements_in(&Rect::new(a.x, a.y, a.w, a.h))),
            }))
        }),
        _ => return None,
    };
    Some(result)
}

fn perceive(engine: &mut PerceptionEngine) -> ToolResult {
    match engine.tick() {
        Ok(Some(report)) => {
            serde_json::to_value(ReportDto::from(&report)).map_err(|e| e.to_string())
        }
        Ok(None) => Err(format!(
            "frame source '{}' is exhausted",
            engine.source_name()
        )),
        Err(e) => Err(e.to_string()),
    }
}

fn parse<T: for<'de> Deserialize<'de>>(args: Value) -> Result<T, String> {
    let args = if args.is_null() { json!({}) } else { args };
    serde_json::from_value(args).map_err(|e| format!("invalid arguments: {e}"))
}
