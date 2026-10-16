//! Planned integration test IT-06 (plan_pruebas.csv, RF-17): the per-stage time breakdown
//! and the redaction of telemetry, as the agent receives them over MCP, driven through the
//! real engine with deterministic port doubles. Run-dependent values live in `_meta` and
//! nowhere else; no screen text, value or typed text is ever in them.

mod common;

use std::time::Duration;

use common::{
    Costs, SESSION_ID, Scene, basic_scene, bx, call, call_ok, control, engine_with_costs,
    error_code, send_raw, server_with,
};
use ommatix_adapter_mcp::{
    ACCESSIBILITY_META_KEY, ACTION_TIMINGS_META_KEY, McpConfig, McpServer, PLAN_META_KEY,
    TIMINGS_META_KEY,
};
use ommatix_application::LocalPolicy;
use ommatix_domain::fusion::{Actionability, Role, Word};
use serde_json::{Value, json};

const SERVER_INFO_KEY: &str = "io.modelcontextprotocol/serverInfo";

/// Strings that only the screen or the agent can supply: none may appear in any `_meta`.
const VALUE: &str = "VALUE-4711";
const CAPTION: &str = "CAPTION-0042";
const BUTTON: &str = "PAY-9931";
const TYPED: &str = "TYPED-SECRET-2718";

/// A form with a button, an input holding a value and a free caption, each with a
/// distinctive string.
fn private_scene() -> Scene {
    Scene {
        width: 320,
        height: 200,
        controls: vec![
            control(
                bx(10.0, 10.0, 120.0, 40.0),
                Role::Button,
                Actionability::Confirmed,
            ),
            control(
                bx(10.0, 70.0, 250.0, 100.0),
                Role::Input,
                Actionability::Confirmed,
            ),
        ],
        words: vec![
            Word::new(BUTTON, bx(20.0, 15.0, 110.0, 35.0)),
            Word::new("Name", bx(10.0, 50.0, 60.0, 65.0)),
            Word::new(VALUE, bx(20.0, 75.0, 120.0, 95.0)),
            Word::new(CAPTION, bx(150.0, 120.0, 300.0, 140.0)),
        ],
    }
}

fn open_policy() -> LocalPolicy {
    LocalPolicy {
        include_values: true,
        ..LocalPolicy::default()
    }
}

fn element(page: &Value, label: &str) -> Value {
    page["elements"]
        .as_array()
        .expect("elements")
        .iter()
        .find(|e| e["label"] == label)
        .unwrap_or_else(|| panic!("no element labelled {label:?} in {page}"))
        .clone()
}

fn action(page: &Value, target: &Value, request_id: &str, extra: &Value) -> Value {
    let mut args = json!({
        "request_id": request_id,
        "session_id": page["session_id"],
        "snapshot_id": page["snapshot_id"],
        "geometry_epoch": page["geometry_epoch"],
        "target_id": target["target_id"],
        "expected_revision": target["revision"],
    });
    for (key, value) in extra.as_object().expect("object") {
        args[key] = value.clone();
    }
    args
}

