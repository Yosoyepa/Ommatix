//! The compiled `ommatix` binary with the pipe transport switch (ADR-0043 and ADR-0040,
//! proposed): `serve` with `[mcp] transport = "pipe"` and one or more `attach` processes driven
//! through their standard streams, over the simulated desktop (never the screen, never input),
//! with real named pipes between the processes. And, in every build, the stdio path: the switch
//! off gives the session the committed bytes (`fixtures/stdio_session.golden`), the same with the
//! feature on and off, compared raw except for the run-dependent numbers [`masked`] names.
//!
//! The pipe tests need a Windows build with `--features broker-pipe`; each names its pipe after
//! this test process, so tests run in parallel and never meet another engine.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

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
    /// while writing is the binary's early exit, not a harness failure), and waits at most
    /// [`RUN_PATIENCE`] for it to end: past that the test fails, and the child is killed. Its
    /// output is read on threads of their own, to the end of each stream.
    fn run(&self, args: &[&str], input: &str) -> Output {
        // Small enough for the input pipe: writing it never waits on the child.
        assert!(input.len() < 32 * 1024, "{}", input.len());
        let mut child = KillOnDrop(self.command(args).spawn().expect("the binary starts"));
        let stdout = read_to_end_on_a_thread(child.0.stdout.take().expect("stdout"));
        let stderr = read_to_end_on_a_thread(child.0.stderr.take().expect("stderr"));
        let written = child
            .0
            .stdin
            .take()
            .expect("stdin")
            .write_all(input.as_bytes());
        if let Err(error) = written {
            assert_eq!(error.kind(), std::io::ErrorKind::BrokenPipe, "{error}");
        }
        let started = Instant::now();
        let status = loop {
            if let Some(status) = child.0.try_wait().expect("the child's status") {
                break status;
            }
            assert!(
                started.elapsed() < RUN_PATIENCE,
                "ommatix {args:?} did not end within {RUN_PATIENCE:?}; it is killed"
            );
            std::thread::sleep(Duration::from_millis(20));
        };
        Output {
            status,
            stdout: stdout.recv_timeout(RUN_PATIENCE).expect("stdout ends"),
            stderr: stderr.recv_timeout(RUN_PATIENCE).expect("stderr ends"),
        }
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

/// How long [`Sandbox::run`] waits for the binary to end before the test fails.
const RUN_PATIENCE: Duration = Duration::from_secs(60);

/// How long the harness waits for a killed child to be reaped. Windows ends a process
/// asynchronously and may first wait for its pending I/O to be cancelled, so the wait has its
/// own bound instead of `Child::wait`, which waits without one.
const REAP_PATIENCE: Duration = Duration::from_secs(10);

/// Kills `child`, unless it has already exited, and waits at most [`REAP_PATIENCE`] for it to be
/// reaped, polling `try_wait`. It never blocks longer: when the deadline passes, or a call fails,
/// it says why, with the pid, instead of the exit status.
fn kill_and_reap(child: &mut std::process::Child) -> Result<std::process::ExitStatus, String> {
    let pid = child.id();
    if let Ok(Some(status)) = child.try_wait() {
        return Ok(status);
    }
    // A kill that fails is not the end of the story (the child may have exited in between):
    // it is reported if the child is not reaped in time.
    let killed = child.kill();
    let deadline = Instant::now() + REAP_PATIENCE;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(20));
            }
            Ok(None) => {
                return Err(format!(
                    "pid {pid} was not reaped within {REAP_PATIENCE:?} of its kill \
                     (kill: {killed:?})"
                ));
            }
            Err(error) => {
                return Err(format!(
                    "waiting for pid {pid} failed: {error} (kill: {killed:?})"
                ));
            }
        }
    }
}

/// A child process killed and reaped, within [`REAP_PATIENCE`], when this is dropped, however the
/// test ends. Dropping it is cleanup only: it never panics (a drop may run while a failed test is
/// already unwinding) and never blocks past the bound; when the child is not confirmed ended it
/// writes the pid and the reason on stderr and gives up. A test that needs the end confirmed
/// stops the child explicitly ([`KillOnDrop::stop`]).
struct KillOnDrop(std::process::Child);

