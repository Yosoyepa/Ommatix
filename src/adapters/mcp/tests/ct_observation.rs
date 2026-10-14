//! Planned contract tests CT-01..CT-04 (plan_pruebas.csv): the observation payload as the
//! agent receives it over MCP, driven through the real engine with deterministic port
//! doubles.

mod common;

use common::{
    SESSION_ID, basic_scene, call, call_ok, check_tool_result, engine_with, error_code, server,
    server_with, word_grid,
};
use ommatix_adapter_mcp::{
    ACCESSIBILITY_META_KEY, ACTION_TIMINGS_META_KEY, McpConfig, McpServer, TIMINGS_META_KEY,
};
use ommatix_application::LocalPolicy;
use ommatix_application::contract::limits;
use ommatix_domain::fusion::{Actionability, Role, Word};
use serde_json::{Value, json};

fn target_number(element: &Value) -> u64 {
    element["target_id"].as_str().expect("target_id")[1..]
        .parse()
        .expect("e<n>")
}

fn observe(conn: &mut ommatix_adapter_mcp::Connection, id: u64, args: Value) -> Value {
    let result = call_ok(conn, id, "observe_screen", args);
    assert_eq!(
        result["isError"], false,
        "observe_screen should succeed: {result}"
    );
    result
}

#[test]
fn ct_01_full_snapshot_matches_the_schema_with_a_stable_order() {
    let mut conn = server().connect();
    let first = observe(&mut conn, 1, json!({}));
    let page = &first["structuredContent"];
    assert_eq!(page["mode"], "full");
    assert_eq!(page["session_id"], SESSION_ID);
    assert_eq!(page["truncated"], false);
    assert!(page.get("next_cursor").is_none());
    let elements = page["elements"].as_array().expect("elements");
    assert_eq!(
        elements.len(),
        4,
        "button, input, unknown control, free text"
    );
    assert_eq!(page["total_elements"], 4);
    let numbers: Vec<u64> = elements.iter().map(target_number).collect();
    assert!(
        numbers.windows(2).all(|w| w[0] < w[1]),
        "sorted by target id: {numbers:?}"
    );
    // Timings are run-dependent: `_meta` only.
    assert!(first["_meta"][TIMINGS_META_KEY]["total_us"].is_u64());
    // Whether the accessibility evidence is complete is run-dependent too: `_meta` only.
    assert_eq!(first["_meta"][ACCESSIBILITY_META_KEY]["complete"], true);
    assert!(page.get("complete").is_none());

    // Same screen again: a new snapshot with the same elements in the same order.
    let second = observe(&mut conn, 2, json!({}));
    assert_ne!(
        second["structuredContent"]["snapshot_id"],
        page["snapshot_id"]
    );
    assert_eq!(second["structuredContent"]["elements"], page["elements"]);

    // Same frames, same JSON: a second engine produces the same payload.
    let mut other = server().connect();
    let replay = observe(&mut other, 1, json!({}));
    assert_eq!(replay["structuredContent"], first["structuredContent"]);
    assert_eq!(replay["content"], first["content"]);
}

#[test]
fn ct_02_pages_stay_in_one_snapshot_and_truncation_is_explicit() {
    let mut conn = server().connect();
    let first = observe(&mut conn, 1, json!({ "limit": 1 }));
    let page = first["structuredContent"].clone();
    let snapshot = page["snapshot_id"].clone();
    let total = page["total_elements"].as_u64().expect("total");
    assert!(total >= 3);
    assert_eq!(page["truncated"], true);
    assert_eq!(page["elements"].as_array().map(Vec::len), Some(1));
    let first_cursor = page["next_cursor"].as_str().expect("cursor").to_owned();

    let mut seen: Vec<u64> = page["elements"]
        .as_array()
        .unwrap()
        .iter()
        .map(target_number)
        .collect();
    let mut cursor = Some(first_cursor.clone());
    let mut id = 2;
    while let Some(c) = cursor {
        let next = observe(&mut conn, id, json!({ "limit": 1, "cursor": c }));
        id += 1;
        let p = &next["structuredContent"];
        assert_eq!(p["snapshot_id"], snapshot, "pages never leave the snapshot");
        assert_eq!(p["total_elements"], total);
        seen.extend(p["elements"].as_array().unwrap().iter().map(target_number));
        cursor = p["next_cursor"].as_str().map(str::to_owned);
        assert_eq!(p["truncated"], cursor.is_some());
    }
    assert_eq!(seen.len() as u64, total);
    assert!(seen.windows(2).all(|w| w[0] < w[1]), "{seen:?}");

    // A new observation publishes a new snapshot: the old cursor is stale.
    observe(&mut conn, 100, json!({}));
    for (i, stale) in [first_cursor.as_str(), "not-a-cursor"]
        .into_iter()
        .enumerate()
    {
        let result = call_ok(
            &mut conn,
            101 + i as u64,
            "observe_screen",
            json!({ "cursor": stale }),
        );
        assert_eq!(result["isError"], true);
        assert_eq!(
            result["structuredContent"]["error"]["code"],
            "STALE_SNAPSHOT"
        );
        assert_eq!(
            result["structuredContent"]["error"]["next_step"],
            "observe_again"
        );
    }
}