/// IT-06 (time breakdown): `_meta` of an observation holds one entry per stage, in whole
/// microseconds, each the stage's own duration, plus the total measured directly. Here
/// the stages run in sequence on a virtual clock, so the numbers are exact. `total` ends at
/// the published snapshot: the commit is inside it, the response build after it (ADR-0024).
#[test]
fn it_06_an_observation_reports_the_time_of_every_stage_in_meta() {
    let costs = Costs {
        capture: Duration::from_millis(3),
        detect: Duration::from_millis(10),
        recognize: Duration::from_millis(4),
    };
    let engine = engine_with_costs(basic_scene(), LocalPolicy::default(), costs);
    let mut conn = McpServer::new(engine, McpConfig::default()).connect();
    let result = call_ok(&mut conn, 1, "observe_screen", json!({}));
    assert_eq!(
        result["_meta"][TIMINGS_META_KEY],
        json!({
            "capture_us": 3_000,
            "plan_us": 0,
            "detect_us": 10_000,
            "recognize_us": 4_000,
            "accessibility_us": 0,
            "fuse_us": 0,
            "track_us": 0,
            "commit_us": 0,
            "total_us": 17_000,
            "response_build_us": 0
        })
    );
    assert_eq!(result["_meta"][ACCESSIBILITY_META_KEY]["complete"], true);

    // The first observation of a session is a full refresh for the reason every planner
    // gives; nothing was inferred by the doubles, so the counts of inference are not measured.
    let plan = &result["_meta"][PLAN_META_KEY];
    assert_eq!(plan["work"], "full", "{plan}");
    assert_eq!(plan["full_refresh_reason"], "no_committed_base", "{plan}");
    assert_eq!(plan["region_count"], 1);
    assert_eq!(plan["region_area_px"], 320.0 * 200.0);
    assert!(
        plan["inference_count"].is_null() && plan["input_shapes"].is_null(),
        "{plan}"
    );
    assert_eq!(plan["accessibility_truncated"], false);
    assert_eq!(plan["controls_n"], 3);
    assert_eq!(plan["words_n"], 3);
    assert_eq!(plan["supports_n"], 6);

    // A page of the same snapshot, fetched by cursor, names the observation it came from.
    let paged = call_ok(&mut conn, 2, "observe_screen", json!({ "limit": 1 }));
    let cursor = paged["structuredContent"]["next_cursor"].clone();
    assert!(cursor.is_string(), "{paged}");
    let next = call_ok(
        &mut conn,
        3,
        "observe_screen",
        json!({ "limit": 1, "cursor": cursor }),
    );
    assert_eq!(
        next["_meta"][TIMINGS_META_KEY],
        paged["_meta"][TIMINGS_META_KEY]
    );

    // An error reports no timings: nothing was published to time.
    let stale = call_ok(
        &mut conn,
        4,
        "observe_screen",
        json!({ "cursor": "not-a-cursor" }),
    );
    assert_eq!(stale["isError"], true);
    assert!(stale["_meta"].get(TIMINGS_META_KEY).is_none(), "{stale}");
}

/// IT-06: an action reports the time of the stages it went through; a replay and a request
/// refused before the gate carry none (they ran nothing).
#[test]
fn it_06_an_action_reports_its_stage_times_in_meta() {
    let mut conn = server_with(private_scene(), McpConfig::default()).connect();
    let page = call_ok(&mut conn, 1, "observe_screen", json!({}))["structuredContent"].clone();
    let button = element(&page, BUTTON);
    let args = action(&page, &button, "r1", &json!({ "action": "click" }));
    let result = call_ok(&mut conn, 2, "execute_action", args.clone());
    assert_eq!(result["structuredContent"]["status"], "simulated");
    let timings = result["_meta"][ACTION_TIMINGS_META_KEY]
        .as_object()
        .unwrap_or_else(|| panic!("action timings in {result}"));
    let mut keys: Vec<&str> = timings.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "delivery_us",
            "focus_us",
            "freshness_us",
            "gate_us",
            "total_us",
            "verification_us"
        ]
    );
    assert!(timings.values().all(Value::is_u64), "{timings:?}");

    let replay = call_ok(&mut conn, 3, "execute_action", args);
    assert_eq!(replay["structuredContent"], result["structuredContent"]);
    assert!(replay["_meta"].get(ACTION_TIMINGS_META_KEY).is_none());
}

/// Every leaf of a `_meta`: its path and value.
fn leaves(prefix: &str, value: &Value, out: &mut Vec<(String, Value)>) {
    match value {
        Value::Object(map) => {
            for (key, inner) in map {
                leaves(&format!("{prefix}/{key}"), inner, out);
            }
        }
        Value::Array(items) => {
            for (i, inner) in items.iter().enumerate() {
                leaves(&format!("{prefix}/{i}"), inner, out);
            }
        }
        leaf => out.push((prefix.to_owned(), leaf.clone())),
    }
}

