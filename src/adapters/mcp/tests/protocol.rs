//! End-to-end MCP conversation against the real engine, synthetic source and
//! contrast detector: the same wiring the binary uses, minus stdio.

use ommatix_adapter_capture::{ScriptedSource, demo_script};
use ommatix_adapter_input::DryRunInput;
use ommatix_adapter_mcp::{McpServer, PROTOCOL_VERSION};
use ommatix_adapter_vision::ContrastBlobDetector;
use ommatix_application::{ActuationPolicy, ActuationService, PerceptionConfig, PerceptionEngine};
use serde_json::{Value, json};

fn server() -> McpServer {
    let engine = PerceptionEngine::new(
        Box::new(ScriptedSource::new(demo_script())),
        Box::new(ContrastBlobDetector::default()),
        PerceptionConfig::default(),
    )
    .unwrap();
    let actuation =
        ActuationService::new(Box::new(DryRunInput::default()), ActuationPolicy::default());
    McpServer::new(engine, actuation)
}

fn rpc(server: &mut McpServer, id: u64, method: &str, params: &Value) -> Value {
    let msg = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
    server
        .handle_line(&msg.to_string())
        .expect("requests get a response")
}

fn call(server: &mut McpServer, id: u64, tool: &str, args: &Value) -> Value {
    let res = rpc(
        server,
        id,
        "tools/call",
        &json!({ "name": tool, "arguments": args }),
    );
    assert_eq!(res["result"]["isError"], false, "{res}");
    res["result"]["structuredContent"].clone()
}

#[test]
fn handshake_and_tool_listing() {
    let mut s = server();
    let init = rpc(
        &mut s,
        1,
        "initialize",
        &json!({ "protocolVersion": PROTOCOL_VERSION }),
    );
    assert_eq!(init["result"]["protocolVersion"], PROTOCOL_VERSION);
    assert_eq!(init["result"]["serverInfo"]["name"], "ommatix");

    let note = json!({ "jsonrpc": "2.0", "method": "notifications/initialized" });
    assert!(s.handle_line(&note.to_string()).is_none());

    let tools = rpc(&mut s, 2, "tools/list", &json!({}));
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
    let mut s = server();

    let first = call(&mut s, 1, "perceive", &json!({}));
    assert_eq!(first["dirty_cells"], first["total_cells"]);
    assert!(first["element_count"].as_u64().unwrap() > 0);

    // "Save" button at (500,300,80,32): the agent asks what is under its centre.
    let hit = call(&mut s, 2, "element_at", &json!({ "x": 540, "y": 316 }));
    assert_eq!(
        hit["elements"][0]["bounds"],
        json!({ "x": 500, "y": 300, "w": 80, "h": 32 })
    );
    assert_eq!(hit["elements"][0]["center"], json!({ "x": 540, "y": 316 }));
    let save_id = hit["elements"][0]["id"].clone();

    let idle = call(&mut s, 3, "perceive", &json!({}));
    assert_eq!(idle["dirty_cells"], 0, "static screen re-analyses nothing");

    let typed = call(&mut s, 4, "perceive", &json!({}));
    assert!(typed["dirty_cells"].as_u64().unwrap() < typed["total_cells"].as_u64().unwrap() / 10);

    let modal = call(&mut s, 5, "perceive", &json!({}));
    assert!(!modal["added"].as_array().unwrap().is_empty(), "{modal}");
    let after = call(&mut s, 6, "element_at", &json!({ "x": 540, "y": 316 }));
    assert_eq!(
        after["elements"][0]["id"], save_id,
        "untouched button keeps its id"
    );

    let exhausted = rpc(&mut s, 7, "tools/call", &json!({ "name": "perceive" }));
    assert_eq!(exhausted["result"]["isError"], true);
}

#[test]
fn protocol_errors() {
    let mut s = server();
    assert_eq!(s.handle_line("{not json").unwrap()["error"]["code"], -32700);
    assert_eq!(
        rpc(&mut s, 1, "resources/list", &json!({}))["error"]["code"],
        -32601
    );
    assert_eq!(
        rpc(&mut s, 2, "tools/call", &json!({ "name": "nope" }))["error"]["code"],
        -32602
    );
    let bad = rpc(
        &mut s,
        3,
        "tools/call",
        &json!({ "name": "element_at", "arguments": { "x": 1 } }),
    );
    assert_eq!(bad["result"]["isError"], true);
}
