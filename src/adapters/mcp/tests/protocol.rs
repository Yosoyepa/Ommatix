//! End-to-end MCP conversations against the real engine and detector, with simulated or
//! replayed screens and a virtual clock: the binary's wiring, minus stdio.

use std::sync::Arc;

use ommatix_adapter_capture::ScriptedSource;
use ommatix_adapter_input::{ActionLog, DryRunInput};
use ommatix_adapter_mcp::{McpServer, PROTOCOL_VERSION};
use ommatix_adapter_sim::{SimulatedDesktop, Theme, layout, scripted_session};
use ommatix_adapter_vision::FlatRegionDetector;
use ommatix_application::{
    ActuationPolicy, ActuationService, AgentSession, FrameSource, InputController, ManualClock,
    Observer, PerceptionConfig, PerceptionEngine, SettlePolicy,
};
use serde_json::{Value, json};

fn server(source: Box<dyn FrameSource>, input: Box<dyn InputController>) -> McpServer {
    let clock = Arc::new(ManualClock::new());
    let perception = PerceptionEngine::new(
        source,
        Box::new(FlatRegionDetector::default()),
        clock.clone(),
        PerceptionConfig::default(),
    )
    .unwrap();
    let observer = Observer::new(clock, SettlePolicy::default()).unwrap();
    let actuation = ActuationService::new(input, ActuationPolicy::default());
    McpServer::new(AgentSession::new(perception, observer, actuation))
}

/// The recorded session (first `frames` frames) with dry-run input.
fn replay(frames: usize) -> (McpServer, ActionLog) {
    let mut script = scripted_session(Theme::Light);
    script.truncate(frames);
    let log = ActionLog::default();
    let s = server(
        Box::new(ScriptedSource::new(script)),
        Box::new(DryRunInput::new(log.clone())),
    );
    (s, log)
}

fn desktop() -> (McpServer, SimulatedDesktop) {
    let sim = SimulatedDesktop::new();
    (server(Box::new(sim.screen()), Box::new(sim.input())), sim)
}

fn send(server: &mut McpServer, raw: &str) -> Option<Value> {
    server.handle_message(raw.as_bytes())
}

fn rpc(server: &mut McpServer, id: u64, method: &str, params: &Value) -> Value {
    let msg = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
    send(server, &msg.to_string()).expect("requests get a response")
}

fn call_raw(server: &mut McpServer, tool: &str, args: &Value) -> Value {
    rpc(
        server,
        1,
        "tools/call",
        &json!({ "name": tool, "arguments": args }),
    )
}

fn call(server: &mut McpServer, tool: &str, args: &Value) -> Value {
    let res = call_raw(server, tool, args);
    assert_eq!(res["result"]["isError"], false, "{res}");
    res["result"]["structuredContent"].clone()
}

fn tool_error(server: &mut McpServer, tool: &str, args: &Value) -> String {
    let res = call_raw(server, tool, args);
    assert_eq!(res["result"]["isError"], true, "{res}");
    res["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn rect(r: ommatix_domain::Rect) -> Value {
    json!({ "x": r.x, "y": r.y, "w": r.w, "h": r.h })
}

#[test]
fn handshake_and_tool_listing() {
    let (mut s, _) = replay(4);
    let init = rpc(
        &mut s,
        1,
        "initialize",
        &json!({ "protocolVersion": PROTOCOL_VERSION }),
    );
    assert_eq!(init["result"]["protocolVersion"], PROTOCOL_VERSION);
    assert_eq!(init["result"]["serverInfo"]["name"], "ommatix");
    let old = rpc(
        &mut s,
        2,
        "initialize",
        &json!({ "protocolVersion": "2024-11-05" }),
    );
    assert_eq!(old["result"]["protocolVersion"], "2024-11-05");

    let note = json!({ "jsonrpc": "2.0", "method": "notifications/initialized" });
    assert!(send(&mut s, &note.to_string()).is_none());

    let tools = rpc(&mut s, 3, "tools/list", &json!({}));
    let names: Vec<&str> = tools["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "perceive",
            "list_elements",
            "element_at",
            "elements_in_rect",
            "click",
            "type_text",
            "press_key"
        ]
    );
}

