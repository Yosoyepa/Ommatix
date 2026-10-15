//! SEC-07 (chapter 08 §8.4): the engine deletes its state and cache when the session ends,
//! however it ends, and it stays usable afterwards without issuing an identifier twice.

mod common;

use std::io;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, PoisonError};
use std::task::{Context, Poll};

use common::{basic_scene, call_ok, engine_with_store, modern_meta, server_with};
use ommatix_adapter_mcp::{Connection, McpConfig, McpServer};
use ommatix_application::{LocalPolicy, PolicyStore};
use serde_json::{Value, json};
use tokio::io::{AsyncRead, AsyncReadExt, ReadBuf};

/// One `observe_screen` request, one JSON-RPC line.
fn observe_line(id: u64) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "tools/call",
        "params": { "name": "observe_screen", "arguments": {}, "_meta": modern_meta() }
    })
    .to_string()
        + "\n"
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("runtime")
}

/// The snapshot identifier of the observation in `output`, a line of the reply.
fn snapshot_of(output: &[u8]) -> String {
    let text = String::from_utf8(output.to_vec()).expect("UTF-8 output");
    let reply: Value = serde_json::from_str(text.lines().next().expect("a reply")).unwrap();
    reply["result"]["structuredContent"]["snapshot_id"]
        .as_str()
        .expect("a snapshot identifier")
        .to_owned()
}

/// A stream that fails once the bytes before it are read: the client went away.
struct Broken;

impl AsyncRead for Broken {
    fn poll_read(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        _: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Poll::Ready(Err(io::Error::other("the client went away")))
    }
}

#[test]
fn sec_07_the_end_of_the_session_deletes_the_published_snapshot() {
    let server = server_with(basic_scene(), McpConfig::default());
    let engine = server.engine();
    let mut output = Vec::new();
    runtime()
        .block_on(
            server
                .clone()
                .serve(observe_line(1).as_bytes(), &mut output),
        )
        .expect("in-memory I/O does not fail");
    assert_eq!(snapshot_of(&output), "s1");

    let engine = engine.lock().unwrap_or_else(PoisonError::into_inner);
    assert!(engine.last_timings().is_none());
    assert!(engine.accessibility_complete().is_none());
    assert_eq!(engine.engine_metrics().pending_regions, 0);
}

#[test]
fn sec_07_a_session_after_a_purge_observes_again_with_new_identifiers() {
    let server = server_with(basic_scene(), McpConfig::default());
    let mut first = Vec::new();
    runtime()
        .block_on(server.clone().serve(observe_line(1).as_bytes(), &mut first))
        .expect("in-memory I/O does not fail");

    let mut second = Vec::new();
    runtime()
        .block_on(
            server
                .clone()
                .serve(observe_line(2).as_bytes(), &mut second),
        )
        .expect("in-memory I/O does not fail");
    assert_eq!(snapshot_of(&first), "s1");
    assert_eq!(snapshot_of(&second), "s2");
}

#[test]
fn sec_07_a_broken_stream_ends_the_session_and_deletes_the_state_too() {
    let server = server_with(basic_scene(), McpConfig::default());
    let engine = server.engine();
    let input = observe_line(1).into_bytes();
    let reader = (&input[..]).chain(Broken);
    let mut output = Vec::new();
    let outcome = runtime().block_on(server.clone().serve(reader, &mut output));
    assert!(outcome.is_err());
    assert_eq!(snapshot_of(&output), "s1");

    let engine = engine.lock().unwrap_or_else(PoisonError::into_inner);
    assert!(engine.last_timings().is_none());
}

/// The operator's policy and purge file, as these tests make them: a policy that allows
/// nothing, and a purge file whose creation is a new instance, honoured once. `ConfigPolicy`
/// keys the file on its modification time and size; the tests count creations instead.
#[derive(Debug, Default)]
struct PurgeFile {
    created: AtomicU64,
    honoured: AtomicU64,
    purges: AtomicU64,
    /// The operator creates the file on the next policy read: the operator acting while the
    /// engine works on a request.
    create_on_read: AtomicBool,
}

impl PurgeFile {
    /// The operator creates the purge file.
    fn create(&self) {
        self.created.fetch_add(1, Ordering::SeqCst);
    }

    fn purges(&self) -> u64 {
        self.purges.load(Ordering::SeqCst)
    }
}

impl PolicyStore for PurgeFile {
    fn current(&self) -> LocalPolicy {
        if self.create_on_read.swap(false, Ordering::SeqCst) {
            self.create();
        }
        LocalPolicy::default()
    }

    fn purge_requested(&self) -> bool {
        self.created.load(Ordering::SeqCst) != self.honoured.load(Ordering::SeqCst)
    }

    fn purge_done(&self) {
        self.honoured
            .store(self.created.load(Ordering::SeqCst), Ordering::SeqCst);
        self.purges.fetch_add(1, Ordering::SeqCst);
    }
}

/// A connection to an engine that observes `basic_scene()` under the given purge file.
fn connection_with(purge: &Arc<PurgeFile>) -> Connection {
    let engine = engine_with_store(basic_scene(), purge.clone());
    McpServer::new(engine, McpConfig::default()).connect()
}

/// A `tools/call` of `observe_screen`; returns the observation (`structuredContent`).
fn observe(conn: &mut Connection, id: u64) -> Value {
    call_ok(conn, id, "observe_screen", json!({}))["structuredContent"].clone()
}

