//! The compiled `ommatix` binary with the pipe transport switch (ADR-0043 and ADR-0040,
//! proposed): `serve` with `[mcp] transport = "pipe"` and one or more `attach` processes driven
//! through their standard streams, over the simulated desktop (never the screen, never input),
//! with real named pipes between the processes. And, in every build, the stdio path: the switch
//! off gives the session the committed bytes (`fixtures/stdio_session.golden`), the same with the
//! feature on and off.
//!
//! The pipe tests need a Windows build with `--features broker-pipe`; each names its pipe after
//! this test process, so tests run in parallel and never meet another engine.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

use serde_json::{Value, json};

const BINARY: &str = env!("CARGO_BIN_EXE_ommatix");

/// A fresh directory used as the working directory and as the temporary directory of the
/// children, so that anything they write shows up.
struct Sandbox(PathBuf);

impl Sandbox {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("ommatix-pipe-{tag}-{}", std::process::id()));
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

    fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(BINARY);
        command
            .args(args)
            .current_dir(self.cwd())
            .env("TEMP", self.tmp())
            .env("TMP", self.tmp())
            .env("TMPDIR", self.tmp())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command
    }

    /// Runs the binary with `args` and `input` on stdin, to the end of stdin (a broken pipe
    /// while writing is the binary's early exit, not a harness failure).
    fn run(&self, args: &[&str], input: &str) -> Output {
        let mut child = self.command(args).spawn().expect("the binary starts");
        let written = child
            .stdin
            .take()
            .expect("stdin")
            .write_all(input.as_bytes());
        if let Err(error) = written {
            assert_eq!(error.kind(), std::io::ErrorKind::BrokenPipe, "{error}");
        }
        child.wait_with_output().expect("the binary ends")
    }

    /// Every file under the sandbox, with its bytes.
    #[cfg(all(windows, feature = "broker-pipe"))]
    fn files(&self) -> Vec<(PathBuf, Vec<u8>)> {
        fn walk(dir: &std::path::Path, out: &mut Vec<(PathBuf, Vec<u8>)>) {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    walk(&path, out);
                } else {
                    let bytes = std::fs::read(&path).unwrap();
                    out.push((path, bytes));
                }
            }
        }
        let mut out = Vec::new();
        walk(&self.0, &mut out);
        out
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
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

// ---- The switch off: the stdio path, byte for byte (IT-33) ----

/// The session of `serve_process.rs`, without its malformed lines: discovery, an observation
/// with values asked for, a dry-run `type`, its replay, a conflicting reuse, a live attempt the
/// default policy denies, metrics, a paged observation and its next page.
fn stdio_session() -> String {
    let click = |id: u64, request_id: &str, extra: &Value| {
        let mut args = json!({
            "request_id": request_id, "session_id": "parity", "snapshot_id": "s1",
            "geometry_epoch": 1, "target_id": "e1", "expected_revision": 1
        });
        for (k, v) in extra.as_object().unwrap() {
            args[k] = v.clone();
        }
        tool(id, "execute_action", &args)
    };
    [
        message(1, "server/discover", json!({})),
        message(2, "tools/list", json!({})),
        tool(3, "observe_screen", &json!({ "include_values": true })),
        click(4, "r1", &json!({ "action": "type", "text": "parity text" })),
        click(5, "r1", &json!({ "action": "type", "text": "parity text" })),
        click(
            6,
            "r1",
            &json!({ "action": "type", "text": "another text" }),
        ),
        click(7, "r2", &json!({ "action": "click", "dry_run": false })),
        tool(8, "get_engine_metrics", &json!({})),
        tool(9, "observe_screen", &json!({ "limit": 1 })),
        tool(10, "observe_screen", &json!({ "limit": 1, "cursor": "c" })),
        tool(11, "execute_action", &json!({ "action": "parity text" })),
    ]
    .concat()
}

