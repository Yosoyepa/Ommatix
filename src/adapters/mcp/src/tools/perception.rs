use ommatix_application::PerceptionEngine;
use ommatix_domain::{Point, Rect};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{ToolResult, parse};
use crate::dto::{ReportDto, elements};

pub fn definitions() -> Vec<Value> {
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
    vec![
        json!({
            "name": "perceive",
            "title": "Perceive screen",
            "description": "Capture one frame, re-analyse only the regions that changed and return what was added, updated or removed. Call this after every action you perform.",
            "inputSchema": none,
            "annotations": { "readOnlyHint": true, "idempotentHint": false }
        }),
        json!({
            "name": "list_elements",
            "title": "List elements",
            "description": "Return every element currently on screen in reading order, with bounds and a click target (center).",
            "inputSchema": none,
            "annotations": { "readOnlyHint": true, "idempotentHint": true }
        }),
        json!({
            "name": "element_at",
            "title": "Element at point",
            "description": "Return the elements under a screen coordinate, most specific (smallest) first.",
            "inputSchema": point,
            "annotations": { "readOnlyHint": true, "idempotentHint": true }
        }),
        json!({
            "name": "elements_in_rect",
            "title": "Elements in rectangle",
            "description": "Return the elements intersecting a screen rectangle, in reading order.",
            "inputSchema": rect,
            "annotations": { "readOnlyHint": true, "idempotentHint": true }
        }),
    ]
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NoArgs {}

pub fn call(engine: &mut PerceptionEngine, name: &str, args: &Value) -> Option<ToolResult> {
    let result = match name {
        "perceive" => parse::<NoArgs>(args).and_then(|_| perceive(engine)),
        "list_elements" => parse::<NoArgs>(args).map(|_| {
            json!({
                "frame_no": engine.frame_no(),
                "elements": elements(&engine.elements()),
            })
        }),
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

/// Runs one perception tick and serialises the report. Shared with action tools.
pub fn perceive(engine: &mut PerceptionEngine) -> ToolResult {
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
