//! The JSON schemas of the Ommatix 1.0 contract, embedded from the thesis dossier
//! (`docs/thesis/contratos/esquemas/`) so the served schemas are the specification's own
//! bytes.

use serde_json::{Map, Value};

pub const OBSERVE_SCREEN_INPUT: &str =
    include_str!("../../../../docs/thesis/contratos/esquemas/observe_screen.input.schema.json");
pub const OBSERVE_SCREEN_OUTPUT: &str =
    include_str!("../../../../docs/thesis/contratos/esquemas/observe_screen.output.schema.json");
pub const EXECUTE_ACTION_INPUT: &str =
    include_str!("../../../../docs/thesis/contratos/esquemas/execute_action.input.schema.json");
pub const EXECUTE_ACTION_OUTPUT: &str =
    include_str!("../../../../docs/thesis/contratos/esquemas/execute_action.output.schema.json");
pub const GET_ENGINE_METRICS_INPUT: &str =
    include_str!("../../../../docs/thesis/contratos/esquemas/get_engine_metrics.input.schema.json");
pub const GET_ENGINE_METRICS_OUTPUT: &str = include_str!(
    "../../../../docs/thesis/contratos/esquemas/get_engine_metrics.output.schema.json"
);
pub const TOOL_ERROR_OUTPUT: &str =
    include_str!("../../../../docs/thesis/contratos/esquemas/tool_error.output.schema.json");

/// Parses one of the embedded schemas.
///
/// The inputs are compile-time constants checked by this module's tests, so a failure is
/// a build defect, not external input.
pub(crate) fn object(schema: &'static str) -> Map<String, Value> {
    match serde_json::from_str(schema) {
        Ok(Value::Object(map)) => map,
        _ => unreachable!("embedded contract schemas are JSON objects (tested)"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_embedded_schema_is_a_draft_2020_12_object() {
        for s in [
            OBSERVE_SCREEN_INPUT,
            OBSERVE_SCREEN_OUTPUT,
            EXECUTE_ACTION_INPUT,
            EXECUTE_ACTION_OUTPUT,
            GET_ENGINE_METRICS_INPUT,
            GET_ENGINE_METRICS_OUTPUT,
            TOOL_ERROR_OUTPUT,
        ] {
            let map = object(s);
            assert_eq!(
                map["$schema"],
                "https://json-schema.org/draft/2020-12/schema"
            );
            assert_eq!(map["type"], "object");
        }
    }
}