/// A whole session over a screen full of distinctive text, values on: observation, typing
/// (a replay, a conflict and a refusal included), a live attempt, a query that quotes a
/// value, metrics and two malformed calls. Every response, in order.
fn session_responses() -> Vec<Value> {
    let mut conn = server_with_values().connect();
    let mut responses: Vec<Value> = Vec::new();
    let mut id = 0;
    let mut send = |conn: &mut ommatix_adapter_mcp::Connection, tool: &str, args: Value| {
        id += 1;
        let response = call(conn, id, tool, args);
        responses.push(response.clone());
        response["result"].clone()
    };

    let observed = send(
        &mut conn,
        "observe_screen",
        json!({ "include_values": true }),
    );
    let page = observed["structuredContent"].clone();
    // The values are on screen and, with the operator's consent, in the page: that is the
    // contract. They must not be anywhere else.
    assert!(element(&page, "Name")["text"].is_string());
    let input = element(&page, "Name");
    let button = element(&page, BUTTON);

    let typed = json!({ "action": "type", "text": TYPED });
    send(
        &mut conn,
        "execute_action",
        action(&page, &input, "t1", &typed),
    );
    send(
        &mut conn,
        "execute_action",
        action(&page, &input, "t1", &typed),
    ); // replay
    send(
        &mut conn,
        "execute_action",
        action(
            &page,
            &input,
            "t1",
            &json!({ "action": "type", "text": "other" }),
        ), // conflict
    );
    send(
        &mut conn,
        "execute_action",
        action(
            &page,
            &button,
            "t2",
            &json!({ "action": "type", "text": TYPED }),
        ), // not an input
    );
    send(
        &mut conn,
        "execute_action",
        action(
            &page,
            &button,
            "t3",
            &json!({ "action": "click", "dry_run": false }),
        ), // denied
    );
    send(
        &mut conn,
        "observe_screen",
        json!({ "query": VALUE, "include_values": true }),
    );
    send(&mut conn, "get_engine_metrics", json!({}));
    // Invalid arguments that quote the screen's and the agent's strings.
    send(&mut conn, "observe_screen", json!({ VALUE: CAPTION }));
    send(&mut conn, "execute_action", json!({ "action": TYPED }));
    responses
}

/// The names the plan may carry (ADR-0024): the closed vocabulary of the planner, never
/// screen content.
const PLAN_WORDS: [&str; 10] = [
    "full",
    "regions",
    "idle",
    "no_committed_base",
    "geometry_changed",
    "closure_budget",
    "too_many_regions",
    "area_budget",
    "experimental_baseline",
    "adaptive_cost",
];

/// IT-06 (redaction): in every result `_meta` there are the engine keys and the server's name
/// and version; the engine's are whole numbers, booleans, `null` for a value not measured and,
/// in the plan, the closed names of the planner; nothing from the screen or the agent is in
/// any of them, and the typed text is in no response at all.
#[test]
fn it_06_telemetry_carries_numbers_only_and_nothing_from_the_screen() {
    let responses = session_responses();
    let mut meta_count = 0;
    for response in &responses {
        let Some(meta) = response["result"].get("_meta") else {
            continue;
        };
        meta_count += 1;
        let mut all = Vec::new();
        leaves("", meta, &mut all);
        for (path, leaf) in &all {
            let text = leaf.to_string();
            for secret in [VALUE, CAPTION, BUTTON, TYPED, "Name"] {
                assert!(!text.contains(secret), "{path}: {text}");
            }
            if path.starts_with(&format!("/{SERVER_INFO_KEY}")) {
                continue;
            }
            if path.starts_with(&format!("/{PLAN_META_KEY}/")) {
                assert!(
                    leaf.is_null()
                        || leaf.is_u64()
                        || leaf.is_f64()
                        || leaf.is_boolean()
                        || leaf.as_str().is_some_and(|word| PLAN_WORDS.contains(&word)),
                    "{path}: the plan is numbers, booleans, null and closed names, not {leaf}"
                );
                continue;
            }
            assert!(
                leaf.is_u64() || leaf.is_boolean(),
                "{path}: engine telemetry is whole numbers and booleans, not {leaf}"
            );
        }
        for key in meta.as_object().expect("meta is an object").keys() {
            assert!(
                [
                    TIMINGS_META_KEY,
                    PLAN_META_KEY,
                    ACTION_TIMINGS_META_KEY,
                    ACCESSIBILITY_META_KEY,
                    SERVER_INFO_KEY
                ]
                .contains(&key.as_str()),
                "unexpected _meta key {key:?}"
            );
        }
    }
    assert!(meta_count >= 5, "{meta_count} results carried _meta");

    // The typed text is the actuator's alone: it is in no response, error or otherwise.
    for response in &responses {
        assert!(
            !response.to_string().contains(TYPED),
            "typed text echoed in {response}"
        );
    }
}

