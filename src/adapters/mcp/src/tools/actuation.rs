use ommatix_application::{ActionReceipt, ActuationService, ClickTarget, PerceptionEngine};
use ommatix_domain::{Key, MouseButton, Point};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{ToolResult, parse, perception};
use crate::dto::{ReceiptDto, parse_element_id};

pub fn definitions() -> Vec<Value> {
    let observe = json!({
        "type": "boolean",
        "default": true,
        "description": "Perceive right after acting and include the resulting change set."
    });
    vec![
        json!({
            "name": "click",
            "title": "Click",
            "description": "Click an element (by id from perception, preferred) or a screen coordinate. The target must be on screen.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "element_id": { "type": "string", "pattern": "^e[0-9]+$", "description": "Element id, e.g. \"e4\"" },
                    "x": { "type": "integer", "minimum": 0 },
                    "y": { "type": "integer", "minimum": 0 },
                    "button": { "type": "string", "enum": ["left", "right", "middle"], "default": "left" },
                    "observe": observe
                },
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": true, "openWorldHint": true }
        }),
        json!({
            "name": "type_text",
            "title": "Type text",
            "description": "Type text into the focused control. Click the field first.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "text": { "type": "string", "minLength": 1 },
                    "observe": observe
                },
                "required": ["text"],
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": true, "openWorldHint": true }
        }),
        json!({
            "name": "press_key",
            "title": "Press key",
            "description": "Press a non-character key.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "key": {
                        "type": "string",
                        "enum": ["enter", "tab", "escape", "backspace", "delete", "space",
                                 "arrow_up", "arrow_down", "arrow_left", "arrow_right",
                                 "home", "end", "page_up", "page_down"]
                    },
                    "observe": observe
                },
                "required": ["key"],
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": true, "openWorldHint": true }
        }),
    ]
}

const fn yes() -> bool {
    true
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ClickArgs {
    element_id: Option<String>,
    x: Option<u32>,
    y: Option<u32>,
    #[serde(default)]
    button: MouseButton,
    #[serde(default = "yes")]
    observe: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TypeArgs {
    text: String,
    #[serde(default = "yes")]
    observe: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct KeyArgs {
    key: Key,
    #[serde(default = "yes")]
    observe: bool,
}

pub fn call(
    perception: &mut PerceptionEngine,
    actuation: &mut ActuationService,
    name: &str,
    args: &Value,
) -> Option<ToolResult> {
    let result = match name {
        "click" => parse::<ClickArgs>(args).and_then(|a| {
            let target = click_target(&a)?;
            let receipt = actuation
                .click(perception, target, a.button)
                .map_err(|e| e.to_string())?;
            respond(perception, &receipt, a.observe)
        }),
        "type_text" => parse::<TypeArgs>(args).and_then(|a| {
            let receipt = actuation.type_text(&a.text).map_err(|e| e.to_string())?;
            respond(perception, &receipt, a.observe)
        }),
        "press_key" => parse::<KeyArgs>(args).and_then(|a| {
            let receipt = actuation.press_key(a.key).map_err(|e| e.to_string())?;
            respond(perception, &receipt, a.observe)
        }),
        _ => return None,
    };
    Some(result)
}

fn click_target(a: &ClickArgs) -> Result<ClickTarget, String> {
    match (&a.element_id, a.x, a.y) {
        (Some(id), None, None) => parse_element_id(id).map(ClickTarget::Element),
        (None, Some(x), Some(y)) => Ok(ClickTarget::Point(Point::new(x, y))),
        _ => Err("provide either element_id, or both x and y".into()),
    }
}

fn respond(
    perception: &mut PerceptionEngine,
    receipt: &ActionReceipt,
    observe: bool,
) -> ToolResult {
    let observed = if observe {
        Some(perception::perceive(perception)?)
    } else {
        None
    };
    Ok(json!({ "performed": ReceiptDto::from(receipt), "observed": observed }))
}
