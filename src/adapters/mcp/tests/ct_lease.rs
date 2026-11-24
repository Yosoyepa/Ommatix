//! Planned contract test CT-11 (plan_pruebas.csv, RF-19; ADR-0039, proposed): the optional
//! `lease` argument of `observe_screen`. It takes `acquire` or `release` and nothing else, it
//! goes with every other argument (a cursor included), and `tools/list` advertises it exactly
//! as the dossier's schema file and its example dialog do. Its effect on a broker is CT-12; on
//! the stdio server, CT-13 (both in `broker.rs`).

mod common;

use common::{call, error_code, read_json, schema, server};
use ommatix_adapter_mcp::dto::{self, LeaseArgument};
use serde_json::{Value, json};

const INVALID_PARAMS: i64 = -32602;

/// A string only the agent can supply: no refusal may echo it.
const AGENT_TEXT: &str = "Zephyr-steal-the-lease";

/// Values of `lease` the input schema refuses, each of a different kind.
fn invalid_leases() -> Vec<Value> {
    vec![
        json!(AGENT_TEXT),
        json!(""),
        json!("ACQUIRE"),
        json!(" acquire"),
        Value::Null,
        json!(true),
        json!(1),
        json!(1.5),
        json!(["acquire"]),
        json!({ "acquire": true }),
    ]
}

#[test]
fn ct_11_lease_takes_acquire_or_release_and_nothing_else() {
    for (value, expected) in [
        ("acquire", LeaseArgument::Acquire),
        ("release", LeaseArgument::Release),
    ] {
        let (request, lease) = dto::parse_observe_call(&json!({ "lease": value })).unwrap();
        assert_eq!(lease, Some(expected));
        assert_eq!(
            request,
            dto::parse_observe_args(&json!({})).unwrap(),
            "the other arguments keep their defaults"
        );
    }
    let (_, absent) = dto::parse_observe_call(&json!({})).unwrap();
    assert_eq!(absent, None, "the argument is optional");

    let validator = jsonschema::validator_for(&schema("observe_screen.input")).unwrap();
    for value in invalid_leases() {
        let arguments = json!({ "lease": value });
        assert!(
            !validator.is_valid(&arguments),
            "the schema refuses {value}"
        );
        let error = dto::parse_observe_call(&arguments).expect_err("the DTO refuses it too");
        assert!(!error.0.contains(AGENT_TEXT), "{error}");
        assert!(!error.0.contains("ACQUIRE"), "{error}");
    }
}

#[test]
fn ct_11_an_invalid_lease_is_invalid_params_on_the_wire_and_never_echoed() {
    let mut conn = server().connect();
    for (id, value) in (1..).zip(invalid_leases()) {
        let response = call(
            &mut conn,
            id,
            "observe_screen",
            json!({ "lease": value.clone() }),
        );
        assert_eq!(error_code(&response), INVALID_PARAMS, "{value}: {response}");
        let text = response.to_string();
        assert!(!text.contains(AGENT_TEXT), "{text}");
        assert!(!text.contains("ACQUIRE"), "{text}");
    }
}

#[test]
fn ct_11_lease_goes_with_every_other_argument_a_cursor_included() {
    let arguments = json!({
        "query": "Save",
        "limit": 2,
        "cursor": "c1",
        "include_values": true,
        "lease": "release"
    });
    let validator = jsonschema::validator_for(&schema("observe_screen.input")).unwrap();
    assert!(validator.is_valid(&arguments));
    let (request, lease) = dto::parse_observe_call(&arguments).unwrap();
    assert_eq!(lease, Some(LeaseArgument::Release));
    assert_eq!(request.query.as_deref(), Some("Save"));
    assert_eq!(request.limit, 2);
    assert_eq!(request.cursor.as_deref(), Some("c1"));
    assert!(request.include_values);
}

#[test]
fn ct_11_tools_list_advertises_lease_as_the_schema_file_and_the_dossier_example_do() {
    let mut conn = server().connect();
    let list = common::modern(&mut conn, 1, "tools/list", json!({}));
    let observe = list["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "observe_screen")
        .unwrap()
        .clone();
    let file = schema("observe_screen.input");
    assert_eq!(
        observe["inputSchema"], file,
        "the advertised schema is the file"
    );
    assert_eq!(
        file["properties"]["lease"],
        json!({ "type": "string", "enum": ["acquire", "release"] })
    );
    assert_eq!(file["additionalProperties"], false);
    assert_eq!(file["required"], json!([]), "lease is optional");

    let dialog = std::fs::read_to_string(common::dossier(
        "contratos/ejemplos/mcp_moderno_respuestas.jsonl",
    ))
    .unwrap();
    let listed: Value = serde_json::from_str(dialog.lines().nth(1).unwrap()).unwrap();
    let example = listed["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "observe_screen")
        .unwrap();
    assert_eq!(
        example["inputSchema"], file,
        "the example's inline copy is the file"
    );

    // The example input of the validation cases is accepted by both.
    let example = read_json("contratos/ejemplos/observe_lease_input.json");
    assert!(jsonschema::validator_for(&file).unwrap().is_valid(&example));
    assert!(dto::parse_observe_call(&example).unwrap().1.is_some());
}