#[test]
fn incremental_perception_through_mcp() {
    let (mut s, _) = replay(4);

    let first = rpc(&mut s, 1, "tools/call", &json!({ "name": "perceive" }));
    assert_eq!(
        first["result"]["_meta"]["ommatix/timings_us"]["total"], 0,
        "virtual clock"
    );
    let first = &first["result"]["structuredContent"];
    assert_eq!(first["dirty_cells"], first["total_cells"]);
    assert_eq!(first["geometry_changed"], true);
    assert_eq!(first["element_count"], 4, "{first}");

    let hit = call(&mut s, "element_at", &json!({ "x": 540, "y": 316 }));
    assert_eq!(hit["elements"][0]["bounds"], rect(layout::SAVE_BUTTON));
    assert_eq!(hit["elements"][0]["center"], json!({ "x": 540, "y": 316 }));
    assert_eq!(hit["elements"][0]["kind"], "blob");
    let save_id = hit["elements"][0]["id"].clone();

    let idle = call(&mut s, "perceive", &json!({}));
    assert_eq!(
        idle["dirty_cells"], 0,
        "a static screen re-analyses nothing"
    );
    assert_eq!(idle["analysed_px"], 0);

    let typed = call(&mut s, "perceive", &json!({}));
    assert!(typed["dirty_cells"].as_u64().unwrap() < 4, "{typed}");
    assert_eq!(typed["added"][0]["bounds"], rect(layout::text_bounds(5)));

    let dialog = call(&mut s, "perceive", &json!({}));
    let added: Vec<&Value> = dialog["added"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| &e["bounds"])
        .collect();
    assert_eq!(
        added,
        [&rect(layout::DIALOG), &rect(layout::OK_BUTTON)],
        "{dialog}"
    );
    let after = call(&mut s, "element_at", &json!({ "x": 540, "y": 316 }));
    assert_eq!(
        after["elements"][0]["id"], save_id,
        "untouched button keeps its id"
    );

    let exhausted = tool_error(&mut s, "perceive", &json!({}));
    assert!(exhausted.contains("no more frames"), "{exhausted}");
}

#[test]
fn acting_waits_for_the_screen_to_settle() {
    let (mut s, sim) = desktop();
    call(&mut s, "perceive", &json!({}));
    let save =
        call(&mut s, "element_at", &json!({ "x": 540, "y": 316 }))["elements"][0]["id"].clone();

    let res = call_raw(&mut s, "click", &json!({ "element_id": save }));
    assert!(
        res["result"]["_meta"]["ommatix/elapsed_us"]
            .as_u64()
            .unwrap()
            >= 100_000
    );
    let clicked = &res["result"]["structuredContent"];
    assert_eq!(
        clicked["performed"]["action"],
        json!({ "type": "click", "at": { "x": 540, "y": 316 }, "button": "left" })
    );
    assert_eq!(clicked["performed"]["element_id"], save);
    assert_eq!(clicked["performed"]["live"], false);
    let seen = &clicked["observation"];
    assert_eq!(seen["settled"], true, "{seen}");
    assert_eq!(seen["added"].as_array().unwrap().len(), 2, "dialog and OK");
    assert!(sim.state().dialog_open);
}

#[test]
fn a_performed_action_is_reported_even_if_watching_it_fails() {
    let (mut s, log) = replay(1);
    call(&mut s, "perceive", &json!({}));
    let typed = call(&mut s, "type_text", &json!({ "text": "hello" }));
    assert_eq!(
        typed["performed"]["action"],
        json!({ "type": "type_text", "text": "hello" })
    );
    assert!(
        typed["observe_error"]
            .as_str()
            .unwrap()
            .contains("no more frames")
    );
    assert_eq!(log.snapshot().len(), 1, "the agent must not type it again");
}

#[test]
fn invalid_actions_are_tool_errors_and_never_reach_the_desktop() {
    let (mut s, log) = replay(4);
    assert!(tool_error(&mut s, "click", &json!({ "x": 1, "y": 1 })).contains("perceive first"));
    call(&mut s, "perceive", &json!({}));
    let cases = [
        ("click", json!({ "x": 640, "y": 1 }), "outside the screen"),
        ("click", json!({ "element_id": "e999" }), "unknown element"),
        (
            "click",
            json!({ "element_id": "e-1" }),
            "invalid element id",
        ),
        ("click", json!({ "x": 1 }), "either element_id"),
        (
            "click",
            json!({ "x": 1, "y": 1, "button": "thumb" }),
            "unknown mouse button",
        ),
        ("type_text", json!({ "text": "a\nb" }), "control character"),
        ("press_key", json!({ "key": "F13" }), "unknown key"),
        (
            "press_key",
            json!({ "key": "enter", "x": 1 }),
            "invalid arguments",
        ),
    ];
    for (tool, args, fragment) in cases {
        let message = tool_error(&mut s, tool, &args);
        assert!(message.contains(fragment), "{tool} {args}: {message}");
    }
    assert_eq!(log.snapshot().len(), 0);
}

