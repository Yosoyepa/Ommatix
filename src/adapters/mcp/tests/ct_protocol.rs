//! Planned contract tests CT-05..CT-10 (plan_pruebas.csv): MCP profiles, notifications,
//! tool listing, the tool/protocol error split, message size limits and invalid inputs.

mod common;

use common::{
    SESSION_ID, basic_scene, call, call_ok, dossier, error_code, legacy_server, modern,
    modern_meta, read_json, request, schema, send, send_raw, server, server_with, word_grid,
};
use ommatix_adapter_mcp::{
    Connection, DEFAULT_MAX_MESSAGE_BYTES, LEGACY_PROTOCOL_VERSION, MODERN_PROTOCOL_VERSION,
    McpConfig, McpServer, Profile,
};
use serde_json::{Value, json};

const UNSUPPORTED_VERSION: i64 = -32022;
const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;
const INTERNAL_ERROR: i64 = -32603;

fn jsonl(path: &str) -> Vec<String> {
    std::fs::read_to_string(dossier(path))
        .expect("example exists")
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(str::to_owned)
        .collect()
}

/// Runs `input` through [`McpServer::serve`] and returns the output lines.
fn serve(server: McpServer, input: &[u8]) -> Vec<String> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("runtime");
    let mut output = Vec::new();
    rt.block_on(server.serve(input, &mut output))
        .expect("in-memory I/O does not fail");
    let output = String::from_utf8(output).expect("UTF-8 output");
    assert!(output.is_empty() || output.ends_with('\n'));
    output.lines().map(str::to_owned).collect()
}

fn tools_list_request(id: u64) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "method": "tools/list", "params": { "_meta": modern_meta() } })
        .to_string()
}

// ---- CT-05 ----

#[test]
fn ct_05_modern_requests_carry_their_metadata_and_results_their_type() {
    let requests = jsonl("contratos/ejemplos/mcp_moderno_solicitudes.jsonl");
    let expected: Vec<Value> = jsonl("contratos/ejemplos/mcp_moderno_respuestas.jsonl")
        .iter()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let mut conn = server().connect();
    assert_eq!(conn.profile(), None);
    let responses: Vec<Value> = requests
        .iter()
        .map(|r| send_raw(&mut conn, r).expect("every example is a request"))
        .collect();
    assert_eq!(
        conn.profile(),
        Some(Profile::Modern),
        "no initialize needed"
    );

    for (got, want) in responses.iter().zip(&expected) {
        assert_eq!(got["id"], want["id"]);
        assert_eq!(got["result"]["resultType"], "complete", "{got}");
        assert!(got["result"]["_meta"]["io.modelcontextprotocol/serverInfo"]["name"].is_string());
    }
    // server/discover
    let discover = &responses[0]["result"];
    assert_eq!(
        discover["supportedVersions"],
        json!([MODERN_PROTOCOL_VERSION])
    );
    assert_eq!(discover["capabilities"], json!({ "tools": {} }));
    assert_eq!(discover["ttlMs"], 0);
    assert_eq!(discover["cacheScope"], "private");
    // tools/list: the documented tools with the dossier's schemas and cache hints.
    let tools = &responses[1]["result"];
    assert_eq!(tools["ttlMs"], 0);
    assert_eq!(tools["cacheScope"], "private");
    let want_tools = expected[1]["result"]["tools"].as_array().unwrap();
    let got_tools = tools["tools"].as_array().unwrap();
    assert_eq!(got_tools.len(), want_tools.len());
    for (g, w) in got_tools.iter().zip(want_tools) {
        assert_eq!(g["name"], w["name"]);
        assert_eq!(g["inputSchema"], w["inputSchema"]);
        assert_eq!(g["outputSchema"], w["outputSchema"]);
    }
    // tools/call: isError explicit, structured content checked against the contract.
    for (response, tool) in
        responses[2..]
            .iter()
            .zip(["observe_screen", "execute_action", "execute_action"])
    {
        common::check_tool_result(&response["result"], tool);
    }
    // The dossier's stale action (snapshot s16 was never published) is a tool error.
    assert_eq!(responses[4]["result"]["isError"], true);
    assert_eq!(
        responses[4]["result"]["structuredContent"]["status"],
        "rejected"
    );
}

