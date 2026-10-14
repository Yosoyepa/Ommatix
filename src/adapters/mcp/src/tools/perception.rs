use ommatix_application::{AgentSession, error_chain};
use ommatix_domain::{Point, Rect};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{Tool, ToolOutput, ToolResult, parse, to_value};
use crate::dto::{ReportDto, elements, timings_meta};

pub(super) const TOOLS: [Tool; 4] = [
    Tool {
        name: "perceive",
        spec: perceive_spec,
        run: perceive,
    },
    Tool {
        name: "list_elements",
        spec: list_spec,
        run: list_elements,
    },
    Tool {
        name: "element_at",
        spec: element_at_spec,
        run: element_at,
    },
    Tool {
        name: "elements_in_rect",
        spec: elements_in_rect_spec,
        run: elements_in_rect,
    },
];

fn no_arguments() -> Value {
    json!({ "type": "object", "properties": {}, "additionalProperties": false })
}

fn perceive_spec() -> Value {
    json!({
        "title": "Perceive screen",
        "description": "Capture one frame, re-analyse only the regions that changed and return what was added, updated or removed. Actions already observe their own effect; call this to look again.",
        "inputSchema": no_arguments(),
        "annotations": { "readOnlyHint": true, "idempotentHint": false }
    })
}

fn list_spec() -> Value {
    json!({
        "title": "List elements",
        "description": "Return every element currently on screen in reading order, with bounds and a click target (center).",
        "inputSchema": no_arguments(),
        "annotations": { "readOnlyHint": true, "idempotentHint": true }
    })
}

fn element_at_spec() -> Value {
    json!({
        "title": "Element at point",
        "description": "Return the elements under a screen coordinate, most specific (smallest) first.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "x": { "type": "integer", "minimum": 0, "description": "Screen x in pixels" },
                "y": { "type": "integer", "minimum": 0, "description": "Screen y in pixels" }
            },
            "required": ["x", "y"],
            "additionalProperties": false
        },
        "annotations": { "readOnlyHint": true, "idempotentHint": true }
    })
}

fn elements_in_rect_spec() -> Value {
    json!({
        "title": "Elements in rectangle",
        "description": "Return the elements intersecting a screen rectangle, in reading order.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "x": { "type": "integer", "minimum": 0 },
                "y": { "type": "integer", "minimum": 0 },
                "w": { "type": "integer", "minimum": 1 },
                "h": { "type": "integer", "minimum": 1 }
            },
            "required": ["x", "y", "w", "h"],
            "additionalProperties": false
        },
        "annotations": { "readOnlyHint": true, "idempotentHint": true }
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NoArgs {}

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

fn perceive(session: &mut AgentSession, args: &Value) -> ToolResult {
    parse::<NoArgs>(args)?;
    let report = session.perceive().map_err(|e| error_chain(&e))?;
    Ok(ToolOutput {
        structured: to_value(ReportDto::from(&report))?,
        meta: Some(timings_meta(&report.timings)),
    })
}

fn list_elements(session: &mut AgentSession, args: &Value) -> ToolResult {
    parse::<NoArgs>(args)?;
    let engine = session.perception();
    answer(engine.frame_no(), &engine.elements())
}

fn element_at(session: &mut AgentSession, args: &Value) -> ToolResult {
    let a = parse::<PointArgs>(args)?;
    let engine = session.perception();
    answer(engine.frame_no(), &engine.elements_at(Point::new(a.x, a.y)))
}

fn elements_in_rect(session: &mut AgentSession, args: &Value) -> ToolResult {
    let a = parse::<RectArgs>(args)?;
    if a.w == 0 || a.h == 0 {
        return Err("w and h must be at least 1".into());
    }
    if a.x.checked_add(a.w).is_none() || a.y.checked_add(a.h).is_none() {
        return Err("the rectangle exceeds the coordinate range".into());
    }
    let engine = session.perception();
    let mut rect = Rect::new(a.x, a.y, a.w, a.h);
    if let Some(screen) = engine.screen_bounds() {
        rect = rect.clip(&screen);
    }
    let found = if rect.is_empty() {
        Vec::new()
    } else {
        engine.elements_in(&rect)
    };
    answer(engine.frame_no(), &found)
}

fn answer(frame_no: u64, found: &[ommatix_domain::Element]) -> ToolResult {
    Ok(ToolOutput::new(
        json!({ "frame_no": frame_no, "elements": to_value(elements(found))? }),
    ))
}