#[test]
fn ct_03_unknown_state_is_reported_not_invented() {
    let mut conn = server().connect();
    let result = observe(&mut conn, 1, json!({ "include_values": true }));
    let elements = result["structuredContent"]["elements"]
        .as_array()
        .unwrap()
        .clone();
    let by_role = |role: &str| -> Vec<Value> {
        elements
            .iter()
            .filter(|e| e["role"] == role)
            .cloned()
            .collect()
    };

    let unknown = by_role("unknown");
    assert_eq!(unknown.len(), 1, "{elements:?}");
    assert_eq!(unknown[0]["label"], "");
    assert_eq!(unknown[0]["label_source"], "unknown");
    assert_eq!(unknown[0]["actionability"], "unknown");

    let button = &by_role("button")[0];
    assert_eq!(button["label"], "Save");
    assert_eq!(button["label_source"], "ocr");
    assert_eq!(button["actionability"], "confirmed");

    let text = &by_role("text_node")[0];
    assert_eq!(text["label"], "Welcome");
    assert_eq!(
        text["actionability"], "unknown",
        "an ID is not an affordance"
    );
    assert_eq!(text["enabled"], Value::Null);

    for e in &elements {
        assert_eq!(e["zone"], "unknown", "no zone evidence yet");
        // The default local policy redacts values even when the agent asks for them.
        assert!(
            e.get("text").is_none() && e.get("text_kind").is_none(),
            "{e}"
        );
    }
}

#[test]
fn ct_04_input_limits_are_enforced_before_the_engine() {
    let mut conn = server().connect();
    let long = |n: usize| "q".repeat(n);
    let click = |extra: Value| {
        let mut args = json!({
            "request_id": "r1", "session_id": SESSION_ID, "snapshot_id": "s1",
            "geometry_epoch": 1, "target_id": "e1", "expected_revision": 1, "action": "type",
            "text": "ok"
        });
        for (k, v) in extra.as_object().unwrap() {
            args[k] = v.clone();
        }
        args
    };
    let rejected = [
        (
            "observe_screen",
            json!({ "query": long(limits::MAX_QUERY_CHARS + 1) }),
        ),
        ("observe_screen", json!({ "limit": 0 })),
        ("observe_screen", json!({ "limit": 201 })),
        ("observe_screen", json!({ "cursor": "" })),
        (
            "observe_screen",
            json!({ "cursor": long(limits::MAX_CURSOR_CHARS + 1) }),
        ),
        (
            "execute_action",
            click(json!({ "text": long(limits::MAX_TYPE_TEXT_CHARS + 1) })),
        ),
        ("execute_action", click(json!({ "text": "" }))),
        ("execute_action", click(json!({ "request_id": long(65) }))),
        (
            "execute_action",
            click(json!({ "action": "scroll", "direction": "down", "steps": 6 }))
                .as_object()
                .map(|o| {
                    let mut o = o.clone();
                    o.remove("text");
                    Value::Object(o)
                })
                .unwrap(),
        ),
    ];
    for (i, (tool, args)) in rejected.into_iter().enumerate() {
        let response = call(&mut conn, i as u64, tool, args);
        assert_eq!(error_code(&response), -32602, "{tool}: {response}");
    }
    // At the bounds the arguments are accepted (whatever the engine then answers).
    let accepted = [
        (
            "observe_screen",
            json!({ "query": long(limits::MAX_QUERY_CHARS), "limit": 200 }),
        ),
        (
            "execute_action",
            click(json!({ "text": long(limits::MAX_TYPE_TEXT_CHARS) })),
        ),
    ];
    for (i, (tool, args)) in accepted.into_iter().enumerate() {
        let response = call(&mut conn, 100 + i as u64, tool, args);
        let result = response
            .get("result")
            .unwrap_or_else(|| panic!("{tool} at the bound: {response}"));
        check_tool_result(result, tool);
    }
}