#[test]
fn ct_05_every_modern_request_needs_its_own_metadata() {
    let mut conn = server().connect();
    // Before any profile: no metadata, no handshake.
    let r = request(&mut conn, 1, "tools/list", json!({}));
    assert_eq!(error_code(&r), INVALID_PARAMS);
    assert!(
        r["error"]["message"]
            .as_str()
            .unwrap()
            .contains("protocolVersion")
    );

    modern(&mut conn, 2, "tools/list", json!({}));
    assert_eq!(conn.profile(), Some(Profile::Modern));
    // Metadata is per request, not per connection.
    let r = request(&mut conn, 3, "tools/list", json!({}));
    assert_eq!(error_code(&r), INVALID_PARAMS);
    let r = request(
        &mut conn,
        4,
        "tools/list",
        json!({ "_meta": { "io.modelcontextprotocol/protocolVersion": MODERN_PROTOCOL_VERSION } }),
    );
    assert_eq!(error_code(&r), INVALID_PARAMS);
    assert!(
        r["error"]["message"]
            .as_str()
            .unwrap()
            .contains("clientCapabilities")
    );
    let r = request(&mut conn, 5, "tools/list", json!({ "_meta": "x" }));
    assert_eq!(error_code(&r), INVALID_PARAMS);
    // Legacy-only methods do not exist in the modern profile.
    let r = modern(&mut conn, 6, "ping", json!({}));
    assert_eq!(error_code(&r), METHOD_NOT_FOUND);
}

// ---- CT-06 ----

#[test]
fn ct_06_unsupported_version_error_in_the_modern_profile() {
    for legacy_enabled in [false, true] {
        let config = McpConfig {
            legacy_enabled,
            ..McpConfig::default()
        };
        let mut conn = server_with(basic_scene(), config).connect();
        for (id, version) in [
            (1, "2099-01-01"),
            (2, LEGACY_PROTOCOL_VERSION),
            (3, "2024-11-05"),
        ] {
            let mut meta = modern_meta();
            meta["io.modelcontextprotocol/protocolVersion"] = json!(version);
            let r = request(&mut conn, id, "tools/list", json!({ "_meta": meta }));
            assert_eq!(error_code(&r), UNSUPPORTED_VERSION, "{r}");
            assert_eq!(r["error"]["data"]["requested"], version);
            assert_eq!(
                r["error"]["data"]["supported"],
                json!([MODERN_PROTOCOL_VERSION])
            );
        }
        assert_eq!(conn.profile(), None, "a rejected request selects nothing");
    }
}

#[test]
fn ct_06_initialize_without_the_legacy_profile_is_an_unsupported_version() {
    let mut conn = server().connect();
    let r = request(
        &mut conn,
        1,
        "initialize",
        json!({ "protocolVersion": LEGACY_PROTOCOL_VERSION, "capabilities": {},
                "clientInfo": { "name": "c", "version": "1" } }),
    );
    assert_eq!(error_code(&r), UNSUPPORTED_VERSION);
    assert_eq!(r["error"]["data"]["requested"], LEGACY_PROTOCOL_VERSION);
    assert_eq!(
        r["error"]["data"]["supported"],
        json!([MODERN_PROTOCOL_VERSION])
    );
    // The connection can still go modern.
    assert!(modern(&mut conn, 2, "tools/list", json!({}))["result"].is_object());
}

#[test]
fn ct_06_unsupported_version_in_the_legacy_profile() {
    let mut conn = legacy_server().connect();
    // 2025-11-25 negotiation: an unknown requested version gets the supported one back.
    let r = request(
        &mut conn,
        1,
        "initialize",
        json!({ "protocolVersion": "2024-11-05", "capabilities": {},
                "clientInfo": { "name": "c", "version": "1" } }),
    );
    assert_eq!(r["result"]["protocolVersion"], LEGACY_PROTOCOL_VERSION);
    assert_eq!(conn.profile(), Some(Profile::Legacy));
    // A legacy session request that declares another version: the legacy error shape.
    let r = request(
        &mut conn,
        2,
        "tools/list",
        json!({ "_meta": modern_meta() }),
    );
    assert_eq!(error_code(&r), INVALID_PARAMS);
    assert_eq!(r["error"]["message"], "Unsupported protocol version");
    assert_eq!(
        r["error"]["data"]["supported"],
        json!([LEGACY_PROTOCOL_VERSION])
    );
    assert_eq!(r["error"]["data"]["requested"], MODERN_PROTOCOL_VERSION);
}

