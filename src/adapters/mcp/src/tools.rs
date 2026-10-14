//! The three Ommatix tools: their descriptors (`tools/list`) and their execution against
//! the engine (`tools/call`).

use std::sync::{Arc, Mutex, PoisonError};

use ommatix_application::Engine;
use ommatix_application::contract::{ErrorCode, NextStep, ToolError, limits};
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
    fn new<T: Serialize>(tool: &'static str, object: &T, is_error: bool) -> Self {
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

    fn tool_error(tool: &'static str, error: &ToolError) -> Self {
        Self::new(tool, &ToolErrorOutput::from(error), true)
    }
}

/// Runs one tool. The engine is locked for the whole call: it is the single writer.
pub(crate) fn call(
    engine: &Mutex<Engine>,
    name: &str,
    arguments: &Value,
) -> Result<ToolOutput, CallError> {
    match name {
        OBSERVE_SCREEN => {
            let request =
                dto::parse_observe_args(arguments).map_err(CallError::InvalidArguments)?;
            let mut engine = engine.lock().unwrap_or_else(PoisonError::into_inner);
            let output = match engine.observe_screen(&request) {
                Ok(page) => {
                    ToolOutput::new(OBSERVE_SCREEN, &ObserveScreenOutput::from(&page), false)
                }
                Err(error) => ToolOutput::tool_error(OBSERVE_SCREEN, &error),
            };
            let timings = engine.last_timings();
            drop(engine);
            Ok(match timings {
                Some(t) if !output.is_error => ToolOutput {
                    meta: vec![(TIMINGS_META_KEY, dto::timings_meta(&t))],
                    ..output
                },
                _ => output,
            })
        }
        EXECUTE_ACTION => {
            let request = dto::parse_action_args(arguments).map_err(CallError::InvalidArguments)?;
            let result = engine
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .execute_action(&request);
            // A rejection, or an indeterminate effect with its error, is an expected
            // domain error: the agent must see it as one.
            let is_error = result.error.is_some()
                || result.status == ommatix_domain::identity::ActionStatus::Rejected;
            Ok(ToolOutput::new(
                EXECUTE_ACTION,
                &ExecuteActionOutput::from(&result),
                is_error,
            ))
        }
        GET_ENGINE_METRICS => {
            dto::parse_metrics_args(arguments).map_err(CallError::InvalidArguments)?;
            let metrics = engine
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .engine_metrics();
            Ok(ToolOutput::new(
                GET_ENGINE_METRICS,
                &EngineMetricsOutput::from(&metrics),
                false,
            ))
        }
        _ => Err(CallError::UnknownTool),
    }
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
