//! The dossier's contract examples (`contratos/validacion_casos.json`): every valid example
//! is accepted by the DTO layer and every deliberately invalid one is rejected, exactly as
//! the JSON schemas decide.

mod common;

use common::{read_json, violations};
use ommatix_adapter_mcp::dto::{
    self, EngineMetricsOutput, ExecuteActionOutput, ObserveScreenOutput, ToolErrorOutput,
};
use ommatix_application::contract::{ActionResult, EngineMetrics, ObservePage, ToolError};
use serde_json::Value;

/// Parses an example with the DTO for `schema` and, for outputs, produces it back from the
/// contract type. `Ok` carries the re-produced payload (outputs only).
fn through_dto(schema: &str, example: &Value) -> Result<Option<Value>, String> {
    fn produce<T: serde::Serialize>(dto: &T) -> Value {
        serde_json::to_value(dto).expect("DTOs serialise")
    }
    match schema {
        "observe_screen.input" => dto::parse_observe_args(example)
            .map(|_| None)
            .map_err(|e| e.0),
        "execute_action.input" => dto::parse_action_args(example)
            .map(|_| None)
            .map_err(|e| e.0),
        "get_engine_metrics.input" => dto::parse_metrics_args(example)
            .map(|()| None)
            .map_err(|e| e.0),
        "observe_screen.output" => dto::parse_output::<ObserveScreenOutput>(example)
            .and_then(ObservePage::try_from)
            .map(|p| Some(produce(&ObserveScreenOutput::from(&p))))
            .map_err(|e| e.0),
        "execute_action.output" => dto::parse_output::<ExecuteActionOutput>(example)
            .and_then(ActionResult::try_from)
            .map(|r| Some(produce(&ExecuteActionOutput::from(&r))))
            .map_err(|e| e.0),
        "get_engine_metrics.output" => dto::parse_output::<EngineMetricsOutput>(example)
            .and_then(EngineMetrics::try_from)
            .map(|m| Some(produce(&EngineMetricsOutput::from(&m))))
            .map_err(|e| e.0),
        "tool_error.output" => dto::parse_output::<ToolErrorOutput>(example)
            .and_then(ToolError::try_from)
            .map(|e| Some(produce(&ToolErrorOutput::from(&e))))
            .map_err(|e| e.0),
        other => panic!("no DTO for schema {other}"),
    }
}

fn schema_name(path: &str) -> &str {
    path.trim_start_matches("contratos/esquemas/")
        .trim_end_matches(".schema.json")
}

#[test]
fn validation_cases_are_accepted_or_rejected_by_the_dto_layer_as_declared() {
    let cases = read_json("contratos/validacion_casos.json");
    let cases = cases.as_array().expect("a list of cases");
    assert!(cases.len() >= 17);
    for case in cases {
        let example_path = case["example"].as_str().expect("example path");
        let schema = schema_name(case["schema"].as_str().expect("schema path"));
        let valid = case["valid"].as_bool().expect("valid flag");
        let example = read_json(example_path);

        // The reference validator agrees with the declared verdict.
        assert_eq!(
            violations(schema, &example).is_empty(),
            valid,
            "jsonschema verdict for {example_path}"
        );

        match through_dto(schema, &example) {
            Ok(produced) => {
                assert!(valid, "{example_path} must be rejected by the DTO layer");
                if let Some(produced) = produced {
                    assert_eq!(produced, example, "{example_path} round-trips unchanged");
                    assert_eq!(violations(schema, &produced).len(), 0);
                }
            }
            Err(reason) => assert!(
                !valid,
                "{example_path} must be accepted by the DTO layer, got: {reason}"
            ),
        }
    }
}

#[test]
fn invalid_input_examples_are_rejected_without_echoing_their_values() {
    for (file, secret) in [
        ("click_con_texto.json", "No permitido"),
        ("accion_shell.json", "shell"),
    ] {
        let example = read_json(&format!("contratos/ejemplos_invalidos/{file}"));
        let error = dto::parse_action_args(&example).unwrap_err();
        assert!(!error.0.contains(secret), "{file}: {error}");
    }
}