// ---- CT-07 ----

fn notifications() -> Vec<Value> {
    vec![
        json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
        json!({ "jsonrpc": "2.0", "method": "notifications/cancelled",
                "params": { "requestId": 1, "reason": "test" } }),
        json!({ "jsonrpc": "2.0", "method": "notifications/progress",
                "params": { "progressToken": "t", "progress": 1 } }),
        json!({ "jsonrpc": "2.0", "method": "notifications/unknown", "params": { "_meta": modern_meta() } }),
        json!({ "jsonrpc": "2.0", "method": "tools/list" }),
        json!({ "jsonrpc": "1.0", "method": "notifications/initialized" }),
        json!({ "jsonrpc": "2.0", "method": "notifications/initialized", "params": "not-an-object" }),
        // A client response: this server sends no requests, so there is nothing to answer.
        json!({ "jsonrpc": "2.0", "id": 99, "result": {} }),
        json!({ "jsonrpc": "2.0", "id": 98, "error": { "code": -1, "message": "x" } }),
    ]
}

#[test]
fn ct_07_notifications_are_never_answered_in_any_state() {
    // Unselected, modern and legacy connections.
    let mut fresh = server().connect();
    let mut modern_conn = server().connect();
    modern(&mut modern_conn, 1, "tools/list", json!({}));
    let mut legacy_conn = legacy_server().connect();
    request(
        &mut legacy_conn,
        1,
        "initialize",
        json!({ "protocolVersion": LEGACY_PROTOCOL_VERSION, "capabilities": {},
                "clientInfo": { "name": "c", "version": "1" } }),
    );
    for conn in [&mut fresh, &mut modern_conn, &mut legacy_conn] {
        let profile = conn.profile();
        for n in notifications() {
            assert_eq!(send(conn, &n), None, "{n}");
        }
        assert_eq!(conn.profile(), profile, "notifications select no profile");
    }
}

#[test]
fn ct_07_a_notification_before_the_lifecycle_does_not_end_the_connection() {
    let mut input = String::new();
    for n in notifications() {
        input.push_str(&n.to_string());
        input.push('\n');
    }
    input.push_str(&tools_list_request(7));
    input.push('\n');
    let lines = serve(server(), input.as_bytes());
    assert_eq!(lines.len(), 1, "only the request is answered: {lines:?}");
    let response: Value = serde_json::from_str(&lines[0]).unwrap();
    assert_eq!(response["id"], 7);
    assert!(response["result"]["tools"].is_array());
}

// ---- CT-08 ----

fn initialize_params() -> Value {
    json!({ "protocolVersion": LEGACY_PROTOCOL_VERSION, "capabilities": {},
            "clientInfo": { "name": "legacy-example", "version": "1.0" } })
}

#[test]
fn ct_08_legacy_handshake_only_when_enabled() {
    // Disabled (the default): discovery advertises only the modern profile.
    let mut conn = server().connect();
    let discover = modern(&mut conn, 1, "server/discover", json!({}));
    assert_eq!(
        discover["result"]["supportedVersions"],
        json!([MODERN_PROTOCOL_VERSION])
    );
    let mut conn = server().connect();
    let r = request(&mut conn, 1, "initialize", initialize_params());
    assert_eq!(error_code(&r), UNSUPPORTED_VERSION);
    assert_eq!(conn.profile(), None);

    // Enabled: discovery advertises both.
    let mut conn = legacy_server().connect();
    let discover = modern(&mut conn, 1, "server/discover", json!({}));
    assert_eq!(
        discover["result"]["supportedVersions"],
        json!([MODERN_PROTOCOL_VERSION, LEGACY_PROTOCOL_VERSION])
    );
}