fn server_with_values() -> McpServer {
    McpServer::new(
        common::engine_with(private_scene(), open_policy()),
        McpConfig::default(),
    )
}

/// IT-06 (rejection reasons): a refusal carries its reason as a code and a fixed
/// sentence, never the text it was about; the protocol errors do not echo what the agent
/// sent either.
#[test]
fn it_06_rejection_reasons_are_codes_and_fixed_sentences() {
    let mut conn = server_with_values().connect();
    let page = call_ok(&mut conn, 1, "observe_screen", json!({}))["structuredContent"].clone();
    let caption = element(&page, CAPTION);
    // A text node cannot be clicked.
    let result = call_ok(
        &mut conn,
        2,
        "execute_action",
        action(&page, &caption, "r1", &json!({ "action": "click" })),
    );
    assert_eq!(result["isError"], true);
    assert_eq!(
        result["structuredContent"]["error"]["code"],
        "NOT_ACTIONABLE"
    );
    for field in ["message"] {
        let sentence = result["structuredContent"]["error"][field]
            .as_str()
            .unwrap();
        for secret in [VALUE, CAPTION, BUTTON, TYPED] {
            assert!(!sentence.contains(secret), "{sentence}");
        }
    }
    // Protocol errors: an unknown tool, an unknown method and a malformed message, each
    // carrying a string the agent chose.
    let hostile = format!("{TYPED} {VALUE}");
    let unknown_tool = call(&mut conn, 3, &hostile, json!({}));
    let unknown_method = common::request(
        &mut conn,
        4,
        &hostile,
        json!({ "_meta": common::modern_meta() }),
    );
    let malformed = send_raw(
        &mut conn,
        &format!(r#"{{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{{"name":"{hostile}","arguments":["{hostile}"]}}}}"#),
    )
    .expect("a request gets a response");
    for response in [unknown_tool, unknown_method, malformed] {
        assert!(response.get("error").is_some(), "{response}");
        assert!(
            error_code(&response) < 0,
            "a JSON-RPC error code: {response}"
        );
        let text = response.to_string();
        assert!(!text.contains(TYPED) && !text.contains(VALUE), "{text}");
    }
    assert_eq!(SESSION_ID, page["session_id"]);
}

/// ADR-0032 D0: refusal diagnostics are counts/booleans in transport metadata only.
#[test]
fn freshness_refusal_reports_counts_only_in_meta_and_replay_has_none() {
    let mut conn = McpServer::new(common::freshness_engine(), McpConfig::default()).connect();
    let page = call_ok(&mut conn, 1, "observe_screen", json!({}))["structuredContent"].clone();
    let button = element(&page, "Save");
    let args = action(
        &page,
        &button,
        "d0",
        &json!({"action": "click", "dry_run": false}),
    );
    let result = call_ok(&mut conn, 2, "execute_action", args.clone());
    assert_eq!(
        result["structuredContent"]["error"]["code"],
        "STALE_SNAPSHOT"
    );
    assert_eq!(
        result["_meta"][ACTION_TIMINGS_META_KEY]["freshness_diff"],
        json!({"total_tiles": 1, "touches_target": true})
    );
    let structured = result["structuredContent"].to_string();
    for key in [
        "freshness_diff",
        "total_tiles",
        "group_tiles",
        "touches_target",
    ] {
        assert!(!structured.contains(key), "{structured}");
        assert!(!result["content"].to_string().contains(key));
    }
    let replay = call_ok(&mut conn, 3, "execute_action", args);
    assert_eq!(replay["structuredContent"], result["structuredContent"]);
    assert!(replay["_meta"].get(ACTION_TIMINGS_META_KEY).is_none());
}