/// A reply with its run-dependent values set to zero: the stage timings of `_meta`, the
/// `age_ms` of a page, and the server's version (which every release changes).
fn normalised(line: &str) -> String {
    fn zero_numbers(value: &mut Value) {
        match value {
            Value::Number(_) => *value = json!(0),
            Value::Array(items) => items.iter_mut().for_each(zero_numbers),
            Value::Object(map) => map.values_mut().for_each(zero_numbers),
            _ => {}
        }
    }
    fn walk(value: &mut Value) {
        match value {
            Value::Object(map) => {
                for (key, inner) in map.iter_mut() {
                    match key.as_str() {
                        "io.ommatix/timings" | "io.ommatix/action_timings" | "age_ms" => {
                            zero_numbers(inner);
                        }
                        "io.modelcontextprotocol/serverInfo" => inner["version"] = json!("*"),
                        "text" if inner.is_string() => {
                            // The text block repeats `structuredContent` as JSON.
                            if let Ok(mut parsed) =
                                serde_json::from_str::<Value>(inner.as_str().unwrap_or(""))
                            {
                                walk(&mut parsed);
                                *inner = json!(parsed.to_string());
                            }
                        }
                        _ => walk(inner),
                    }
                }
            }
            Value::Array(items) => items.iter_mut().for_each(walk),
            _ => {}
        }
    }
    let mut value: Value = serde_json::from_str(line).expect("a JSON-RPC line");
    walk(&mut value);
    value.to_string()
}

/// IT-33: with the switch off, `serve` over stdio emits the committed bytes, run-dependent
/// numbers aside, on stdout and on stderr. This test runs in every build, so the build with
/// `broker-pipe` and the build without it are held to the same file.
#[test]
fn it_33_with_the_switch_off_serve_over_stdio_emits_the_committed_bytes() {
    let golden = include_str!("fixtures/stdio_session.golden").replace("\r\n", "\n");
    let sandbox = Sandbox::new("parity");
    let output = sandbox.run(&["serve", "--session-id", "parity"], &stdio_session());
    assert!(output.status.success(), "{:?}", output.status);
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    let mut actual = String::from("# stdout\n");
    for line in stdout.lines() {
        actual.push_str(&normalised(line));
        actual.push('\n');
    }
    actual.push_str("# stderr\n");
    actual.push_str(&stderr.replace("\r\n", "\n"));
    assert!(
        actual == golden,
        "the stdio session differs from fixtures/stdio_session.golden:\n{actual}"
    );
}

/// IT-33: a build without the feature (or off Windows) refuses `[mcp] transport = "pipe"` at
/// start: nothing on stdout, the reason on stderr, nothing written.
#[cfg(not(all(windows, feature = "broker-pipe")))]
#[test]
fn it_33_without_the_feature_the_pipe_switch_is_refused_before_anything_is_opened() {
    let sandbox = Sandbox::new("no-feature");
    let config = sandbox.0.join("ommatix.toml");
    std::fs::write(&config, "[mcp]\ntransport = \"pipe\"\n").unwrap();
    let request = tool(1, "get_engine_metrics", &json!({}));
    let refused = sandbox.run(&["serve", "--config", config.to_str().unwrap()], &request);
    assert!(!refused.status.success());
    assert!(refused.stdout.is_empty(), "nothing is served");
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert!(stderr.contains("--features broker-pipe"), "{stderr}");
    assert!(std::fs::read_dir(sandbox.cwd()).unwrap().next().is_none());
    assert!(std::fs::read_dir(sandbox.tmp()).unwrap().next().is_none());
}

#[cfg(all(windows, feature = "broker-pipe"))]
mod pipe {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::path::Path;
    use std::process::{Child, ChildStdin, ExitStatus};
    use std::sync::mpsc::{self, Receiver};
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::{Duration, Instant};

    use sha2::{Digest, Sha256};

    use super::*;

    /// How long a test waits for a line or an exit before it fails instead of hanging.
    const PATIENCE: Duration = Duration::from_secs(60);

    /// The ticket the tests issue: 40 characters of the base64url alphabet.
    const TICKET: &str = "Tkt-Qv7_secret-0123456789-abcdefghijklmn";