#[test]
fn ct_08_legacy_session_uses_legacy_rules_only() {
    let mut conn = legacy_server().connect();
    let example = jsonl("contratos/ejemplos/mcp_legacy_solicitudes.jsonl");
    let init = send_raw(&mut conn, &example[0]).expect("initialize is answered");
    let result = &init["result"];
    assert_eq!(result["protocolVersion"], LEGACY_PROTOCOL_VERSION);
    assert_eq!(result["capabilities"], json!({ "tools": {} }));
    assert_eq!(result["serverInfo"]["name"], "ommatix");
    assert!(result.get("resultType").is_none());
    assert_eq!(conn.profile(), Some(Profile::Legacy));
    assert!(!conn.legacy_initialized());

    assert_eq!(
        send_raw(&mut conn, &example[1]),
        None,
        "initialized is a notification"
    );
    assert!(conn.legacy_initialized());

    let list = send_raw(&mut conn, &example[2]).expect("tools/list is answered");
    let result = &list["result"];
    assert_eq!(result["tools"].as_array().map(Vec::len), Some(3));
    for modern_only in ["resultType", "ttlMs", "cacheScope", "_meta"] {
        assert!(result.get(modern_only).is_none(), "{modern_only}: {result}");
    }

    // Tool calls without per-request metadata; legacy result shape.
    let r = request(
        &mut conn,
        3,
        "tools/call",
        json!({ "name": "get_engine_metrics", "arguments": {} }),
    );
    common::check_tool_result(&r["result"], "get_engine_metrics");
    assert!(r["result"].get("resultType").is_none());
    assert!(r["result"].get("_meta").is_none());
    assert_eq!(
        request(&mut conn, 4, "ping", json!({}))["result"],
        json!({})
    );

    // Modern rules are not available here.
    let r = modern(&mut conn, 5, "server/discover", json!({}));
    assert_eq!(error_code(&r), INVALID_PARAMS, "{r}");
    let r = request(&mut conn, 6, "server/discover", json!({}));
    assert_eq!(error_code(&r), METHOD_NOT_FOUND);
    let r = request(&mut conn, 7, "initialize", initialize_params());
    assert_eq!(error_code(&r), INVALID_REQUEST, "already initialized");
    assert_eq!(conn.profile(), Some(Profile::Legacy));
}

#[test]
fn ct_08_a_modern_connection_never_switches_to_legacy() {
    let mut conn = legacy_server().connect();
    modern(&mut conn, 1, "server/discover", json!({}));
    assert_eq!(conn.profile(), Some(Profile::Modern));
    let r = request(&mut conn, 2, "initialize", initialize_params());
    assert_eq!(error_code(&r), UNSUPPORTED_VERSION);
    assert_eq!(conn.profile(), Some(Profile::Modern));
    // Legacy-style requests (no metadata) stay rejected.
    assert_eq!(
        error_code(&request(&mut conn, 3, "tools/list", json!({}))),
        INVALID_PARAMS
    );
}

#[test]
fn ct_08_ping_before_initialize_only_with_the_legacy_profile() {
    let mut conn = legacy_server().connect();
    assert_eq!(
        request(&mut conn, 1, "ping", json!({}))["result"],
        json!({})
    );
    assert_eq!(conn.profile(), None);
    let mut conn = server().connect();
    assert_eq!(
        error_code(&request(&mut conn, 1, "ping", json!({}))),
        INVALID_PARAMS
    );
}

// ---- CT-09 ----

#[test]
fn ct_09_tools_list_is_exactly_the_three_tools_with_the_dossier_schemas() {
    let mut conn = server().connect();
    let first = conn
        .handle_message(tools_list_request(1).as_bytes())
        .unwrap();
    let again = conn
        .handle_message(tools_list_request(1).as_bytes())
        .unwrap();
    assert_eq!(first, again, "deterministic listing");
    let other = server()
        .connect()
        .handle_message(tools_list_request(1).as_bytes())
        .unwrap();
    assert_eq!(first, other, "independent of the connection");

    let list: Value = serde_json::from_str(&first).unwrap();
    let tools = list["result"]["tools"].as_array().unwrap();
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(
        names,
        ["execute_action", "get_engine_metrics", "observe_screen"]
    );
    for tool in tools {
        let name = tool["name"].as_str().unwrap();
        assert_eq!(tool["inputSchema"], schema(&format!("{name}.input")));
        assert_eq!(tool["outputSchema"], schema(&format!("{name}.output")));
        assert!(tool["description"].as_str().is_some_and(|d| !d.is_empty()));
    }
}