#[test]
fn protocol_errors_use_json_rpc_codes() {
    let (mut s, _) = replay(4);
    let code = |s: &mut McpServer, raw: &str| send(s, raw).unwrap()["error"]["code"].clone();
    assert_eq!(code(&mut s, "{not json"), -32700);
    assert_eq!(code(&mut s, "42"), -32600);
    assert_eq!(code(&mut s, "[]"), -32600);
    assert_eq!(
        code(&mut s, r#"{"jsonrpc":"1.0","id":1,"method":"ping"}"#),
        -32600
    );
    assert_eq!(
        code(&mut s, r#"{"jsonrpc":"2.0","id":{},"method":"ping"}"#),
        -32600
    );
    assert_eq!(
        code(
            &mut s,
            r#"{"jsonrpc":"2.0","id":1,"method":"resources/list"}"#
        ),
        -32601
    );
    assert_eq!(
        code(
            &mut s,
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"nope"}}"#
        ),
        -32602
    );
    assert!(
        send(&mut s, r#"{"jsonrpc":"2.0","id":5,"result":{}}"#).is_none(),
        "client responses are ignored"
    );
    let bad_args = tool_error(&mut s, "element_at", &json!({ "x": 1 }));
    assert!(bad_args.contains("invalid arguments"), "{bad_args}");
}

#[test]
fn batches_get_one_response_per_request() {
    let (mut s, _) = replay(4);
    let batch = r#"[
        {"jsonrpc":"2.0","id":1,"method":"ping"},
        {"jsonrpc":"2.0","method":"notifications/initialized"},
        7,
        {"jsonrpc":"2.0","id":2,"method":"nope"}
    ]"#;
    let responses = send(&mut s, batch).unwrap();
    let ids_and_codes: Vec<(Value, Value)> = responses
        .as_array()
        .unwrap()
        .iter()
        .map(|r| (r["id"].clone(), r["error"]["code"].clone()))
        .collect();
    assert_eq!(
        ids_and_codes,
        [
            (json!(1), Value::Null),
            (Value::Null, json!(-32600)),
            (json!(2), json!(-32601))
        ]
    );
    let only_notes = r#"[{"jsonrpc":"2.0","method":"notifications/initialized"}]"#;
    assert!(send(&mut s, only_notes).is_none());
}

#[test]
fn serve_survives_invalid_utf8_and_blank_lines() {
    let (mut s, _) = replay(4);
    let input: &[u8] = b"\xff\xfe\n\n   \n{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\r\n";
    let mut out = Vec::new();
    s.serve(input, &mut out).unwrap();
    let lines: Vec<Value> = String::from_utf8(out)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0]["error"]["code"], -32700);
    assert_eq!(lines[1], json!({ "jsonrpc": "2.0", "id": 1, "result": {} }));
}

#[test]
fn hostile_rectangles_are_answered_without_overflow_or_stalls() {
    let (mut s, _) = replay(4);
    call(&mut s, "perceive", &json!({}));
    let max = u32::MAX;
    let everything = call(
        &mut s,
        "elements_in_rect",
        &json!({ "x": 0, "y": 0, "w": max, "h": max }),
    );
    assert_eq!(everything["elements"].as_array().unwrap().len(), 4);
    let off_screen = call(
        &mut s,
        "elements_in_rect",
        &json!({ "x": max - 1, "y": max - 1, "w": 1, "h": 1 }),
    );
    assert_eq!(off_screen["elements"].as_array().unwrap().len(), 0);
    for bad in [
        json!({ "x": max, "y": 0, "w": 1, "h": 1 }),
        json!({ "x": 0, "y": 0, "w": 0, "h": 1 }),
        json!({ "x": -1, "y": 0, "w": 1, "h": 1 }),
        json!({ "x": 0, "y": 0, "w": 1.5, "h": 1 }),
    ] {
        tool_error(&mut s, "elements_in_rect", &bad);
    }
    let corner = call(&mut s, "element_at", &json!({ "x": max, "y": max }));
    assert_eq!(corner["elements"].as_array().unwrap().len(), 0);
}

/// Same inputs, same bytes: no clock, hash-map order or id reuse leaks into answers.
#[test]
fn conversations_are_reproducible_byte_for_byte() {
    fn transcript() -> Vec<String> {
        let (mut s, _) = desktop();
        let steps = [
            ("perceive", json!({})),
            ("click", json!({ "x": 230, "y": 135 })),
            ("type_text", json!({ "text": "Ommatix" })),
            ("press_key", json!({ "key": "backspace" })),
            ("click", json!({ "element_id": "e4" })),
            ("list_elements", json!({})),
            ("press_key", json!({ "key": "escape" })),
            (
                "elements_in_rect",
                json!({ "x": 0, "y": 0, "w": 640, "h": 400 }),
            ),
        ];
        steps
            .iter()
            .map(|(tool, args)| call_raw(&mut s, tool, args).to_string())
            .collect()
    }
    assert_eq!(transcript(), transcript());
}