/// A dry-run click on the element labelled `label` of `page`.
fn click_args(page: &Value, label: &str, request_id: &str) -> Value {
    let element = page["elements"]
        .as_array()
        .expect("elements")
        .iter()
        .find(|e| e["label"] == label)
        .unwrap_or_else(|| panic!("no element labelled {label:?}"));
    json!({
        "request_id": request_id,
        "session_id": page["session_id"],
        "snapshot_id": page["snapshot_id"],
        "geometry_epoch": page["geometry_epoch"],
        "target_id": element["target_id"],
        "expected_revision": element["revision"],
        "action": "click",
        "dry_run": true
    })
}

/// The `structuredContent` of a dry-run click on "Save" of `page`.
fn click_save(conn: &mut Connection, id: u64, page: &Value, request_id: &str) -> Value {
    let args = click_args(page, "Save", request_id);
    call_ok(conn, id, "execute_action", args)["structuredContent"].clone()
}

/// The `structuredContent` of a live click on "Save" of `page`. The engine reads the policy
/// for it at the gate, which the dry run never does.
fn live_click_save(conn: &mut Connection, id: u64, page: &Value, request_id: &str) -> Value {
    let mut args = click_args(page, "Save", request_id);
    args["dry_run"] = json!(false);
    call_ok(conn, id, "execute_action", args)["structuredContent"].clone()
}

#[test]
fn sec_07_an_operator_purge_request_is_honoured_before_the_next_request() {
    let purge = Arc::new(PurgeFile::default());
    let mut conn = connection_with(&purge);
    let page = observe(&mut conn, 1);
    purge.create();

    // The next request is the boundary: the engine purges before it handles this one.
    let refused = click_save(&mut conn, 2, &page, "r1");
    assert_eq!(refused["error"]["code"], "STALE_SNAPSHOT", "{refused}");
    assert_eq!(refused["input_delivered"], false);
    assert_eq!(purge.purges(), 1);
    let metrics = call_ok(&mut conn, 3, "get_engine_metrics", json!({}));
    assert_eq!(metrics["structuredContent"]["delivered_actions"], 0);
}

#[test]
fn sec_07_the_operator_purge_never_interrupts_a_request() {
    let purge = Arc::new(PurgeFile::default());
    let mut conn = connection_with(&purge);
    let page = observe(&mut conn, 1);

    // The operator creates the file while the engine reads the policy for this live action.
    // The default policy refuses it at the gate, so the request ends on its own snapshot.
    purge.create_on_read.store(true, Ordering::SeqCst);
    let during = live_click_save(&mut conn, 2, &page, "r1");
    assert_eq!(during["error"]["code"], "POLICY_DENIED", "{during}");
    assert_eq!(during["input_delivered"], false);
    assert_eq!(purge.purges(), 0, "nothing is deleted inside a request");

    // The next request is the boundary.
    let after = click_save(&mut conn, 3, &page, "r2");
    assert_eq!(after["error"]["code"], "STALE_SNAPSHOT", "{after}");
    assert_eq!(purge.purges(), 1);
}

#[test]
fn sec_07_without_an_operator_request_the_engine_keeps_its_state() {
    let purge = Arc::new(PurgeFile::default());
    let mut conn = connection_with(&purge);
    let page = observe(&mut conn, 1);
    call_ok(&mut conn, 2, "get_engine_metrics", json!({}));

    let kept = click_save(&mut conn, 3, &page, "r1");
    assert_eq!(kept["status"], "simulated", "{kept}");
    assert_eq!(purge.purges(), 0);
}

#[test]
fn sec_07_an_honoured_purge_file_is_not_honoured_again_and_a_new_one_is() {
    let purge = Arc::new(PurgeFile::default());
    let mut conn = connection_with(&purge);
    observe(&mut conn, 1);
    purge.create();
    call_ok(&mut conn, 2, "get_engine_metrics", json!({}));
    assert_eq!(purge.purges(), 1);

    let page = observe(&mut conn, 3);
    call_ok(&mut conn, 4, "get_engine_metrics", json!({}));
    call_ok(&mut conn, 5, "get_engine_metrics", json!({}));
    assert_eq!(purge.purges(), 1, "the same instance is not honoured again");
    let kept = click_save(&mut conn, 6, &page, "r1");
    assert_eq!(kept["status"], "simulated", "{kept}");

    purge.create();
    let refused = click_save(&mut conn, 7, &page, "r2");
    assert_eq!(refused["error"]["code"], "STALE_SNAPSHOT", "{refused}");
    assert_eq!(purge.purges(), 2);
}

#[test]
fn sec_07_ids_are_never_reused_after_an_operator_purge() {
    let purge = Arc::new(PurgeFile::default());
    let mut conn = connection_with(&purge);
    let before = observe(&mut conn, 1);
    purge.create();
    call_ok(&mut conn, 2, "get_engine_metrics", json!({}));

    let after = observe(&mut conn, 3);
    assert_ne!(after["snapshot_id"], before["snapshot_id"]);
    let old: Vec<&Value> = before["elements"]
        .as_array()
        .expect("elements")
        .iter()
        .map(|e| &e["target_id"])
        .collect();
    for element in after["elements"].as_array().expect("elements") {
        assert!(!old.contains(&&element["target_id"]), "{element}");
    }
}