#[test]
fn ct_09_tool_errors_and_protocol_errors_are_kept_apart() {
    let mut conn = server().connect();
    // Expected domain error → tool result with isError (never a JSON-RPC error).
    let result = call_ok(
        &mut conn,
        1,
        "execute_action",
        json!({ "request_id": "r1", "session_id": SESSION_ID, "snapshot_id": "s999",
                "geometry_epoch": 1, "target_id": "e1", "expected_revision": 1,
                "action": "click" }),
    );
    assert_eq!(result["isError"], true);
    let action = &result["structuredContent"];
    assert_eq!(action["status"], "rejected");
    assert_eq!(action["input_delivered"], false);
    assert_eq!(action["verified"], false);
    assert!(action["error"]["code"].is_string());

    // Metrics before any observation: nothing invented.
    let metrics = call_ok(&mut conn, 2, "get_engine_metrics", json!({}));
    assert_eq!(metrics["isError"], false);
    assert_eq!(
        metrics["structuredContent"]["timings_source"],
        "not_measured"
    );
    assert_eq!(metrics["structuredContent"]["mode"], "simulation");

    // Protocol errors.
    assert_eq!(
        error_code(&call(&mut conn, 3, "click_at", json!({ "x": 1, "y": 2 }))),
        INVALID_PARAMS,
        "unknown tool"
    );
    assert_eq!(
        error_code(&call(
            &mut conn,
            4,
            "get_engine_metrics",
            json!({ "verbose": true })
        )),
        INVALID_PARAMS,
        "arguments outside the input schema"
    );
    assert_eq!(
        error_code(&modern(
            &mut conn,
            5,
            "tools/call",
            json!({ "arguments": {} })
        )),
        INVALID_PARAMS,
        "no tool name"
    );
    assert_eq!(
        error_code(&modern(&mut conn, 6, "resources/list", json!({}))),
        METHOD_NOT_FOUND
    );
    assert_eq!(
        error_code(&modern(
            &mut conn,
            7,
            "tools/list",
            json!({ "cursor": "x" })
        )),
        INVALID_PARAMS
    );
    let parse = send_raw(&mut conn, "{not json").unwrap();
    assert_eq!(error_code(&parse), PARSE_ERROR);
    assert_eq!(parse["id"], Value::Null);
    for (raw, id) in [
        ("[]", Value::Null),
        (
            r#"[{"jsonrpc":"2.0","id":1,"method":"tools/list"}]"#,
            Value::Null,
        ),
        (
            r#"{"jsonrpc":"2.0","id":null,"method":"tools/list"}"#,
            Value::Null,
        ),
        (
            r#"{"jsonrpc":"2.0","id":1.5,"method":"tools/list"}"#,
            Value::Null,
        ),
        (r#"{"id":8,"method":"tools/list"}"#, json!(8)),
        (r#"{"jsonrpc":"2.0","id":"a","method":7}"#, json!("a")),
        (r#"{"jsonrpc":"2.0","id":9}"#, json!(9)),
    ] {
        let r = send_raw(&mut conn, raw).unwrap();
        assert_eq!(error_code(&r), INVALID_REQUEST, "{raw}");
        assert_eq!(r["id"], id, "{raw}");
    }
    let r = send_raw(
        &mut conn,
        r#"{"jsonrpc":"2.0","id":10,"method":"tools/list","params":[1]}"#,
    )
    .unwrap();
    assert_eq!(error_code(&r), INVALID_PARAMS);
}

// ---- CT-10 ----

/// A modern `tools/list` request padded with spaces to exactly `len` bytes.
fn padded_request(id: u64, len: usize) -> String {
    let base = tools_list_request(id);
    assert!(base.len() < len);
    let mut padded = base[..base.len() - 1].to_owned();
    padded.push_str(&" ".repeat(len - base.len()));
    padded.push('}');
    assert_eq!(padded.len(), len);
    padded
}

#[test]
fn ct_10_incoming_messages_over_64_kib_are_rejected_without_buffering() {
    let mut input = Vec::new();
    for line in [
        padded_request(1, DEFAULT_MAX_MESSAGE_BYTES),
        padded_request(2, DEFAULT_MAX_MESSAGE_BYTES + 1),
        "x".repeat(4 * 1024 * 1024),
        tools_list_request(3),
    ] {
        input.extend_from_slice(line.as_bytes());
        input.push(b'\n');
    }
    let lines = serve(server(), &input);
    assert_eq!(lines.len(), 4, "{lines:?}");
    let responses: Vec<Value> = lines
        .iter()
        .map(|l| {
            assert!(l.len() <= DEFAULT_MAX_MESSAGE_BYTES);
            serde_json::from_str(l).unwrap()
        })
        .collect();
    assert_eq!(responses[0]["id"], 1, "exactly 64 KiB is accepted");
    assert!(responses[0]["result"]["tools"].is_array());
    for oversized in &responses[1..3] {
        assert_eq!(error_code(oversized), INVALID_REQUEST);
        assert_eq!(oversized["id"], Value::Null);
        assert_eq!(
            oversized["error"]["data"]["maxMessageBytes"],
            DEFAULT_MAX_MESSAGE_BYTES
        );
    }
    assert_eq!(
        responses[3]["id"], 3,
        "the stream recovers after an oversized line"
    );
}

#[test]
fn ct_10_no_outgoing_message_exceeds_the_limit() {
    // A tiny limit makes even tools/list too large: an error is sent instead.
    let config = McpConfig {
        legacy_enabled: false,
        max_message_bytes: 2048,
    };
    let mut conn = server_with(basic_scene(), config).connect();
    let line = conn
        .handle_message(tools_list_request(1).as_bytes())
        .unwrap();
    assert!(line.len() <= 2048);
    let r: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(error_code(&r), INTERNAL_ERROR);
    assert_eq!(r["id"], 1);

    // An id so long that even the error cannot echo it.
    let mut conn = server().connect();
    let huge_id = "i".repeat(DEFAULT_MAX_MESSAGE_BYTES - 200);
    let raw = json!({ "jsonrpc": "2.0", "id": huge_id, "method": "tools/list",
                      "params": { "_meta": modern_meta() } })
    .to_string();
    let line = conn.handle_message(raw.as_bytes()).unwrap();
    assert!(line.len() <= DEFAULT_MAX_MESSAGE_BYTES);
    let r: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(error_code(&r), INTERNAL_ERROR);
}

#[test]
fn ct_10_an_observation_too_large_for_one_message_asks_to_reduce_the_request() {
    // 200 elements with 128-character labels do not fit in 64 KiB (with the text block).
    let mut conn = server_with(word_grid(200, 128), McpConfig::default()).connect();
    let line = conn
        .handle_message(
            json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call",
                    "params": { "_meta": modern_meta(), "name": "observe_screen",
                                "arguments": { "limit": 200 } } })
            .to_string()
            .as_bytes(),
        )
        .unwrap();
    assert!(line.len() <= DEFAULT_MAX_MESSAGE_BYTES);
    let r: Value = serde_json::from_str(&line).unwrap();
    common::check_tool_result(&r["result"], "observe_screen");
    assert_eq!(r["result"]["isError"], true);
    let error = &r["result"]["structuredContent"]["error"];
    assert_eq!(error["code"], "LIMIT_EXCEEDED");
    assert_eq!(error["next_step"], "reduce_request");

    // A smaller page of the same screen fits.
    let page = call_ok(&mut conn, 2, "observe_screen", json!({ "limit": 50 }));
    assert_eq!(page["isError"], false);
    assert_eq!(page["structuredContent"]["truncated"], true);
}

#[test]
fn ct_10_invalid_inputs_are_rejected() {
    let mut conn: Connection = server().connect();
    let invalid = [
        "click_con_coordenadas.json",
        "click_con_texto.json",
        "type_sin_texto.json",
        "scroll_sin_direccion.json",
        "accion_shell.json",
        "revision_cero.json",
    ];
    for (i, file) in invalid.iter().enumerate() {
        let args = read_json(&format!("contratos/ejemplos_invalidos/{file}"));
        let r = call(&mut conn, i as u64, "execute_action", args);
        assert_eq!(error_code(&r), INVALID_PARAMS, "{file}: {r}");
    }
    for (i, args) in [
        json!({ "x": 10, "y": 20 }),
        json!({ "limit": "10" }),
        json!({ "include_values": "yes" }),
        json!({ "query": null }),
        json!("observe"),
    ]
    .into_iter()
    .enumerate()
    {
        let r = call(&mut conn, 100 + i as u64, "observe_screen", args.clone());
        assert_eq!(error_code(&r), INVALID_PARAMS, "{args}");
    }
    // Invalid UTF-8 is a parse error.
    let line = conn
        .handle_message(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"\xff\"}")
        .unwrap();
    let r: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(error_code(&r), PARSE_ERROR);
}