    fn sha256_hex(text: &str) -> String {
        use std::fmt::Write as _;
        Sha256::digest(text.as_bytes())
            .iter()
            .fold(String::new(), |mut hex, b| {
                let _ = write!(hex, "{b:02x}");
                hex
            })
    }

    /// A grant for [`TICKET`] that lets its client read values (and deliver nothing: the
    /// simulated desktop's actuator is never live anyway).
    fn grants_for_ticket() -> String {
        format!(
            "[[grant]]\nticket_sha256 = \"{}\"\napplications = []\ndeliver_input = false\n\
             read_values = true\n",
            sha256_hex(TICKET)
        )
    }

    /// The configuration of a test's engine: the pipe switch on, a pipe named after this test
    /// process and `tag`, values allowed by the operator, and the grants file if `grants`.
    fn configure(sandbox: &Sandbox, tag: &str, grants: Option<&str>) -> PathBuf {
        let mut text = format!(
            "[mcp]\ntransport = \"pipe\"\npipe_name = \"ommatix-test-{}-{tag}\"\n\
             [safety]\ninclude_values = true\n",
            std::process::id()
        );
        if let Some(grants) = grants {
            std::fs::write(sandbox.0.join("grants.toml"), grants).unwrap();
            text.push_str("grants_file = \"grants.toml\"\n");
        }
        let path = sandbox.0.join("ommatix.toml");
        std::fs::write(&path, text).unwrap();
        path
    }

