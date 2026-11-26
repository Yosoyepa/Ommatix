//! CT-14 (ADR-0044): what an `execute_action` result says after `type`, on the wire.
//!
//! Every result the engine can produce for `type` validates against the dossier's output
//! schema and survives the DTO round trip, the new one included: `input_delivered` with
//! `verified: false` and `observed_effect: visual_change`. A `verified: true` without
//! `effect_observed` is refused by both. And an agent is told, where it reads the contract
//! (the tool description and the output schema `tools/list` serves), that after `type` the
//! engine does not know how many characters arrived.

mod common;

use common::{assert_valid, modern, server, violations};
use ommatix_adapter_mcp::EXECUTE_ACTION;
use ommatix_adapter_mcp::dto::{self, ExecuteActionOutput};
use ommatix_application::contract::{ActionResult, ErrorCode, ObservedEffect, ToolError};
use ommatix_domain::identity::ActionStatus;
use serde_json::{Value, json};

/// The sentence an agent must find wherever it learns what a `type` result means.
const NOT_THE_TEXT: &str = "not how many characters arrived or which";

fn result(
    status: ActionStatus,
    delivered: bool,
    error: Option<ErrorCode>,
    observed_effect: Option<ObservedEffect>,
) -> ActionResult {
    ActionResult {
        request_id: "t1".into(),
        status,
        target_id: "e3".into(),
        snapshot_id: "s2".into(),
        error: error.map(|code| ToolError::new(code, "a sentence of the engine")),
        observed_effect,
        delivered,
    }
}

/// Every outcome `act.rs` can give a `type` request, by the path that gives it.
fn type_results() -> Vec<(&'static str, ActionResult)> {
    use ActionStatus::{Indeterminate, InputDelivered, Rejected, Simulated};
    use ErrorCode::{
        EpochMismatch, IndeterminateEffect, LimitExceeded, NotActionable, PolicyDenied,
        RequestConflict, SessionMismatch, StaleSnapshot, StaleTarget, TargetNotFound,
    };
    let unknown = Some(ObservedEffect::Unknown);
    let mut all = vec![
        ("dry run", result(Simulated, false, None, None)),
        (
            "expected_effect none",
            result(InputDelivered, true, None, None),
        ),
        (
            "visual_change seen after the keys (ADR-0044)",
            result(
                InputDelivered,
                true,
                None,
                Some(ObservedEffect::VisualChange),
            ),
        ),
        (
            "no change, no capture, value_updated, focus not confirmed or keys refused",
            result(Indeterminate, true, Some(IndeterminateEffect), unknown),
        ),
        (
            "the actuator failed or the click was not accepted",
            result(Indeterminate, false, Some(IndeterminateEffect), unknown),
        ),
    ];
    for code in [
        StaleSnapshot,
        StaleTarget,
        TargetNotFound,
        NotActionable,
        PolicyDenied,
        LimitExceeded,
        RequestConflict,
        SessionMismatch,
        EpochMismatch,
    ] {
        all.push((
            "refused at the gate",
            result(Rejected, false, Some(code), None),
        ));
    }
    all
}

#[test]
fn ct_14_every_type_result_validates_against_the_schema_and_round_trips() {
    for (case, typed) in type_results() {
        let wire = serde_json::to_value(ExecuteActionOutput::from(&typed)).expect("serialises");
        assert_valid("execute_action.output", &wire);
        let back = dto::parse_output::<ExecuteActionOutput>(&wire)
            .and_then(ActionResult::try_from)
            .unwrap_or_else(|e| panic!("{case}: {}", e.0));
        assert_eq!(back, typed, "{case}");
        assert_ne!(
            wire["status"], "effect_observed",
            "{case}: never after type"
        );
        assert_eq!(wire["verified"], false, "{case}: never verified after type");
    }
}

/// The new answer, exactly as it goes on the wire.
#[test]
fn ct_14_a_type_with_a_visual_change_is_delivered_and_not_verified_on_the_wire() {
    let typed = result(
        ActionStatus::InputDelivered,
        true,
        None,
        Some(ObservedEffect::VisualChange),
    );
    let wire = serde_json::to_value(ExecuteActionOutput::from(&typed)).expect("serialises");
    assert_eq!(
        wire,
        json!({
            "schema_version": "1.0",
            "request_id": "t1",
            "status": "input_delivered",
            "target_id": "e3",
            "snapshot_id": "s2",
            "input_delivered": true,
            "verified": false,
            "observed_effect": "visual_change"
        })
    );
}

/// `verified: true` belongs to `effect_observed` alone: the schema and the DTO refuse it
/// with `input_delivered`, so the claim cannot come back through the wire either.
#[test]
fn ct_14_verified_true_without_effect_observed_is_refused() {
    let claim = json!({
        "schema_version": "1.0",
        "request_id": "t1",
        "status": "input_delivered",
        "target_id": "e3",
        "snapshot_id": "s2",
        "input_delivered": true,
        "verified": true,
        "observed_effect": "visual_change"
    });
    assert!(
        !violations("execute_action.output", &claim).is_empty(),
        "the schema refuses it"
    );
    assert!(
        dto::parse_output::<ExecuteActionOutput>(&claim)
            .and_then(ActionResult::try_from)
            .is_err(),
        "the DTO refuses it"
    );
}

fn served_execute_action() -> Value {
    let mut conn = server().connect();
    let list = modern(&mut conn, 1, "tools/list", json!({}));
    list["result"]["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .find(|t| t["name"] == EXECUTE_ACTION)
        .expect("execute_action is listed")
        .clone()
}

/// The words an agent reads: the tool description and the descriptions of the output
/// schema's fields. Removing the sentence from either fails here.
#[test]
fn ct_14_the_tool_description_and_the_schema_say_what_type_does_not_verify() {
    let tool = served_execute_action();
    let description = tool["description"].as_str().expect("a description");
    assert!(description.contains(NOT_THE_TEXT), "{description}");
    assert!(description.contains("observe again"), "{description}");

    let fields = &tool["outputSchema"]["properties"];
    let verified = fields["verified"]["description"]
        .as_str()
        .expect("verified is described");
    assert!(verified.contains(NOT_THE_TEXT), "{verified}");
    assert!(verified.contains("observe again"), "{verified}");
    let status = fields["status"]["description"]
        .as_str()
        .expect("status is described");
    assert!(status.contains("never after type"), "{status}");
    let effect = fields["observed_effect"]["description"]
        .as_str()
        .expect("observed_effect is described");
    assert!(effect.contains("after the keys"), "{effect}");
}
