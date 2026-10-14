use ommatix_application::{ActionOutcome, AgentSession, ClickTarget, error_chain};
use ommatix_domain::Point;
use serde::Deserialize;
use serde_json::{Map, Value, json};

use super::{Tool, ToolOutput, ToolResult, parse, to_value};
use crate::dto::{
    BUTTONS, KEYS, ObservationDto, ReceiptDto, elapsed_meta, parse_button, parse_element_id,
    parse_key,
};

pub(super) const TOOLS: [Tool; 3] = [
    Tool {
        name: "click",
        spec: click_spec,
        run: click,
    },
    Tool {
        name: "type_text",
        spec: type_text_spec,
        run: type_text,
    },
    Tool {
        name: "press_key",
        spec: press_key_spec,
        run: press_key,
    },
];

fn observe_property() -> Value {
    json!({
        "type": "boolean",
        "default": true,
        "description": "Watch the screen after acting until it settles and report what changed."
    })
}

fn acts() -> Value {
    json!({ "readOnlyHint": false, "destructiveHint": true, "openWorldHint": true })
}

fn click_spec() -> Value {
    let buttons: Vec<&str> = BUTTONS.iter().map(|(name, _)| *name).collect();
    json!({
        "title": "Click",
        "description": "Click an element (by id from perception, preferred) or a screen coordinate. The target must be on screen.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "element_id": { "type": "string", "pattern": "^e[0-9]+$", "description": "Element id, e.g. \"e4\"" },
                "x": { "type": "integer", "minimum": 0 },
                "y": { "type": "integer", "minimum": 0 },
                "button": { "type": "string", "enum": buttons, "default": "left" },
                "observe": observe_property()
            },
            "additionalProperties": false
        },
        "annotations": acts()
    })
}

fn type_text_spec() -> Value {
    json!({
        "title": "Type text",
        "description": "Type printable text into the focused control (click the field first). Control characters are rejected: send Enter, Tab or Backspace with press_key.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "text": { "type": "string", "minLength": 1 },
                "observe": observe_property()
            },
            "required": ["text"],
            "additionalProperties": false
        },
        "annotations": acts()
    })
}

fn press_key_spec() -> Value {
    let keys: Vec<&str> = KEYS.iter().map(|(name, _)| *name).collect();
    json!({
        "title": "Press key",
        "description": "Press a non-character key.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "key": { "type": "string", "enum": keys },
                "observe": observe_property()
            },
            "required": ["key"],
            "additionalProperties": false
        },
        "annotations": acts()
    })
}

const fn yes() -> bool {
    true
}

fn left() -> String {
    "left".into()
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ClickArgs {
    element_id: Option<String>,
    x: Option<u32>,
    y: Option<u32>,
    #[serde(default = "left")]
    button: String,
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
    key: String,
    #[serde(default = "yes")]
    observe: bool,
}

fn click(session: &mut AgentSession, args: &Value) -> ToolResult {
    let a = parse::<ClickArgs>(args)?;
    let target = match (&a.element_id, a.x, a.y) {
        (Some(id), None, None) => ClickTarget::Element(parse_element_id(id)?),
        (None, Some(x), Some(y)) => ClickTarget::Point(Point::new(x, y)),
        _ => return Err("provide either element_id, or both x and y".into()),
    };
    let button = parse_button(&a.button)?;
    let outcome = session
        .click(target, button, a.observe)
        .map_err(|e| error_chain(&e))?;
    respond(&outcome)
}

fn type_text(session: &mut AgentSession, args: &Value) -> ToolResult {
    let a = parse::<TypeArgs>(args)?;
    let outcome = session
        .type_text(&a.text, a.observe)
        .map_err(|e| error_chain(&e))?;
    respond(&outcome)
}

fn press_key(session: &mut AgentSession, args: &Value) -> ToolResult {
    let a = parse::<KeyArgs>(args)?;
    let outcome = session
        .press_key(parse_key(&a.key)?, a.observe)
        .map_err(|e| error_chain(&e))?;
    respond(&outcome)
}

/// The action happened, so the call succeeds even if watching its effect failed: the
/// agent must not retry an action that was performed. What was seen before such a failure
/// is still reported.
fn respond(outcome: &ActionOutcome) -> ToolResult {
    let mut body = Map::new();
    body.insert(
        "performed".into(),
        to_value(ReceiptDto::from(&outcome.receipt))?,
    );
    let (observation, error) = match &outcome.observation {
        None => return Ok(ToolOutput::new(Value::Object(body))),
        Some(Ok(observation)) => (observation, None),
        Some(Err(e)) => (e.partial.as_ref(), Some(error_chain(e))),
    };
    if observation.frames > 0 {
        body.insert(
            "observation".into(),
            to_value(ObservationDto::from(observation))?,
        );
    }
    if let Some(error) = error {
        body.insert("observe_error".into(), error.into());
    }
    Ok(ToolOutput {
        structured: Value::Object(body),
        meta: Some(elapsed_meta(observation.elapsed)),
    })
}
