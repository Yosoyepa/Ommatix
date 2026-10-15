//! SEC-07 (chapter 08 §8.4): the engine deletes its state and cache when the session ends,
//! however it ends, and it stays usable afterwards without issuing an identifier twice.

mod common;

use std::io;
use std::pin::Pin;
use std::sync::PoisonError;
use std::task::{Context, Poll};

use common::{basic_scene, modern_meta, server_with};
use ommatix_adapter_mcp::McpConfig;
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
