//! Planned security test SEC-04 (plan_pruebas.csv, RF-16, risk R09): nothing sensitive
//! leaves by default, at the MCP boundary. The server offers three tools and nothing else;
//! no tool takes a path, a URL or an image; no result carries pixels, a file or a key the
//! dossier's schemas do not define; values are redacted unless the agent asks and the
//! operator allows. The repository-wide half (no code path writes a capture, opens a
//! socket or prints to stdout) is in `src/acceptance/tests/repository.rs`, and the process
//! half (what `ommatix serve` writes to stdout, stderr and disk) in `src/app/tests`.

mod common;

use std::collections::BTreeSet;

use common::{
    Scene, bx, call, call_ok, control, dossier, error_code, modern, modern_meta, request,
    server_with,
};
use ommatix_adapter_mcp::McpConfig;
use ommatix_application::LocalPolicy;
use ommatix_domain::fusion::{Actionability, Role, Word};
use serde_json::{Value, json};

const VALUE: &str = "VALUE-4711";
const TYPED: &str = "TYPED-SECRET-2718";

/// A form with a labelled input that shows `VALUE`.
fn form_scene() -> Scene {
    Scene {
        width: 320,
        height: 200,
        controls: vec![
            control(
                bx(10.0, 70.0, 250.0, 100.0),
                Role::Input,
                Actionability::Confirmed,
            ),
            control(
                bx(10.0, 10.0, 90.0, 40.0),
                Role::Button,
                Actionability::Confirmed,
            ),
        ],
        words: vec![
            Word::new("Name", bx(10.0, 50.0, 60.0, 65.0)),
            Word::new(VALUE, bx(20.0, 75.0, 120.0, 95.0)),
            Word::new("Save", bx(25.0, 15.0, 75.0, 35.0)),
        ],
    }
}

fn policy(include_values: bool) -> LocalPolicy {
    LocalPolicy {
        include_values,
        ..LocalPolicy::default()
    }
}

fn connection(include_values: bool) -> ommatix_adapter_mcp::Connection {
    let engine = common::engine_with(form_scene(), policy(include_values));
    ommatix_adapter_mcp::McpServer::new(engine, McpConfig::default()).connect()
}

/// Every `properties` name the dossier's schemas define, at any depth.
fn contract_keys() -> BTreeSet<String> {
    fn walk(value: &Value, out: &mut BTreeSet<String>) {
        match value {
            Value::Object(map) => {
                if let Some(Value::Object(props)) = map.get("properties") {
                    out.extend(props.keys().cloned());
                }
                map.values().for_each(|v| walk(v, out));
            }
            Value::Array(items) => items.iter().for_each(|v| walk(v, out)),
            _ => {}
        }
    }
    let mut keys = BTreeSet::new();
    for entry in std::fs::read_dir(dossier("contratos/esquemas")).expect("schemas") {
        let text = std::fs::read_to_string(entry.unwrap().path()).unwrap();
        walk(&serde_json::from_str(&text).unwrap(), &mut keys);
    }
    keys
}

fn object_keys(value: &Value, out: &mut BTreeSet<String>) {
    match value {
        Value::Object(map) => {
            for (key, inner) in map {
                out.insert(key.clone());
                object_keys(inner, out);
            }
        }
        Value::Array(items) => items.iter().for_each(|v| object_keys(v, out)),
        _ => {}
    }
}

/// SEC-04: the server declares the `tools` capability and no other. Nothing else the MCP
/// can carry (resources, prompts, sampling, logging, completions, roots) is served, so
/// there is no second channel on which pixels, files or logs could leave.
#[test]
fn sec_04_the_server_offers_tools_and_nothing_else() {
    let mut conn = server_with(form_scene(), McpConfig::default()).connect();
    let discover = modern(&mut conn, 1, "server/discover", json!({}));
    let capabilities = discover["result"]["capabilities"]
        .as_object()
        .unwrap_or_else(|| panic!("capabilities in {discover}"));
    let declared: Vec<&str> = capabilities.keys().map(String::as_str).collect();
    assert_eq!(declared, ["tools"], "{discover}");

    for (i, method) in [
        "resources/list",
        "resources/read",
        "resources/templates/list",
        "resources/subscribe",
        "prompts/list",
        "prompts/get",
        "completion/complete",
        "logging/setLevel",
        "sampling/createMessage",
        "roots/list",
        "elicitation/create",
    ]
    .into_iter()
    .enumerate()
    {
        let response = modern(&mut conn, 10 + i as u64, method, json!({}));
        assert_eq!(error_code(&response), -32601, "{method}: {response}");
    }
}

/// SEC-04: no input schema has a field that names a path, a URL, a file, an image or a
/// destination, and no tool description or name promises a screenshot or a capture.
#[test]
fn sec_04_no_tool_takes_a_path_a_url_or_an_image() {
    let mut conn = server_with(form_scene(), McpConfig::default()).connect();
    let list = modern(&mut conn, 1, "tools/list", json!({}));
    let tools = list["result"]["tools"].as_array().expect("tools");
    assert_eq!(tools.len(), 3);
    let forbidden = [
        "path",
        "file",
        "filename",
        "url",
        "uri",
        "image",
        "screenshot",
        "capture",
        "pixels",
        "data",
        "blob",
        "destination",
        "output",
        "command",
        "shell",
        "script",
        "coordinates",
        "x",
        "y",
    ];
    for tool in tools {
        let name = tool["name"].as_str().unwrap();
        for word in [
            "screenshot",
            "capture",
            "export",
            "upload",
            "save",
            "file",
            "read_",
        ] {
            assert!(!name.contains(word), "{name}");
        }
        let description = tool["description"].as_str().unwrap().to_lowercase();
        for word in ["screenshot", "upload", "send the image"] {
            assert!(!description.contains(word), "{name}: {description}");
        }
        let mut keys = BTreeSet::new();
        object_keys(&tool["inputSchema"], &mut keys);
        let properties = tool["inputSchema"]["properties"].as_object();
        for field in properties.into_iter().flat_map(|p| p.keys()) {
            assert!(!forbidden.contains(&field.as_str()), "{name} takes {field}");
        }
    }
}

