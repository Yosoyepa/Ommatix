//! The compiled `ommatix` binary, serving MCP on stdio over the simulated desktop (never
//! the screen, never input): what one session writes to stdout, to stderr and to disk.
//! IT-06 (telemetry is redacted) and SEC-04 (nothing leaves by default) at the process
//! boundary, and SEC-06 (a tampered curated-icon file stops the server before it serves).
//!
//! Rule 5 of AGENTS.md: stdout belongs to MCP and every diagnostic goes to stderr. The
//! process also owns its disk behaviour: serving writes no file anywhere.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use serde_json::{Value, json};

const BINARY: &str = env!("CARGO_BIN_EXE_ommatix");

/// The strings of the simulated desktop's screen and the agent's typed text.
const SCREEN_TEXT: [&str; 3] = ["Name", "Cancel", "Save"];
const TYPED: &str = "TYPED-SECRET-2718";

/// A fresh directory used as the working directory and as the temporary directory of the
/// child, so that anything it writes shows up.
struct Sandbox(PathBuf);

impl Sandbox {
    fn new(tag: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("ommatix-process-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("cwd")).unwrap();
        std::fs::create_dir_all(dir.join("tmp")).unwrap();
        Self(dir)
    }

    fn cwd(&self) -> PathBuf {
        self.0.join("cwd")
    }

    fn tmp(&self) -> PathBuf {
        self.0.join("tmp")
    }

    /// Runs the binary with `args` and `input` on stdin, to the end of stdin. A binary that
    /// exits before reading all of stdin (a startup refusal) closes the pipe: that is its
    /// answer, not a harness failure, so a broken pipe while writing is ignored.
    fn run(&self, args: &[&str], input: &str) -> Output {
        let mut child = Command::new(BINARY)
            .args(args)
            .current_dir(self.cwd())
            .env("TEMP", self.tmp())
            .env("TMP", self.tmp())
            .env("TMPDIR", self.tmp())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("the binary starts");
        let written = child
            .stdin
            .take()
            .expect("stdin")
            .write_all(input.as_bytes());
        if let Err(error) = written {
            assert_eq!(
                error.kind(),
                std::io::ErrorKind::BrokenPipe,
                "the request is written: {error}"
            );
        }
        child.wait_with_output().expect("the binary ends")
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn entries(dir: &Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect()
}

fn meta() -> Value {
    json!({
        "io.modelcontextprotocol/protocolVersion": "2026-07-28",
        "io.modelcontextprotocol/clientCapabilities": {}
    })
}

fn message(id: u64, method: &str, mut params: Value) -> String {
    params["_meta"] = meta();
    json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }).to_string() + "\n"
}

fn tool(id: u64, name: &str, arguments: &Value) -> String {
    message(
        id,
        "tools/call",
        json!({ "name": name, "arguments": arguments }),
    )
}

/// A whole session over the simulated desktop: discovery, observation with values asked
/// for, a dry-run `type`, its replay, a conflicting reuse of the id, a live attempt the
/// default policy denies, metrics, and two malformed calls quoting the typed text.
fn session() -> (String, usize) {
    // The simulated desktop is deterministic: the first snapshot is `s1`, epoch 1, and the
    // field "Name" is `e1` at revision 1.
    let click = |id: u64, request_id: &str, extra: Value| {
        let mut args = json!({
            "request_id": request_id, "session_id": "process", "snapshot_id": "s1",
            "geometry_epoch": 1, "target_id": "e1", "expected_revision": 1
        });
        for (k, v) in extra.as_object().unwrap() {
            args[k] = v.clone();
        }
        tool(id, "execute_action", &args)
    };
    let requests = [
        message(1, "server/discover", json!({})),
        message(2, "tools/list", json!({})),
        tool(3, "observe_screen", &json!({ "include_values": true })),
        click(4, "r1", json!({ "action": "type", "text": TYPED })),
        click(5, "r1", json!({ "action": "type", "text": TYPED })),
        click(6, "r1", json!({ "action": "type", "text": "another text" })),
        click(7, "r2", json!({ "action": "click", "dry_run": false })),
        tool(8, "get_engine_metrics", &json!({})),
        tool(9, "execute_action", &json!({ "action": TYPED })),
        tool(10, TYPED, &json!({})),
    ];
    let count = requests.len();
    (requests.concat(), count)
}

fn lines(bytes: &[u8]) -> Vec<String> {
    String::from_utf8(bytes.to_vec())
        .expect("UTF-8")
        .lines()
        .map(str::to_owned)
        .collect()
}

/// IT-06 and SEC-04 at the process boundary: stdout carries the replies and only them, one
/// JSON-RPC message per request in order; stderr carries only the server's own `ommatix:`
/// lines and nothing the screen showed or the agent typed; and serving writes no file in
/// the working directory or the temporary directory.
#[test]
fn it_06_and_sec_04_a_session_writes_replies_to_stdout_diagnostics_to_stderr_and_nothing_to_disk() {
    let sandbox = Sandbox::new("session");
    let (input, count) = session();
    let output = sandbox.run(&["serve", "--session-id", "process"], &input);
    assert!(output.status.success(), "{:?}", output.status);

    let replies = lines(&output.stdout);
    assert_eq!(replies.len(), count, "one reply per request, nothing else");
    for (i, line) in replies.iter().enumerate() {
        let reply: Value = serde_json::from_str(line)
            .unwrap_or_else(|e| panic!("stdout line {i} is not JSON-RPC: {e}: {line}"));
        assert_eq!(reply["jsonrpc"], "2.0");
        assert_eq!(reply["id"], (i + 1) as u64);
        assert!(
            !line.contains(TYPED),
            "typed text echoed on stdout (reply {})",
            i + 1
        );
    }
    // What the session did, so that the checks above are about something.
    let reply = |n: usize| -> Value { serde_json::from_str(&replies[n - 1]).unwrap() };
    assert_eq!(reply(3)["result"]["isError"], false);
    assert_eq!(
        reply(4)["result"]["structuredContent"]["status"],
        "simulated"
    );
    assert_eq!(
        reply(5)["result"]["structuredContent"],
        reply(4)["result"]["structuredContent"]
    );
    assert_eq!(
        reply(6)["result"]["structuredContent"]["error"]["code"],
        "REQUEST_CONFLICT"
    );
    assert_eq!(
        reply(7)["result"]["structuredContent"]["error"]["code"],
        "POLICY_DENIED"
    );
    assert!(reply(9)["error"].is_object() && reply(10)["error"].is_object());
    // The default policy redacts values although the agent asked for them.
    assert!(
        reply(3)["result"]["structuredContent"]["elements"][0]
            .get("text")
            .is_none()
    );

    let stderr = String::from_utf8(output.stderr).expect("UTF-8");
    assert!(!stderr.trim().is_empty(), "the startup banner");
    for line in stderr.lines() {
        assert!(
            line.starts_with("ommatix:"),
            "an unstructured stderr line: {line:?}"
        );
    }
    for secret in SCREEN_TEXT.iter().chain(&[TYPED, "another text"]) {
        assert!(!stderr.contains(secret), "{secret:?} in stderr:\n{stderr}");
    }
    // SEC-07: the session ends after its state was deleted, and says so.
    assert!(
        stderr
            .lines()
            .last()
            .is_some_and(|l| l.contains("session ended") && l.contains("were deleted")),
        "{stderr}"
    );

    assert!(
        entries(&sandbox.cwd()).is_empty(),
        "{:?}",
        entries(&sandbox.cwd())
    );
    assert!(
        entries(&sandbox.tmp()).is_empty(),
        "{:?}",
        entries(&sandbox.tmp())
    );
}

/// IT-06: `demo --timings` puts one record per observation on stderr: whole microseconds,
/// the plan, and a signature per element, never a label or any other text of the screen;
/// the labels it prints are the demo's own report, on stdout.
#[test]
fn it_06_demo_timings_on_stderr_are_numbers_only() {
    let sandbox = Sandbox::new("demo");
    let output = sandbox.run(&["demo", "--session-id", "demo", "--timings"], "");
    assert!(output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    let records: Vec<&str> = stderr
        .lines()
        .filter_map(|l| l.strip_prefix("ommatix-observation "))
        .collect();
    assert!(records.len() >= 2, "{stderr}");
    for line in records {
        let record: serde_json::Value = serde_json::from_str(line).expect("a JSON record");
        assert_eq!(record["schema"], "ommatix.observation/1", "{line}");
        for stage in [
            "capture",
            "plan",
            "detect",
            "recognize",
            "accessibility",
            "fuse",
            "track",
            "commit",
            "response_build",
            "total",
        ] {
            assert!(
                record["timings_us"][stage].is_u64(),
                "{stage} missing or not whole microseconds in {line:?}"
            );
        }
        assert!(
            ["full", "regions", "idle"].contains(&record["plan"]["work"].as_str().unwrap_or("")),
            "{line:?}"
        );
        for signature in record["signatures"].as_array().expect("signatures") {
            let text = signature.as_str().expect("a signature");
            assert_eq!(text.len(), 16, "{line:?}");
            assert!(text.bytes().all(|b| b.is_ascii_hexdigit()), "{line:?}");
        }
        for secret in SCREEN_TEXT {
            assert!(!line.contains(secret), "{line:?}");
        }
    }
    for secret in SCREEN_TEXT {
        assert!(!stderr.contains(secret), "{secret:?} in stderr:\n{stderr}");
    }
    assert!(entries(&sandbox.cwd()).is_empty() && entries(&sandbox.tmp()).is_empty());
}

/// SEC-06 at the process boundary: a curated-icon file that no longer matches the digest
/// the operator pinned stops `serve` before it answers anything: non-zero exit, nothing
/// on stdout, the reason on stderr.
#[test]
fn sec_06_a_tampered_prototypes_file_stops_serve_before_it_serves() {
    let sandbox = Sandbox::new("poisoned");
    let grid = [0, 90, 90, 0, 0, 90, 90, 0, 0].repeat(8);
    let reviewed = format!(
        "{}\n",
        json!({ "label": "Save", "context": "notepad.exe", "grid": grid })
    );
    let digest = {
        // The pin of the reviewed file, as the operator would compute it.
        use sha2::{Digest, Sha256};
        use std::fmt::Write as _;
        Sha256::digest(reviewed.as_bytes())
            .iter()
            .fold(String::new(), |mut hex, b| {
                let _ = write!(hex, "{b:02x}");
                hex
            })
    };
    let write_config = |dir: &Path| {
        std::fs::write(
            dir.join("ommatix.toml"),
            format!(
                "[cache]\nprototypes = \"icons.jsonl\"\nprototypes_sha256 = \"{digest}\"\n\
                 context = \"notepad.exe\"\ngradient_tau = 3\nmax_distance = 0.1\nmin_margin = 0.1\n"
            ),
        )
        .unwrap();
    };
    let dir = sandbox.cwd();
    write_config(&dir);
    let config = dir.join("ommatix.toml");
    let config = config.to_str().unwrap();
    let request = tool(1, "get_engine_metrics", &json!({}));

    // Reviewed: serves.
    std::fs::write(dir.join("icons.jsonl"), &reviewed).unwrap();
    let served = sandbox.run(&["serve", "--config", config], &request);
    assert!(served.status.success());
    assert_eq!(lines(&served.stdout).len(), 1);

    // An extra prototype appended after the review: refused.
    let poisoned = format!(
        "{reviewed}{}\n",
        json!({ "label": "Delete everything", "context": "notepad.exe", "grid": grid })
    );
    std::fs::write(dir.join("icons.jsonl"), poisoned).unwrap();
    let refused = sandbox.run(&["serve", "--config", config], &request);
    assert!(!refused.status.success());
    assert!(refused.stdout.is_empty(), "nothing is served");
    let stderr = String::from_utf8(refused.stderr).unwrap();
    assert!(stderr.contains("SHA-256 mismatch"), "{stderr}");
    assert!(!stderr.contains("Delete everything"), "{stderr}");
}

/// The harness itself: `serve` refusing to start (here, a configuration file that does not
/// exist) exits without reading stdin, and a request larger than any pipe buffer then meets
/// a closed pipe. The refusal must be reported as the binary's exit, not as a panic of the
/// harness (the race that failed `sec_06_a_tampered_prototypes_file_stops_serve_before_it_serves`
/// on CI).
#[test]
fn a_refusal_before_reading_stdin_is_reported_as_the_binarys_exit() {
    let sandbox = Sandbox::new("closed-pipe");
    let missing = sandbox.cwd().join("missing.toml");
    let request = "x".repeat(4 * 1024 * 1024);
    let refused = sandbox.run(&["serve", "--config", missing.to_str().unwrap()], &request);
    assert!(!refused.status.success());
    assert_eq!(lines(&refused.stdout).len(), 0);
}