#[cfg(all(windows, feature = "broker-pipe"))]
impl KillOnDrop {
    /// Kills and reaps the child within [`REAP_PATIENCE`], and fails the test if its end is not
    /// confirmed by then (only the pipe's tests stop a child explicitly).
    fn stop(&mut self) -> std::process::ExitStatus {
        kill_and_reap(&mut self.0).unwrap_or_else(|why| panic!("stopping a child: {why}"))
    }
}

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        if let Err(why) = kill_and_reap(&mut self.0) {
            eprintln!("test harness: {why}; left behind");
        }
    }
}

/// Everything `stream` yields to its end, sent once on the returned channel.
fn read_to_end_on_a_thread(
    mut stream: impl std::io::Read + Send + 'static,
) -> std::sync::mpsc::Receiver<Vec<u8>> {
    let (send, receive) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stream.read_to_end(&mut bytes);
        let _ = send.send(bytes);
    });
    receive
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

/// The keys whose values depend on the run, each with the reason, in the two spellings a reply
/// carries them: plainly, and inside the escaped JSON of a `content[].text` block.
///
/// - `io.ommatix/timings` and `io.ommatix/action_timings`: measured stage durations, an object
///   of numbers in `_meta` (every number in it is masked);
/// - `age_ms`: the time since the page's snapshot, a number (masked only where it is a number:
///   the schema's `"age_ms":{"minimum":0}` in `tools/list` is stable and compared).
const TIMING_KEYS: [&str; 4] = [
    "\"io.ommatix/timings\":",
    "\\\"io.ommatix/timings\\\":",
    "\"io.ommatix/action_timings\":",
    "\\\"io.ommatix/action_timings\\\":",
];
const AGE_KEYS: [&str; 2] = ["\"age_ms\":", "\\\"age_ms\\\":"];

/// The length of the JSON number at the start of `rest` (digits, sign, point, exponent).
fn number_len(rest: &str) -> usize {
    rest.bytes()
        .take_while(|b| b.is_ascii_digit() || matches!(b, b'-' | b'+' | b'.' | b'e' | b'E'))
        .count()
}

/// The raw bytes of the stdio wire with only the values of [`TIMING_KEYS`] and [`AGE_KEYS`]
/// replaced by `0`: each value's own span, nothing around it. Delimiters, escaping, line
/// endings, property order and every other byte stay as they were.
fn masked(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    loop {
        let next = TIMING_KEYS
            .iter()
            .chain(AGE_KEYS.iter())
            .filter_map(|key| rest.find(key).map(|at| (at, *key)))
            .min_by_key(|(at, _)| *at);
        let Some((at, key)) = next else {
            out.push_str(rest);
            return out;
        };
        let after = at + key.len();
        out.push_str(&rest[..after]);
        rest = &rest[after..];
        if AGE_KEYS.contains(&key) {
            let n = number_len(rest);
            if n > 0 {
                out.push('0');
                rest = &rest[n..];
            }
            continue;
        }
        // A timings object: `{`, then `"name":number` pairs (escaped or not), then `}`.
        let close = rest.find('}').expect("a timings object ends");
        let object = &rest[..=close];
        assert!(
            object.starts_with('{') && !object[1..].contains('{'),
            "a timings object is flat: {object}"
        );
        let mut inner = object;
        while let Some(colon) = inner.find(':') {
            out.push_str(&inner[..=colon]);
            inner = &inner[colon + 1..];
            let n = number_len(inner);
            assert!(n > 0, "a timing is a number: {object}");
            out.push('0');
            inner = &inner[n..];
        }
        out.push_str(inner);
        rest = &rest[close + 1..];
    }
}

/// IT-33: with the switch off, `serve` over stdio emits the committed bytes on stdout and on
/// stderr, compared raw: only the run-dependent numbers of [`masked`] are replaced, each for its
/// stated reason. This test runs in every build, so the build with `broker-pipe` and the build
/// without it are held to the same file.
#[test]
fn it_33_with_the_switch_off_serve_over_stdio_emits_the_committed_bytes() {
    let golden = include_str!("fixtures/stdio_session.golden");
    let sandbox = Sandbox::new("parity");
    let output = sandbox.run(&["serve", "--session-id", "parity"], &stdio_session());
    assert!(output.status.success(), "{:?}", output.status);
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    let actual = format!("# stdout\n{}# stderr\n{stderr}", masked(&stdout));
    if actual != golden {
        // The whole session, between markers, so that a deliberate change can be reviewed.
        eprintln!("---- actual stdio session ----\n{actual}---- end ----");
    }
    assert!(
        actual == golden,
        "the stdio session differs from fixtures/stdio_session.golden"
    );
}