    /// Lines read from a child's stream on a thread of their own.
    fn lines_of(stream: impl Read + Send + 'static) -> Receiver<String> {
        let (send, receive) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(stream).lines() {
                let Ok(line) = line else { break };
                if send.send(line).is_err() {
                    break;
                }
            }
        });
        receive
    }

    /// Everything a child wrote on a stream, collected on a thread of its own.
    fn collect(stream: impl Read + Send + 'static) -> Arc<Mutex<Vec<u8>>> {
        let collected = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&collected);
        thread::spawn(move || {
            let mut stream = stream;
            let mut buffer = [0_u8; 4096];
            while let Ok(n) = stream.read(&mut buffer) {
                if n == 0 {
                    break;
                }
                sink.lock().unwrap().extend_from_slice(&buffer[..n]);
            }
        });
        collected
    }

    fn text(collected: &Arc<Mutex<Vec<u8>>>) -> String {
        String::from_utf8_lossy(&collected.lock().unwrap()).into_owned()
    }

    fn wait(child: &mut Child) -> ExitStatus {
        let deadline = Instant::now() + PATIENCE;
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                return status;
            }
            assert!(Instant::now() < deadline, "the process did not end");
            thread::sleep(Duration::from_millis(20));
        }
    }

    /// `serve` with the pipe switch on, listening: its stderr is collected.
    struct Engine {
        child: Child,
        _stdin: ChildStdin,
        stdout: Arc<Mutex<Vec<u8>>>,
        stderr: Arc<Mutex<Vec<String>>>,
    }

    impl Engine {
        /// Starts `serve` and waits for the line that says it listens.
        fn start(sandbox: &Sandbox, config: &Path) -> Self {
            let config = config.to_str().unwrap();
            let mut child = sandbox
                .command(&["serve", "--config", config, "--session-id", "pipe"])
                .spawn()
                .expect("the binary starts");
            let stdin = child.stdin.take().unwrap();
            let stdout = collect(child.stdout.take().unwrap());
            let lines = lines_of(child.stderr.take().unwrap());
            let stderr = Arc::new(Mutex::new(Vec::new()));
            let deadline = Instant::now() + PATIENCE;
            loop {
                let left = deadline.saturating_duration_since(Instant::now());
                let line = lines.recv_timeout(left).unwrap_or_else(|_| {
                    panic!("serve did not listen: {:?}", stderr.lock().unwrap())
                });
                let listening = line.contains("on the pipe");
                stderr.lock().unwrap().push(line);
                if listening {
                    break;
                }
            }
            let sink = Arc::clone(&stderr);
            thread::spawn(move || {
                while let Ok(line) = lines.recv() {
                    sink.lock().unwrap().push(line);
                }
            });
            Self {
                child,
                _stdin: stdin,
                stdout,
                stderr,
            }
        }

        fn stderr(&self) -> String {
            self.stderr.lock().unwrap().join("\n")
        }

        /// Waits until `serve` has said `needle` on stderr.
        fn said(&self, needle: &str) -> String {
            let deadline = Instant::now() + PATIENCE;
            loop {
                let said = self.stderr();
                if said.contains(needle) {
                    return said;
                }
                assert!(
                    Instant::now() < deadline,
                    "serve did not say {needle:?}: {said}"
                );
                thread::sleep(Duration::from_millis(20));
            }
        }

        fn stop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    impl Drop for Engine {
        fn drop(&mut self) {
            self.stop();
        }
    }

    /// One `attach` process, driven through its standard streams.
    struct Attached {
        child: Child,
        stdin: Option<ChildStdin>,
        replies: Receiver<String>,
        stderr: Arc<Mutex<Vec<u8>>>,
        next_id: u64,
    }

    impl Attached {
        fn start(sandbox: &Sandbox, config: &Path, ticket_file: Option<&Path>) -> Self {
            let mut args = vec![
                "attach".to_owned(),
                "--config".to_owned(),
                config.to_str().unwrap().to_owned(),
            ];
            if let Some(path) = ticket_file {
                args.push("--ticket-file".to_owned());
                args.push(path.to_str().unwrap().to_owned());
            }
            // The ticket never travels as an argument: only the path of its file does.
            assert!(args.iter().all(|a| !a.contains(TICKET)));
            let args: Vec<&str> = args.iter().map(String::as_str).collect();
            let mut child = sandbox.command(&args).spawn().expect("the binary starts");
            let stdin = child.stdin.take();
            let replies = lines_of(child.stdout.take().unwrap());
            let stderr = collect(child.stderr.take().unwrap());
            Self {
                child,
                stdin,
                replies,
                stderr,
                next_id: 0,
            }
        }

        fn send(&mut self, line: &str) {
            let stdin = self.stdin.as_mut().expect("the input is open");
            stdin.write_all(line.as_bytes()).unwrap();
            stdin.flush().unwrap();
        }

        fn reply(&self) -> Value {
            let line = self
                .replies
                .recv_timeout(PATIENCE)
                .unwrap_or_else(|_| panic!("no reply; stderr: {}", text(&self.stderr)));
            serde_json::from_str(&line).expect("a JSON-RPC reply")
        }

        fn call(&mut self, name: &str, arguments: &Value) -> Value {
            self.next_id += 1;
            let line = tool(self.next_id, name, arguments);
            self.send(&line);
            let reply = self.reply();
            assert_eq!(reply["id"], self.next_id, "{reply}");
            reply["result"].clone()
        }

        /// Observes with values asked for; whether the page carries them (the input field's
        /// `text`, which the policy and the grant must both allow).
        fn sees_values(&mut self) -> bool {
            let result = self.call("observe_screen", &json!({ "include_values": true }));
            assert_eq!(result["isError"], false, "{result}");
            result["structuredContent"]["elements"]
                .as_array()
                .unwrap()
                .iter()
                .any(|e| e.get("text").is_some())
        }

        /// Closes the input and waits for the exit: the code and stderr.
        fn finish(mut self) -> (i32, String, Vec<String>) {
            drop(self.stdin.take());
            let status = wait(&mut self.child);
            // The reader thread ends with the process's stdout.
            let mut rest = Vec::new();
            while let Ok(line) = self.replies.recv_timeout(Duration::from_millis(200)) {
                rest.push(line);
            }
            (status.code().unwrap_or(-1), text(&self.stderr), rest)
        }

        /// Waits for the exit without closing the input: the code, stderr and any reply.
        fn exit(mut self) -> (i32, String, Vec<String>) {
            let status = wait(&mut self.child);
            let mut rest = Vec::new();
            while let Ok(line) = self.replies.recv_timeout(Duration::from_millis(200)) {
                rest.push(line);
            }
            (status.code().unwrap_or(-1), text(&self.stderr), rest)
        }
    }

    impl Drop for Attached {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    fn ticket_file(sandbox: &Sandbox, content: &str) -> PathBuf {
        let path = sandbox.0.join("ticket");
        std::fs::write(&path, content).unwrap();
        path
    }

    /// IT-34 and IT-26: an agent attached with a ticket acts under its grant, one attached
    /// without a ticket observes without values and delivers nothing, a ticket presented twice
    /// is refused, and nothing of the ticket appears on any stream, in any file but its own or
    /// in the arguments.
    #[test]
    fn it_34_agents_attached_with_and_without_a_ticket_and_a_ticket_used_twice() {
        let sandbox = Sandbox::new("tickets");
        let config = configure(&sandbox, "tickets", Some(&grants_for_ticket()));
        let ticket = ticket_file(&sandbox, TICKET);
        let mut engine = Engine::start(&sandbox, &config);
        engine.said("1 grant(s) now");

        let mut granted = Attached::start(&sandbox, &config, Some(&ticket));
        assert!(granted.sees_values(), "the grant lets it read values");
        let mut plain = Attached::start(&sandbox, &config, None);
        assert!(!plain.sees_values(), "no grant, no values");
        let page = plain.call("observe_screen", &json!({}));
        let page = &page["structuredContent"];
        let live = plain.call(
            "execute_action",
            &json!({
                "request_id": "live-1", "session_id": page["session_id"],
                "snapshot_id": page["snapshot_id"], "geometry_epoch": page["geometry_epoch"],
                "target_id": "e2", "expected_revision": 1, "action": "click", "dry_run": false
            }),
        );
        assert_eq!(
            live["structuredContent"]["error"]["code"], "POLICY_DENIED",
            "{live}"
        );
        assert_eq!(live["structuredContent"]["input_delivered"], false);

        // The same ticket again, while its first connection lasts: refused, nothing relayed.
        // Its input stays open: the engine's refusal, not the end of the input, ends it.
        let (code, stderr, rest) = Attached::start(&sandbox, &config, Some(&ticket)).exit();
        assert_eq!(code, 4, "{stderr}");
        assert!(stderr.contains("refused"), "{stderr}");
        assert!(rest.is_empty(), "{rest:?}");
        // And after it left.
        let (code, granted_stderr, _) = granted.finish();
        assert_eq!(code, 0, "{granted_stderr}");
        assert!(
            granted_stderr.contains("closed the input"),
            "{granted_stderr}"
        );
        let (code, stderr, _) = Attached::start(&sandbox, &config, Some(&ticket)).exit();
        assert_eq!(code, 4, "{stderr}");
        let (code, plain_stderr, _) = plain.finish();
        assert_eq!(code, 0, "{plain_stderr}");

        engine.stop();
        let engine_stdout = text(&engine.stdout);
        assert!(
            engine_stdout.is_empty(),
            "serve over the pipe writes nothing on stdout"
        );
        for (name, shown) in [
            ("serve's stderr", engine.stderr()),
            ("attach's stderr", granted_stderr),
            ("attach's stderr", plain_stderr),
            ("attach's stderr", stderr),
        ] {
            assert!(!shown.contains(TICKET), "the ticket in {name}");
            assert!(!shown.contains(&sha256_hex(TICKET)), "its hash in {name}");
        }
        for (path, bytes) in sandbox.files() {
            if path == ticket {
                continue;
            }
            let content = String::from_utf8_lossy(&bytes);
            assert!(
                !content.contains(TICKET),
                "the ticket in {}",
                path.display()
            );
        }
        assert!(std::fs::read_dir(sandbox.cwd()).unwrap().next().is_none());
        assert!(std::fs::read_dir(sandbox.tmp()).unwrap().next().is_none());
    }

    /// IT-34: a grant withdrawn from the file between two actions takes effect at the next one.
    #[test]
    fn it_34_a_grant_withdrawn_from_the_file_between_two_calls_takes_effect_at_the_next() {
        let sandbox = Sandbox::new("withdrawn");
        let config = configure(&sandbox, "withdrawn", Some(&grants_for_ticket()));
        let ticket = ticket_file(&sandbox, TICKET);
        let _engine = Engine::start(&sandbox, &config);
        let mut agent = Attached::start(&sandbox, &config, Some(&ticket));
        assert!(agent.sees_values());
        std::fs::write(sandbox.0.join("grants.toml"), "").unwrap();
        assert!(
            !agent.sees_values(),
            "withdrawn: no values at the next call"
        );
        std::fs::write(sandbox.0.join("grants.toml"), grants_for_ticket()).unwrap();
        assert!(agent.sees_values(), "listed again: values again");
        std::fs::write(sandbox.0.join("grants.toml"), "[[grant]]\nbroken").unwrap();
        assert!(!agent.sees_values(), "an invalid file grants nothing");
        let (code, stderr, _) = agent.finish();
        assert_eq!(code, 0, "{stderr}");
    }

    /// IT-34: without a grants file every client is without a grant, and `serve` says so
    /// once at start.
    #[test]
    fn it_34_without_a_grants_file_every_client_is_without_a_grant() {
        let sandbox = Sandbox::new("no-grants");
        let config = configure(&sandbox, "no-grants", None);
        let ticket = ticket_file(&sandbox, TICKET);
        let engine = Engine::start(&sandbox, &config);
        let mut agent = Attached::start(&sandbox, &config, Some(&ticket));
        assert!(!agent.sees_values());
        let (code, stderr, _) = agent.finish();
        assert_eq!(code, 0, "{stderr}");
        let said = engine.said("grants_file is not set");
        assert_eq!(said.matches("grants_file is not set").count(), 1, "{said}");
    }

    /// IT-34: a ticket file with a trailing line break is refused before anything is sent:
    /// exit code 1, nothing on stdout, nothing of the file on stderr, and the ticket is not
    /// spent (the same ticket, written without the line break, is admitted afterwards).
    #[test]
    fn it_34_a_ticket_file_with_a_line_break_is_refused_before_anything_is_sent() {
        let sandbox = Sandbox::new("line-break");
        let config = configure(&sandbox, "line-break", Some(&grants_for_ticket()));
        let _engine = Engine::start(&sandbox, &config);
        for content in [
            format!("{TICKET}\n"),
            format!("{TICKET}\r\n"),
            "short".to_owned(),
        ] {
            let ticket = ticket_file(&sandbox, &content);
            let refused = sandbox.run(
                &[
                    "attach",
                    "--config",
                    config.to_str().unwrap(),
                    "--ticket-file",
                    ticket.to_str().unwrap(),
                ],
                &tool(1, "get_engine_metrics", &json!({})),
            );
            assert_eq!(refused.status.code(), Some(1));
            assert!(refused.stdout.is_empty(), "{:?}", refused.stdout);
            let stderr = String::from_utf8_lossy(&refused.stderr);
            assert!(stderr.contains("--ticket-file"), "{stderr}");
            assert!(
                !stderr.contains(TICKET) && !stderr.contains("short"),
                "{stderr}"
            );
        }
        let ticket = ticket_file(&sandbox, TICKET);
        let mut agent = Attached::start(&sandbox, &config, Some(&ticket));
        assert!(
            agent.sees_values(),
            "the ticket was never sent, so never spent"
        );
        let (code, _, _) = agent.finish();
        assert_eq!(code, 0);
    }

    /// IT-32: `attach` with no engine listening exits with code 3, says so, and starts or
    /// writes nothing.
    #[test]
    fn it_32_attach_with_no_engine_listening_exits_3_and_starts_nothing() {
        let sandbox = Sandbox::new("no-engine");
        let config = configure(&sandbox, "no-engine", None);
        let ticket = ticket_file(&sandbox, TICKET);
        let started = Instant::now();
        let output = sandbox.run(
            &[
                "attach",
                "--config",
                config.to_str().unwrap(),
                "--ticket-file",
                ticket.to_str().unwrap(),
            ],
            &tool(1, "get_engine_metrics", &json!({})),
        );
        assert_eq!(output.status.code(), Some(3));
        assert!(output.stdout.is_empty(), "{:?}", output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(
            stderr.trim_end(),
            "ommatix: attach: no engine is listening on the pipe; start `ommatix serve` with \
             [mcp] transport = \"pipe\" (attach starts no engine)"
        );
        assert!(started.elapsed() < PATIENCE);
        assert!(std::fs::read_dir(sandbox.cwd()).unwrap().next().is_none());
        assert!(std::fs::read_dir(sandbox.tmp()).unwrap().next().is_none());
        // Still nobody listening: no engine was started.
        let again = sandbox.run(&["attach", "--config", config.to_str().unwrap()], "");
        assert_eq!(again.status.code(), Some(3));
    }

    /// IT-28: a second `serve` on the same pipe name is refused, and the first keeps serving.
    #[test]
    fn it_28_a_second_serve_on_the_same_name_is_refused() {
        let sandbox = Sandbox::new("second-serve");
        let config = configure(&sandbox, "second-serve", None);
        let _engine = Engine::start(&sandbox, &config);
        let second = sandbox.run(&["serve", "--config", config.to_str().unwrap()], "");
        assert!(!second.status.success());
        assert!(second.stdout.is_empty(), "{:?}", second.stdout);
        let stderr = String::from_utf8_lossy(&second.stderr);
        assert!(stderr.contains("name is taken"), "{stderr}");
        let mut agent = Attached::start(&sandbox, &config, None);
        assert!(!agent.sees_values());
        let (code, _, _) = agent.finish();
        assert_eq!(code, 0);
    }

    /// IT-31: `attach` relays pipelined requests and large replies between two real processes,
    /// every reply matched in order, and the engine going away ends it with code 5.
    #[test]
    fn it_31_attach_relays_pipelined_requests_and_large_replies_and_ends_with_the_engine() {
        const REQUESTS: u64 = 600;
        let sandbox = Sandbox::new("relay");
        let config = configure(&sandbox, "relay", None);
        let mut engine = Engine::start(&sandbox, &config);
        let mut agent = Attached::start(&sandbox, &config, None);
        let mut stdin = agent.stdin.take().unwrap();
        let writer = thread::spawn(move || {
            let mut batch = String::new();
            for id in 1..=REQUESTS {
                batch.push_str(&match id % 3 {
                    0 => message(id, "tools/list", json!({})),
                    1 => tool(id, "observe_screen", &json!({ "limit": 200 })),
                    _ => tool(id, "get_engine_metrics", &json!({})),
                });
            }
            stdin.write_all(batch.as_bytes()).unwrap();
            stdin.flush().unwrap();
            stdin
        });
        let mut bytes = 0;
        let mut largest = 0;
        for id in 1..=REQUESTS {
            let line = agent.replies.recv_timeout(PATIENCE).expect("a reply");
            bytes += line.len();
            largest = largest.max(line.len());
            let reply: Value = serde_json::from_str(&line).unwrap();
            assert_eq!(reply["id"], id, "replies in order");
            assert!(reply.get("result").is_some(), "{reply}");
        }
        assert!(largest > 7000, "tools/list is the large reply: {largest}");
        assert!(bytes > 1_500_000, "{bytes}");
        let stdin = writer.join().unwrap();
        agent.stdin = Some(stdin);

        engine.stop();
        let (code, stderr, rest) = agent.exit();
        assert!(rest.is_empty(), "{rest:?}");
        assert_eq!(code, 5, "{stderr}");
        assert!(
            stderr.contains("the engine ended the connection"),
            "{stderr}"
        );
    }
}
