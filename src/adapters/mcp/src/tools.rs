//! The three Ommatix tools: their descriptors (`tools/list`) and their execution against
//! the engine (`tools/call`).

use std::sync::{Arc, Mutex, PoisonError};

use ommatix_application::Engine;
use ommatix_application::contract::{
    ActionRequest, ActionResult, ActionTimings, ErrorCode, NextStep, ObservePage, ObserveRequest,
    ToolError, limits,
};
use rmcp::model::Tool;
use serde::Serialize;
use serde_json::Value;

use crate::dto::{
    self, EngineMetricsOutput, ExecuteActionOutput, InvalidArguments, ObserveScreenOutput,
    ToolErrorOutput,
};
use crate::schemas;

pub const OBSERVE_SCREEN: &str = "observe_screen";
pub const EXECUTE_ACTION: &str = "execute_action";
pub const GET_ENGINE_METRICS: &str = "get_engine_metrics";

/// `_meta` key of the per-stage timings of the observation behind a result.
pub const TIMINGS_META_KEY: &str = "io.ommatix/timings";

/// `_meta` key of what the planner decided for the observation behind a result: the work
/// kind, the regions, the inferences and the evidence counts (ADR-0024).
pub const PLAN_META_KEY: &str = "io.ommatix/plan";

/// `_meta` key of the per-stage timings of an `execute_action` result: gate, freshness
/// capture, delivery, focus confirmation and verification.
pub const ACTION_TIMINGS_META_KEY: &str = "io.ommatix/action_timings";

/// `_meta` key of an `observe_screen` result's accessibility completeness.
pub const ACCESSIBILITY_META_KEY: &str = "io.ommatix/accessibility";

/// The tool descriptors, sorted by name (the order of `tools/list`).
pub(crate) fn descriptors() -> Vec<Tool> {
    let tool = |name: &'static str, description: &'static str, input, output| {
        let mut tool = Tool::new(name, description, Arc::new(schemas::object(input)));
        tool.output_schema = Some(Arc::new(schemas::object(output)));
        tool
    };
    vec![
        tool(
            EXECUTE_ACTION,
            "Act on one element of the last observation by target ID and revision: click, \
             type or scroll. No coordinates. A dry run unless dry_run is false and the local \
             policy allows live input; the status never claims more than what happened.",
            schemas::EXECUTE_ACTION_INPUT,
            schemas::EXECUTE_ACTION_OUTPUT,
        ),
        tool(
            GET_ENGINE_METRICS,
            "Engine counters, model state, pending regions and the origin of its timings.",
            schemas::GET_ENGINE_METRICS_INPUT,
            schemas::GET_ENGINE_METRICS_OUTPUT,
        ),
        tool(
            OBSERVE_SCREEN,
            "Observe the screen as a full snapshot of GUI elements with stable session IDs, \
             roles, labels and actionability (no coordinates). Pages are explicit: when \
             truncated, pass next_cursor to read the same snapshot.",
            schemas::OBSERVE_SCREEN_INPUT,
            schemas::OBSERVE_SCREEN_OUTPUT,
        ),
    ]
}

/// Why `tools/call` could not run a tool: a protocol error, not a tool result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CallError {
    UnknownTool,
    InvalidArguments(InvalidArguments),
    /// The broker cannot serve the call: a call panicked while it held the broker's lock, so
    /// nothing is served and nothing is delivered from then on (fail closed, ADR-0039). The
    /// stdio server never returns it.
    Unavailable,
}

/// A tool call whose arguments passed the tool's input schema.
#[derive(Debug, Clone)]
pub(crate) enum ToolCall {
    Observe(ObserveRequest),
    Act(ActionRequest),
    Metrics,
}

/// Parses one tool call, without touching the engine.
pub(crate) fn parse(name: &str, arguments: &Value) -> Result<ToolCall, CallError> {
    match name {
        OBSERVE_SCREEN => dto::parse_observe_args(arguments)
            .map(ToolCall::Observe)
            .map_err(CallError::InvalidArguments),
        EXECUTE_ACTION => dto::parse_action_args(arguments)
            .map(ToolCall::Act)
            .map_err(CallError::InvalidArguments),
        GET_ENGINE_METRICS => dto::parse_metrics_args(arguments)
            .map(|()| ToolCall::Metrics)
            .map_err(CallError::InvalidArguments),
        _ => Err(CallError::UnknownTool),
    }
}