/// SEC-04: whatever the agent asks, every result is text content with a structured object
/// made only of keys the dossier's schemas define; there is no image block, no resource
/// link, no `data`, `uri` or `path` anywhere. `isError` results too.
#[test]
fn sec_04_results_carry_only_contract_keys_and_text() {
    let allowed = contract_keys();
    assert!(allowed.contains("target_id") && !allowed.contains("image"));
    let mut conn = connection(true);
    let page = call_ok(
        &mut conn,
        1,
        "observe_screen",
        json!({ "include_values": true }),
    );
    let structured = page["structuredContent"].clone();
    let input = structured["elements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["role"] == "input")
        .unwrap()
        .clone();
    let act = |request_id: &str, extra: Value| {
        let mut args = json!({
            "request_id": request_id,
            "session_id": structured["session_id"],
            "snapshot_id": structured["snapshot_id"],
            "geometry_epoch": structured["geometry_epoch"],
            "target_id": input["target_id"],
            "expected_revision": input["revision"],
        });
        for (k, v) in extra.as_object().unwrap() {
            args[k] = v.clone();
        }
        args
    };
    let results = [
        page,
        call_ok(&mut conn, 2, "observe_screen", json!({ "limit": 1 })),
        call_ok(&mut conn, 3, "observe_screen", json!({ "cursor": "x" })),
        call_ok(&mut conn, 4, "get_engine_metrics", json!({})),
        call_ok(
            &mut conn,
            5,
            "execute_action",
            act("a1", json!({ "action": "click" })),
        ),
        call_ok(
            &mut conn,
            6,
            "execute_action",
            act("a2", json!({ "action": "type", "text": TYPED })),
        ),
        call_ok(
            &mut conn,
            7,
            "execute_action",
            act("a3", json!({ "action": "click", "dry_run": false })),
        ),
        call_ok(
            &mut conn,
            8,
            "execute_action",
            act(
                "a4",
                json!({ "action": "scroll", "direction": "down", "steps": 2 }),
            ),
        ),
    ];
    for result in &results {
        let content = result["content"].as_array().expect("content");
        assert_eq!(content.len(), 1);
        assert_eq!(content[0]["type"], "text");
        let mut content_keys = BTreeSet::new();
        object_keys(&content[0], &mut content_keys);
        let content_keys: Vec<&str> = content_keys.iter().map(String::as_str).collect();
        assert_eq!(content_keys, ["text", "type"], "no mime type, data or uri");
        let mut keys = BTreeSet::new();
        object_keys(&result["structuredContent"], &mut keys);
        let unknown: Vec<&String> = keys.difference(&allowed).collect();
        assert!(unknown.is_empty(), "keys outside the schemas: {unknown:?}");
        let text = result.to_string();
        for marker in ["data:image", "iVBORw0KGgo", "image/png", "file://", TYPED] {
            assert!(!text.contains(marker), "{marker} in {text}");
        }
        assert!(result.get("resource").is_none() && result.get("resources").is_none());
    }
}

/// SEC-04: a field's content is in the page only when the agent asks **and** the operator
/// allows, over the wire as in the engine; the typed text of an action is never echoed.
#[test]
fn sec_04_values_follow_the_operators_policy_over_the_wire() {
    for (asks, allows) in [(false, false), (true, false), (false, true), (true, true)] {
        let mut conn = connection(allows);
        let page = call_ok(
            &mut conn,
            1,
            "observe_screen",
            json!({ "include_values": asks }),
        );
        let shown = page.to_string().contains(VALUE);
        assert_eq!(shown, asks && allows, "asks {asks}, allows {allows}");
        let probe = call_ok(
            &mut conn,
            2,
            "observe_screen",
            json!({ "include_values": asks, "query": "value-4711" }),
        );
        assert_eq!(
            probe["structuredContent"]["total_elements"],
            u64::from(asks && allows),
            "a query must not reveal what is hidden (asks {asks}, allows {allows})"
        );
    }
    // The agent cannot widen its own view: `include_values` is the request's, and a
    // policy field in the arguments is a contract violation.
    let mut conn = connection(false);
    let response = call(
        &mut conn,
        3,
        "observe_screen",
        json!({ "include_values": true, "policy": { "include_values": true } }),
    );
    assert_eq!(error_code(&response), -32602, "{response}");
    let response = request(
        &mut conn,
        4,
        "tools/call",
        json!({
            "name": "observe_screen",
            "arguments": { "include_values": true },
            "_meta": modern_meta(),
            "include_values": true
        }),
    );
    let page = response["result"].to_string();
    assert!(!page.contains(VALUE), "{page}");
}