/// The mask replaces each run-dependent value's own span and nothing else.
#[test]
fn it_33_the_mask_touches_only_the_run_dependent_values() {
    let line = "{\"a\":1,\"_meta\":{\"io.ommatix/timings\":{\"x_us\":12,\"y_us\":3}},\
                \"age_ms\":40,\"s\":{\"age_ms\":{\"minimum\":0}},\
                \"text\":\"{\\\"age_ms\\\":7,\\\"io.ommatix/action_timings\\\":{\\\"t_us\\\":99}}\"}\n";
    assert_eq!(
        masked(line),
        "{\"a\":1,\"_meta\":{\"io.ommatix/timings\":{\"x_us\":0,\"y_us\":0}},\
         \"age_ms\":0,\"s\":{\"age_ms\":{\"minimum\":0}},\
         \"text\":\"{\\\"age_ms\\\":0,\\\"io.ommatix/action_timings\\\":{\\\"t_us\\\":0}}\"}\n"
    );
    let stable = "{\"jsonrpc\":\"2.0\", \"id\":1}\r\n";
    assert_eq!(masked(stable), stable);
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

    use ommatix_adapter_mcp::pipe::RELAY_CHUNK;
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

    /// Everything a child wrote on a stream, collected on a thread of its own. The thread ends
    /// at the stream's end (a read of 0 bytes), or at a read error, which it returns: the two are
    /// told apart, and only the first is the whole stream.
    struct Collected {
        bytes: Arc<Mutex<Vec<u8>>>,
        reader: thread::JoinHandle<Result<(), String>>,
    }

    fn collect(stream: impl Read + Send + 'static) -> Collected {
        let bytes = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&bytes);
        let reader = thread::spawn(move || {
            let mut stream = stream;
            let mut buffer = [0_u8; 4096];
            loop {
                match stream.read(&mut buffer) {
                    Ok(0) => return Ok(()),
                    Ok(n) => sink.lock().unwrap().extend_from_slice(&buffer[..n]),
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(error) => return Err(error.to_string()),
                }
            }
        });
        Collected { bytes, reader }
    }

    impl Collected {
        /// What has been collected so far.
        fn text(&self) -> String {
            String::from_utf8_lossy(&self.bytes.lock().unwrap()).into_owned()
        }

        /// Everything, once the stream has ended (the writer's process exited and nothing else
        /// holds the stream). The test fails if the reader stopped on a read error instead of the
        /// stream's end (what it collected would be a prefix), or if it has not ended within
        /// [`PATIENCE`].
        fn into_text_at_end(self) -> String {
            let deadline = Instant::now() + PATIENCE;
            while !self.reader.is_finished() {
                assert!(Instant::now() < deadline, "the stream did not end");
                thread::sleep(Duration::from_millis(10));
            }
            let ended = self.reader.join().expect("the reader thread");
            if let Err(error) = ended {
                panic!("the stream was not read to its end: a read failed ({error})");
            }
            String::from_utf8_lossy(&self.bytes.lock().unwrap()).into_owned()
        }
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
        child: KillOnDrop,
        _stdin: ChildStdin,
        stdout: Collected,
        stderr: Arc<Mutex<Vec<String>>>,
    }

    impl Engine {
        /// Starts `serve` and waits at most [`PATIENCE`] for the line that says it listens; past
        /// that the test fails and the child is killed (it is held by [`KillOnDrop`] from the
        /// start).
        fn start(sandbox: &Sandbox, config: &Path) -> Self {
            let config = config.to_str().unwrap();
            let mut child = KillOnDrop(
                sandbox
                    .command(&["serve", "--config", config, "--session-id", "pipe"])
                    .spawn()
                    .expect("the binary starts"),
            );
            let stdin = child.0.stdin.take().unwrap();
            let stdout = collect(child.0.stdout.take().unwrap());
            let lines = lines_of(child.0.stderr.take().unwrap());
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

        /// Ends `serve` and fails the test if its end is not confirmed within the bound. When the
        /// engine is only dropped, its guard ends it and reports, without failing.
        fn stop(&mut self) {
            self.child.stop();
        }
    }

    /// One `attach` process, driven through its standard streams.
    struct Attached {
        child: KillOnDrop,
        stdin: Option<ChildStdin>,
        replies: Receiver<String>,
        stderr: Option<Collected>,
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
            let mut child = KillOnDrop(sandbox.command(&args).spawn().expect("the binary starts"));
            let stdin = child.0.stdin.take();
            let replies = lines_of(child.0.stdout.take().unwrap());
            let stderr = Some(collect(child.0.stderr.take().unwrap()));
            Self {
                child,
                stdin,
                replies,
                stderr,
                next_id: 0,
            }
        }

        /// Writes one line to `attach`'s input. Each line is a few hundred bytes and every call
        /// waits for its reply before the next line is sent, so the 64 KiB input pipe never holds
        /// more than one unread line and this write cannot wait on the child.
        fn send(&mut self, line: &str) {
            let stdin = self.stdin.as_mut().expect("the input is open");
            stdin.write_all(line.as_bytes()).unwrap();
            stdin.flush().unwrap();
        }

        fn reply(&self) -> Value {
            let line = self
                .replies
                .recv_timeout(PATIENCE)
                .unwrap_or_else(|_| panic!("no reply; stderr: {}", self.stderr_so_far()));
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

        fn stderr_so_far(&self) -> String {
            self.stderr
                .as_ref()
                .map(Collected::text)
                .unwrap_or_default()
        }

        /// Closes the input and waits for the exit: the code, the whole of stderr (read to its
        /// end) and any reply left.
        fn finish(mut self) -> (i32, String, Vec<String>) {
            drop(self.stdin.take());
            self.exit()
        }

        /// Waits for the exit without closing the input: the code, the whole of stderr (read to
        /// its end, so that nothing written just before the exit is missed) and any reply.
        fn exit(mut self) -> (i32, String, Vec<String>) {
            let status = wait(&mut self.child.0);
            let stderr = self.stderr.take().expect("stderr").into_text_at_end();
            // The reader thread ends with the process's stdout.
            let mut rest = Vec::new();
            while let Ok(line) = self.replies.recv_timeout(Duration::from_millis(200)) {
                rest.push(line);
            }
            (status.code().unwrap_or(-1), stderr, rest)
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
        let engine_stdout = engine.stdout.text();
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

    /// IT-28: a second `serve` on the same pipe name is refused with exit code 7 and a line that
    /// names the refusal (access denied on the first instance) and its usual cause, and the first
    /// keeps serving.
    #[test]
    fn it_28_a_second_serve_on_the_same_name_is_refused() {
        let sandbox = Sandbox::new("second-serve");
        let config = configure(&sandbox, "second-serve", None);
        let _engine = Engine::start(&sandbox, &config);
        let second = sandbox.run(&["serve", "--config", config.to_str().unwrap()], "");
        assert_eq!(second.status.code(), Some(7), "{second:?}");
        assert!(second.stdout.is_empty(), "{:?}", second.stdout);
        let stderr = String::from_utf8_lossy(&second.stderr);
        assert_eq!(
            stderr.trim_end(),
            format!(
                "ommatix: cannot serve on the pipe \\\\.\\pipe\\ommatix-test-{}-second-serve: \
                 the system refused to create the first instance of this name with access denied \
                 (os error 5); the usual cause is that another process already holds the name \
                 (another `ommatix serve` with the same name, or another program), but access \
                 denied alone does not prove it; exit code 7",
                std::process::id()
            )
        );
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
        // Every reply has arrived, so the writer has written every request; its join is still
        // bounded, so that a writer blocked on the child fails the test instead of hanging it.
        let deadline = Instant::now() + PATIENCE;
        while !writer.is_finished() {
            assert!(
                Instant::now() < deadline,
                "the writer of attach's input did not finish"
            );
            thread::sleep(Duration::from_millis(10));
        }
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

    /// The capacity of the pipe the standard library creates for a child's piped stdout (pinned
    /// 1.99.0, `child_pipe.rs`: `PIPE_BUFFER_CAPACITY`, 64 KiB).
    const STD_PIPE_CAPACITY: usize = 64 * 1024;

    /// Whether the bytes the host can still read after the exit, `held`, are within one write of
    /// the relay ([`RELAY_CHUNK`]) of the stdout pipe's capacity. It describes what was retained
    /// and is read after the exit; it is not an observation that a write of the relay was
    /// blocked at any moment (the pipe's sizes are advisory and `RELAY_CHUNK` is an upper bound).
    fn retained_within_one_relay_write_of_capacity(held: usize) -> bool {
        held > STD_PIPE_CAPACITY - RELAY_CHUNK && held <= STD_PIPE_CAPACITY
    }

    /// The bound the tests give `attach`'s exit once its end is due.
    const BOUND: Duration = Duration::from_secs(10);

    /// An `attach` whose host sends `requests` `tools/list` requests (more than 7 kB of reply
    /// each), keeps the read end of its stdout open and unread, and waits three seconds. What the
    /// tests then assert is what they observe: exit codes and timings, the bytes the unread pipe
    /// retained and the whole replies among them. That the relay's write to the host was blocked
    /// is not observed; it rests on the source (the relay's owned handle is the only writer of
    /// the host's stdout: UT-79's source test) and on those observations.
    struct StalledHost {
        engine: Engine,
        child: KillOnDrop,
        stdin: Option<ChildStdin>,
        stdout: std::process::ChildStdout,
        stderr: Option<Collected>,
        requests: usize,
    }

    impl StalledHost {
        fn start(sandbox: &Sandbox, tag: &str, requests: u64) -> Self {
            let config = configure(sandbox, tag, None);
            let engine = Engine::start(sandbox, &config);
            let mut child = KillOnDrop(
                sandbox
                    .command(&["attach", "--config", config.to_str().unwrap()])
                    .spawn()
                    .expect("the binary starts"),
            );
            let mut stdin = child.0.stdin.take().unwrap();
            let stdout = child.0.stdout.take().unwrap();
            let stderr = collect(child.0.stderr.take().unwrap());
            // About 150 bytes each: the whole batch fits the input pipe, so this write returns.
            let batch: String = (1..=requests)
                .map(|id| message(id, "tools/list", json!({})))
                .collect();
            assert!(batch.len() < STD_PIPE_CAPACITY / 2, "{}", batch.len());
            stdin.write_all(batch.as_bytes()).unwrap();
            stdin.flush().unwrap();
            thread::sleep(Duration::from_secs(3));
            Self {
                engine,
                child,
                stdin: Some(stdin),
                stdout,
                stderr: Some(stderr),
                requests: usize::try_from(requests).unwrap(),
            }
        }

        /// Waits at most `bound` for the exit: its status and how long it took, or `None`.
        fn exit_within(&mut self, bound: Duration) -> Option<(ExitStatus, Duration)> {
            let from = Instant::now();
            loop {
                if let Some(status) = self.child.0.try_wait().unwrap() {
                    return Some((status, from.elapsed()));
                }
                if from.elapsed() >= bound {
                    return None;
                }
                thread::sleep(Duration::from_millis(10));
            }
        }

        /// [`Self::after_exit`], or, when the process did not exit (`timed_out`), a failure that
        /// says so with what stderr held; the watchdog only fails the test, and [`KillOnDrop`] ends
        /// the process.
        fn after_exit_or_fail(&mut self, timed_out: bool) -> (String, usize, usize) {
            assert!(
                !timed_out,
                "attach did not exit within the bound; stderr so far: {}",
                self.stderr
                    .as_ref()
                    .map(Collected::text)
                    .unwrap_or_default()
            );
            self.after_exit()
        }

        /// After the exit: the whole of stderr, and what the stdout pipe still held (the host
        /// reads it only now), with its count of whole replies.
        fn after_exit(&mut self) -> (String, usize, usize) {
            let stderr = self.stderr.take().unwrap().into_text_at_end();
            let mut held = Vec::new();
            self.stdout.read_to_end(&mut held).unwrap();
            let whole = held.iter().fold(0, |n, &b| n + usize::from(b == b'\n'));
            (stderr, held.len(), whole)
        }
    }

    /// IT-31: the host's input ends while its stdout is unread. With 12 requests (fewer than the
    /// engine's side takes), `attach` exits 0 within [`BOUND`] after the host closes its input,
    /// with no drain wait; afterwards the unread pipe still holds bytes within one relay write of
    /// its capacity, and fewer whole replies than requests: output the host had not read is lost
    /// to it.
    #[test]
    fn it_31_attach_exits_0_at_the_end_of_its_input_with_its_output_unread() {
        let sandbox = Sandbox::new("full-output");
        let mut host = StalledHost::start(&sandbox, "full-output", 12);
        drop(host.stdin.take());
        let ended = host.exit_within(BOUND);
        let (stderr, held, whole) = host.after_exit_or_fail(ended.is_none());
        let (status, took) = ended.unwrap();
        assert_eq!(status.code(), Some(0), "{stderr}");
        assert_eq!(
            stderr.trim_end(),
            "ommatix: attach: the agent's host closed the input; detached"
        );
        assert!(
            retained_within_one_relay_write_of_capacity(held),
            "the unread pipe retained {held} bytes"
        );
        assert!(
            whole < host.requests,
            "{whole} whole replies of {}",
            host.requests
        );
        eprintln!(
            "exit {took:?} after the end of the input; the pipe retained {held} bytes, {whole} \
             whole replies of {}",
            host.requests
        );
    }

    /// IT-31: with 120 requests (more than the engine's side takes) and stdout unread, closing
    /// the host's input does not end `attach` within [`BOUND`] while the engine lives (the end of
    /// the input is not observed by the relay). Once the engine is stopped, `attach` exits 5 within
    /// `BOUND` plus the 5 s drain; the unread pipe retains bytes within one relay write of its
    /// capacity and fewer whole replies than requests.
    #[test]
    fn it_31_with_more_requests_queued_than_the_engine_takes_attach_waits_for_the_engine() {
        let sandbox = Sandbox::new("held-input");
        let mut host = StalledHost::start(&sandbox, "held-input", 120);
        drop(host.stdin.take());
        assert!(
            host.exit_within(BOUND).is_none(),
            "attach exited although its host's 120 requests did not fit the engine's side"
        );
        host.engine.stop();
        let ended = host.exit_within(BOUND + Duration::from_secs(5));
        let (stderr, held, whole) = host.after_exit_or_fail(ended.is_none());
        let (status, took) = ended.unwrap();
        assert_eq!(status.code(), Some(5), "{stderr}");
        assert_eq!(
            stderr.trim_end(),
            "ommatix: attach: the engine ended the connection"
        );
        assert!(retained_within_one_relay_write_of_capacity(held), "{held}");
        assert!(whole < host.requests, "{whole}");
        eprintln!(
            "exit {took:?} after the engine ended; the pipe retained {held} bytes, {whole} whole \
             replies of {}",
            host.requests
        );
    }

    /// IT-31: the case the bound does not cover. The host neither reads `attach`'s stdout nor
    /// closes its input (12 requests sent, all of which fit the engine's side); the engine is then
    /// stopped. `attach` is still running after [`BOUND`] plus the 5 s drain: the engine's end is
    /// not noticed. Once the host closes its input, `attach` exits 0 within `BOUND`.
    #[test]
    fn it_31_a_host_that_neither_reads_nor_closes_keeps_attach_after_the_engine_ends() {
        let sandbox = Sandbox::new("unnoticed-end");
        let mut host = StalledHost::start(&sandbox, "unnoticed-end", 12);
        host.engine.stop();
        assert!(
            host.exit_within(BOUND + Duration::from_secs(5)).is_none(),
            "attach exited although its host neither read nor closed"
        );
        drop(host.stdin.take());
        let ended = host.exit_within(BOUND);
        let (stderr, held, whole) = host.after_exit_or_fail(ended.is_none());
        let (status, took) = ended.unwrap();
        assert_eq!(status.code(), Some(0), "{stderr}");
        assert_eq!(
            stderr.trim_end(),
            "ommatix: attach: the agent's host closed the input; detached"
        );
        assert!(retained_within_one_relay_write_of_capacity(held), "{held}");
        assert!(whole < host.requests, "{whole}");
        eprintln!(
            "exit {took:?} after the host closed its input, the engine long gone; the pipe \
             retained {held} bytes, {whole} whole replies of {}",
            host.requests
        );
    }

    /// IT-34: a configuration that `attach` cannot use is refused (exit code 1) by its path and
    /// the kind of the problem, never by its text: a marker on the bad line appears nowhere, for
    /// a parse error (with its line number) and for a value that fails its rule (by its key).
    #[test]
    fn it_34_attach_names_a_bad_configuration_without_quoting_it() {
        const MARKER: &str = "Qz7MarkerOfTheFile";
        let sandbox = Sandbox::new("bad-config");
        let pipe = format!(
            "[mcp]\ntransport = \"pipe\"\npipe_name = \"ommatix-test-{}-bad-config\"\n",
            std::process::id()
        );
        for (name, text, says) in [
            (
                "syntax.toml",
                format!("{pipe}{MARKER} = = 1\n"),
                "not valid TOML for this configuration (a syntax error, an unknown key or a \
                 value of the wrong type) at line 4, column",
            ),
            (
                "rule.toml",
                format!("{pipe}[safety.allowed_window_classes]\n\"{MARKER}\" = []\n"),
                "invalid values: [safety] allowed_window_classes fail their rule",
            ),
        ] {
            let path = sandbox.0.join(name);
            std::fs::write(&path, &text).unwrap();
            let refused = sandbox.run(
                &["attach", "--config", path.to_str().unwrap()],
                &tool(1, "get_engine_metrics", &json!({})),
            );
            assert_eq!(refused.status.code(), Some(1), "{refused:?}");
            assert!(refused.stdout.is_empty(), "{:?}", refused.stdout);
            let stderr = String::from_utf8_lossy(&refused.stderr);
            assert!(
                stderr.contains(&format!("--config {}: ", path.display())),
                "{stderr}"
            );
            assert!(stderr.contains(says), "{stderr}");
            assert!(!stderr.contains(MARKER), "{stderr}");
        }
        assert!(std::fs::read_dir(sandbox.cwd()).unwrap().next().is_none());
        assert!(std::fs::read_dir(sandbox.tmp()).unwrap().next().is_none());
    }

    /// IT-34: the operator's ticket and grants files are read only from regular files. A path
    /// that opens as a named pipe (a running engine's own) or as a device (`NUL`) is refused
    /// before anything is read: `attach` exits 1 and sends nothing, `serve` says the grants file
    /// is invalid and no client has a grant. The engine whose pipe was named keeps serving.
    #[test]
    fn it_34_ticket_and_grants_files_that_are_not_regular_files_are_refused_unread() {
        let sandbox = Sandbox::new("not-a-file");
        let config = configure(&sandbox, "not-a-file", None);
        let _engine = Engine::start(&sandbox, &config);
        let pipe = format!(r"\\.\pipe\ommatix-test-{}-not-a-file", std::process::id());
        for ticket in [pipe.as_str(), "NUL"] {
            let refused = sandbox.run(
                &[
                    "attach",
                    "--config",
                    config.to_str().unwrap(),
                    "--ticket-file",
                    ticket,
                ],
                &tool(1, "get_engine_metrics", &json!({})),
            );
            assert_eq!(refused.status.code(), Some(1), "{refused:?}");
            assert!(refused.stdout.is_empty(), "{:?}", refused.stdout);
            let stderr = String::from_utf8_lossy(&refused.stderr);
            assert!(
                stderr.contains("--ticket-file: the ticket file is not a regular file on a disk"),
                "{stderr}"
            );
        }
        // A second engine whose grants file is the first engine's pipe.
        let other = Sandbox::new("not-a-file-grants");
        let other_config = other.0.join("ommatix.toml");
        std::fs::write(
            &other_config,
            format!(
                "[mcp]\ntransport = \"pipe\"\npipe_name = \"ommatix-test-{}-not-a-file-2\"\n\
                 [safety]\ninclude_values = true\ngrants_file = '{pipe}'\n",
                std::process::id()
            ),
        )
        .unwrap();
        let second = Engine::start(&other, &other_config);
        let said = second.said("is invalid now");
        assert!(
            said.contains(&format!(
                "grants file {pipe} is invalid now (it is not a regular file on a disk"
            )),
            "{said}"
        );
        let mut agent = Attached::start(&sandbox, &config, None);
        assert!(!agent.sees_values());
        let (code, _, _) = agent.finish();
        assert_eq!(code, 0);
    }
}