/// What a tool produced, before it is wrapped in a `CallToolResult`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ToolOutput {
    /// The tool that produced it.
    pub tool: &'static str,
    /// The Ommatix object (`structuredContent`); the text block serialises the same object.
    pub structured: Value,
    pub text: String,
    pub is_error: bool,
    /// Run-dependent values for the result `_meta`.
    pub meta: Vec<(&'static str, Value)>,
}

impl ToolOutput {
    pub(crate) fn new<T: Serialize>(tool: &'static str, object: &T, is_error: bool) -> Self {
        // Serialising plain structs of strings, numbers and booleans cannot fail.
        let text = serde_json::to_string(object).unwrap_or_default();
        let structured = serde_json::to_value(object).unwrap_or(Value::Null);
        Self {
            tool,
            structured,
            text,
            is_error,
            meta: Vec::new(),
        }
    }

    pub(crate) fn tool_error(tool: &'static str, error: &ToolError) -> Self {
        Self::new(tool, &ToolErrorOutput::from(error), true)
    }
}

/// Runs one tool. The engine is locked for the whole call: it is the single writer.
pub(crate) fn call(
    engine: &Mutex<Engine>,
    name: &str,
    arguments: &Value,
) -> Result<ToolOutput, CallError> {
    let call = parse(name, arguments)?;
    let mut engine = engine.lock().unwrap_or_else(PoisonError::into_inner);
    Ok(match call {
        ToolCall::Observe(request) => {
            let result = engine.observe_screen(&request);
            observation(&engine, result)
        }
        ToolCall::Act(request) => {
            let result = engine.execute_action(&request);
            action(&result, engine.last_action_timings())
        }
        ToolCall::Metrics => metrics(&engine),
    })
}

/// The `observe_screen` result for `result`, with the `_meta` of the observation the
/// published snapshot came from when it is a page.
pub(crate) fn observation(engine: &Engine, result: Result<ObservePage, ToolError>) -> ToolOutput {
    let output = match result {
        Ok(page) => ToolOutput::new(OBSERVE_SCREEN, &ObserveScreenOutput::from(&page), false),
        Err(error) => ToolOutput::tool_error(OBSERVE_SCREEN, &error),
    };
    match (
        engine.last_timings(),
        engine.last_plan(),
        engine.accessibility_complete(),
    ) {
        (Some(t), Some(plan), Some(complete)) if !output.is_error => ToolOutput {
            meta: vec![
                (TIMINGS_META_KEY, dto::timings_meta(&t)),
                (PLAN_META_KEY, dto::plan_meta(&plan)),
                (ACCESSIBILITY_META_KEY, dto::accessibility_meta(complete)),
            ],
            ..output
        },
        _ => output,
    }
}

/// The `execute_action` result for `result`, with the stage timings of the gate that ran it,
/// if one did.
pub(crate) fn action(result: &ActionResult, timings: Option<ActionTimings>) -> ToolOutput {
    // A rejection, or an indeterminate effect with its error, is an expected domain error:
    // the agent must see it as one.
    let is_error =
        result.error.is_some() || result.status == ommatix_domain::identity::ActionStatus::Rejected;
    let output = ToolOutput::new(EXECUTE_ACTION, &ExecuteActionOutput::from(result), is_error);
    match timings {
        Some(t) => ToolOutput {
            meta: vec![(ACTION_TIMINGS_META_KEY, dto::action_timings_meta(&t))],
            ..output
        },
        None => output,
    }
}

/// The `get_engine_metrics` result.
pub(crate) fn metrics(engine: &Engine) -> ToolOutput {
    ToolOutput::new(
        GET_ENGINE_METRICS,
        &EngineMetricsOutput::from(&engine.engine_metrics()),
        false,
    )
}

/// The replacement for an `observe_screen` result that would not fit in one message.
pub(crate) fn limit_exceeded(max_message_bytes: usize) -> ToolOutput {
    let mut message = format!(
        "the result exceeds the {max_message_bytes}-byte message limit; request fewer \
         elements (lower limit), a narrower query or no values"
    );
    message.truncate(limits::MAX_MESSAGE_CHARS);
    ToolOutput::tool_error(
        OBSERVE_SCREEN,
        &ToolError {
            code: ErrorCode::LimitExceeded,
            message,
            retryable: false,
            next_step: NextStep::ReduceRequest,
        },
    )
}