#[test]
fn ct_04_labels_and_values_are_capped_in_the_output() {
    let mut scene = basic_scene();
    // A label far longer than the contract allows and an input value longer than 512.
    scene.words[0] = Word::new("S".repeat(300), scene.words[0].bbox);
    scene.controls.push(common::control(
        common::bx(10.0, 130.0, 240.0, 180.0),
        Role::Input,
        Actionability::Confirmed,
    ));
    scene.words.push(Word::new(
        "v".repeat(600),
        common::bx(15.0, 135.0, 235.0, 175.0),
    ));
    let policy = LocalPolicy {
        include_values: true,
        ..LocalPolicy::default()
    };
    let server = McpServer::new(engine_with(scene, policy), McpConfig::default());
    let mut conn = server.connect();
    let result = observe(&mut conn, 1, json!({ "include_values": true }));
    let elements = result["structuredContent"]["elements"].as_array().unwrap();
    for e in elements {
        assert!(e["label"].as_str().unwrap().chars().count() <= limits::MAX_LABEL_CHARS);
        if let Some(text) = e.get("text") {
            assert!(text.as_str().unwrap().chars().count() <= limits::MAX_TEXT_CHARS);
            assert!(e.get("text_kind").is_some());
        }
    }
    assert!(
        elements
            .iter()
            .any(|e| e["label"].as_str().is_some_and(|l| l.starts_with("SSS"))),
        "the long label is truncated, not dropped"
    );
    assert!(
        elements.iter().any(|e| e.get("text").is_some()),
        "values are served when both the agent and the policy allow them"
    );
}

#[test]
fn ct_04_query_filters_the_view_without_forgetting_elements() {
    let mut conn = server_with(word_grid(30, 8), McpConfig::default()).connect();
    let all = observe(&mut conn, 1, json!({}));
    let total = all["structuredContent"]["total_elements"].as_u64().unwrap();
    assert_eq!(total, 30);
    let filtered = observe(&mut conn, 2, json!({ "query": "BBBB" }));
    let f = &filtered["structuredContent"];
    assert!(f["total_elements"].as_u64().unwrap() < total);
    assert!(
        f["elements"]
            .as_array()
            .unwrap()
            .iter()
            .all(|e| e["label"].as_str().unwrap().contains('b'))
    );
    let again = observe(&mut conn, 3, json!({}));
    assert_eq!(again["structuredContent"]["total_elements"], total);
}

/// S4 third round, finding 6: an action's stage timings come back in `_meta`, never in the
/// Ommatix object.
#[test]
fn execute_action_results_carry_their_stage_timings_in_meta() {
    let mut conn = server().connect();
    let page = observe(&mut conn, 1, json!({}));
    let page = &page["structuredContent"];
    let button = page["elements"]
        .as_array()
        .expect("elements")
        .iter()
        .find(|e| e["role"] == "button")
        .expect("a button");
    let click = |request_id: &str, snapshot_id: &str| {
        json!({
            "request_id": request_id, "session_id": SESSION_ID, "snapshot_id": snapshot_id,
            "geometry_epoch": page["geometry_epoch"], "target_id": button["target_id"],
            "expected_revision": button["revision"], "action": "click"
        })
    };
    let stages = [
        "gate",
        "freshness",
        "delivery",
        "focus",
        "verification",
        "total",
    ];

    // A dry run and a rejection both went through the gate.
    for (id, (request_id, snapshot_id)) in [
        ("r1", page["snapshot_id"].as_str().unwrap()),
        ("r2", "s999"),
    ]
    .into_iter()
    .enumerate()
    {
        let result = call_ok(
            &mut conn,
            10 + id as u64,
            "execute_action",
            click(request_id, snapshot_id),
        );
        check_tool_result(&result, "execute_action");
        let meta = &result["_meta"][ACTION_TIMINGS_META_KEY];
        for stage in stages {
            assert!(meta[format!("{stage}_us")].is_u64(), "{stage}: {result}");
        }
        assert!(result["structuredContent"].get("timings").is_none());
    }

    // A replay of an earlier request has no timings of its own.
    let replay = call_ok(
        &mut conn,
        20,
        "execute_action",
        click("r1", page["snapshot_id"].as_str().unwrap()),
    );
    assert!(
        replay["_meta"].get(ACTION_TIMINGS_META_KEY).is_none(),
        "{replay}"
    );
}
