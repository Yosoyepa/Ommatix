//! The broker's named pipe (ADR-0043, proposed), on Windows with the feature `broker-pipe`:
//! real pipes between threads of this test process, under names unique to the run, serving the
//! broker on the simulated desktop. Nothing injects real input, captures a screen or calls a
//! model provider; nothing here needs administrator rights.

#![cfg(all(windows, feature = "broker-pipe"))]

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::num::NonZeroU8;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier, Condvar, Mutex, MutexGuard, PoisonError, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use common::{error_code, modern_meta};
use interprocess::os::windows::named_pipe::{PipeListener, PipeListenerOptions, pipe_mode};
use ommatix_adapter_mcp::pipe::{
    CONNECT_ATTEMPTS, CONNECT_PAUSE, ConnectError, FULL_LINE, MAX_REFUSING, PipeError, PipeName,
    PipeServer, PipeStats, PipeStream, RelayEnd, RelayError, ShutdownState, connect, connect_with,
    relay,
};
use ommatix_adapter_mcp::{
    Admission, Broker, DEFAULT_MAX_MESSAGE_BYTES, LEASE_META_KEY, McpConfig,
};
use ommatix_adapter_sim::fixture::FixtureActuator;
use ommatix_adapter_sim::{FixtureDesktop, FixtureScene, confirmed};
use ommatix_application::contract::ValidatedAction;
use ommatix_application::{
    CapturedFrame, DeliveryReceipt, EngineConfig, FrameCapturer, Grant, GrantStore, LocalPolicy,
    ManualClock, NoAccessibility, OsActuator, PortError, Ports, TopLevelWindow, WindowOpacity,
};
use ommatix_domain::bbox::BBox;
use ommatix_domain::fusion::Role;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const SESSION: &str = "pipe-session";
const APP: &str = "pipe-test-app";
const TERM: Duration = Duration::from_secs(2);
/// The longest any test waits for a condition it expects; never reached when the code is right.
const PATIENCE: Duration = Duration::from_secs(20);

type Listener = PipeListener<pipe_mode::Bytes, pipe_mode::Bytes>;

// ---- the world: the simulated desktop with a live test actuator ----

fn bx(x0: f64, y0: f64, x1: f64, y1: f64) -> BBox {
    BBox::new(x0, y0, x1, y1).expect("test boxes are valid")
}

fn scene() -> FixtureScene {
    let mut scene = FixtureScene::new(320, 200);
    scene.add_control(confirmed(Role::Button, bx(10.0, 10.0, 130.0, 40.0)));
    scene.add_word("Zephyrquartz", bx(15.0, 15.0, 125.0, 35.0));
    scene.add_control(confirmed(Role::Input, bx(10.0, 70.0, 220.0, 100.0)));
    scene.add_word("Obsidianmarrow", bx(15.0, 75.0, 215.0, 95.0));
    scene
}

/// The operator: live input to the test application, values allowed.
struct Operator;

impl ommatix_application::PolicyStore for Operator {
    fn current(&self) -> LocalPolicy {
        LocalPolicy {
            actuation_enabled: true,
            include_values: true,
            allowed_applications: vec![APP.to_owned()],
            ..LocalPolicy::default()
        }
    }
}

/// The desktop's actuator made live; what it is handed is recorded by the simulated desktop,
/// and each delivery passes the world's delivery gate first.
struct Live(FixtureActuator, Arc<Gate>);

impl OsActuator for Live {
    fn name(&self) -> &'static str {
        "pipe-test-live"
    }

    fn is_live(&self) -> bool {
        true
    }

    fn foreground_application(&self) -> Option<String> {
        Some(APP.to_owned())
    }

    fn foreground_window(&self) -> Option<TopLevelWindow> {
        self.0.foreground_window()
    }

    fn window_at(&self, point: (f64, f64)) -> Option<TopLevelWindow> {
        self.0.window_at(point)
    }

    fn top_level_windows(&self) -> Option<Vec<TopLevelWindow>> {
        self.0.top_level_windows()
    }

    fn window_opacity(&self, window: &TopLevelWindow) -> Option<WindowOpacity> {
        self.0.window_opacity(window)
    }

    fn deliver(&mut self, action: &ValidatedAction) -> Result<DeliveryReceipt, PortError> {
        self.1.enter();
        self.0.deliver(action)?;
        Ok(DeliveryReceipt {
            submitted: 1,
            inserted: 1,
        })
    }
}

/// The operator's grants by ticket hash.
#[derive(Default)]
struct Grants(Mutex<BTreeMap<[u8; 32], Grant>>);

impl Grants {
    fn listed(&self) -> MutexGuard<'_, BTreeMap<[u8; 32], Grant>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn list(&self, ticket: &str, grant: Grant) {
        self.listed()
            .insert(Sha256::digest(ticket.as_bytes()).into(), grant);
    }
}

impl GrantStore for Grants {
    fn grant(&self, ticket_sha256: &[u8; 32]) -> Option<Grant> {
        self.listed().get(ticket_sha256).cloned()
    }
}

fn grant(deliver_input: bool) -> Grant {
    Grant {
        applications: BTreeSet::from([APP.to_owned()]),
        deliver_input,
        read_values: true,
        ..Grant::default()
    }
}

/// A ticket of the exact form, distinct for each `n`.
fn ticket(n: u64) -> String {
    format!("Pp7-Tkt_{n:035}")
}

/// A gate on a port of the simulated desktop (the capturer, or the actuator's deliveries):
/// every call is counted as it begins, and waits while the gate is closed. A tool call that
/// reaches it holds the broker's lock meanwhile. It can also be armed with an [`ExitHold`], which
/// the next call registers on its own thread.
#[derive(Default)]
struct Gate {
    /// Whether the gate is closed, and how many calls began.
    state: Mutex<(bool, usize)>,
    changed: Condvar,
    armed: Mutex<Option<ExitHold>>,
}

/// A release a test opens; a thread that waits on it goes on by itself after [`PATIENCE`] and
/// records that, so that a defect that would hang the test fails it instead.
#[derive(Default)]
struct Release {
    open: Mutex<bool>,
    changed: Condvar,
    failsafe: std::sync::atomic::AtomicBool,
}

impl Release {
    fn open(&self) {
        *self.open.lock().unwrap_or_else(PoisonError::into_inner) = true;
        self.changed.notify_all();
    }

    fn pass(&self) {
        let deadline = Instant::now() + PATIENCE;
        let mut open = self.open.lock().unwrap_or_else(PoisonError::into_inner);
        while !*open {
            let Some(left) = deadline.checked_duration_since(Instant::now()) else {
                self.failsafe.store(true, Ordering::SeqCst);
                return;
            };
            open = self
                .changed
                .wait_timeout(open, left)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
    }

    fn failsafe_fired(&self) -> bool {
        self.failsafe.load(Ordering::SeqCst)
    }
}

/// Held by a thread-local of the thread that registers it: its destructor runs after that
/// thread's closure has returned and before the thread has exited, so a join of the thread waits
/// on it. It says when it began, then waits for its release. Registered by a listener's worker
/// through the simulated desktop, it holds the worker after its connection is closed and before
/// its thread is joined.
struct ExitHold {
    began: mpsc::Sender<()>,
    release: Arc<Release>,
}

impl Drop for ExitHold {
    fn drop(&mut self) {
        let _ = self.began.send(());
        self.release.pass();
    }
}

thread_local! {
    static EXIT_HOLD: std::cell::RefCell<Option<ExitHold>> = const { std::cell::RefCell::new(None) };
}

impl Gate {
    fn state(&self) -> MutexGuard<'_, (bool, usize)> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn set_closed(&self, closed: bool) {
        self.state().0 = closed;
        self.changed.notify_all();
    }

    fn captures(&self) -> usize {
        self.state().1
    }

    /// Waits until at least `n` captures began; fails the test after [`PATIENCE`].
    fn wait_captures(&self, n: usize) -> usize {
        let deadline = Instant::now() + PATIENCE;
        let mut state = self.state();
        while state.1 < n {
            let left = deadline
                .checked_duration_since(Instant::now())
                .unwrap_or_else(|| panic!("{} captures, waiting for {n}", state.1));
            state = self
                .changed
                .wait_timeout(state, left)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
        state.1
    }

    fn arm(&self, hold: ExitHold) {
        *self.armed.lock().unwrap_or_else(PoisonError::into_inner) = Some(hold);
    }

    fn enter(&self) {
        if let Some(hold) = self
            .armed
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
        {
            EXIT_HOLD.with(|slot| *slot.borrow_mut() = Some(hold));
        }
        let mut state = self.state();
        state.1 += 1;
        self.changed.notify_all();
        while state.0 {
            state = self
                .changed
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }
}

/// Opens its gate when dropped, so that a failing test never leaves a call waiting on it.
struct Opens(Arc<Gate>);

impl Drop for Opens {
    fn drop(&mut self) {
        self.0.set_closed(false);
    }
}

struct Gated {
    inner: Box<dyn FrameCapturer>,
    gate: Arc<Gate>,
}

impl FrameCapturer for Gated {
    fn name(&self) -> &'static str {
        self.inner.name()
    }

    fn capture(&mut self) -> Result<CapturedFrame, PortError> {
        self.gate.enter();
        self.inner.capture()
    }
}

/// A port whose destructor, foreign code from the listener's point of view, says on which
/// thread it runs and then waits for its release.
struct Teardown {
    inner: Box<dyn FrameCapturer>,
    ran: mpsc::Sender<thread::ThreadId>,
    release: Arc<Release>,
}

impl FrameCapturer for Teardown {
    fn name(&self) -> &'static str {
        self.inner.name()
    }

    fn capture(&mut self) -> Result<CapturedFrame, PortError> {
        self.inner.capture()
    }
}

impl Drop for Teardown {
    fn drop(&mut self) {
        let _ = self.ran.send(thread::current().id());
        self.release.pass();
    }
}

struct World {
    desktop: FixtureDesktop,
    clock: Arc<ManualClock>,
    grants: Arc<Grants>,
    gate: Arc<Gate>,
    delivery: Arc<Gate>,
}

impl World {
    fn new() -> Self {
        let clock = Arc::new(ManualClock::new());
        let grants = Arc::new(Grants::default());
        grants.list(&ticket(1), grant(true));
        Self {
            desktop: FixtureDesktop::new(scene(), clock.clone()),
            clock,
            grants,
            gate: Arc::default(),
            delivery: Arc::default(),
        }
    }

    fn broker_with(&self, config: McpConfig, max_clients: usize) -> Broker {
        let ports = Ports {
            capturer: Box::new(Gated {
                inner: Box::new(self.desktop.capturer()),
                gate: Arc::clone(&self.gate),
            }),
            detector: Box::new(self.desktop.detector()),
            recognizer: Box::new(self.desktop.recognizer()),
            accessibility: Box::new(NoAccessibility),
            actuator: Box::new(Live(self.desktop.actuator(), Arc::clone(&self.delivery))),
            policy: Arc::new(Operator),
            clock: self.clock.clone(),
        };
        Broker::with_grant_store(
            EngineConfig::new(SESSION),
            ports,
            config,
            TERM,
            max_clients,
            self.grants.clone(),
        )
        .expect("a valid broker")
    }

    fn broker(&self) -> Broker {
        self.broker_with(McpConfig::default(), 8)
    }

    /// A broker whose capturer, a port the broker owns, says on which thread it is dropped and
    /// then waits for `release`.
    fn broker_with_teardown(
        &self,
        ran: mpsc::Sender<thread::ThreadId>,
        release: Arc<Release>,
    ) -> Broker {
        let ports = Ports {
            capturer: Box::new(Teardown {
                inner: Box::new(Gated {
                    inner: Box::new(self.desktop.capturer()),
                    gate: Arc::clone(&self.gate),
                }),
                ran,
                release,
            }),
            detector: Box::new(self.desktop.detector()),
            recognizer: Box::new(self.desktop.recognizer()),
            accessibility: Box::new(NoAccessibility),
            actuator: Box::new(Live(self.desktop.actuator(), Arc::clone(&self.delivery))),
            policy: Arc::new(Operator),
            clock: self.clock.clone(),
        };
        Broker::with_grant_store(
            EngineConfig::new(SESSION),
            ports,
            McpConfig::default(),
            TERM,
            8,
            self.grants.clone(),
        )
        .expect("a valid broker")
    }

    fn bounded(&self, max_clients: usize) -> Broker {
        self.broker_with(McpConfig::default(), max_clients)
    }

    fn events(&self) -> usize {
        self.desktop.handed_actions().len()
    }
}

// ---- pipes and peers ----

/// A pipe name unique to this process and this call, so that parallel tests and leftovers of
/// other runs cannot collide.
fn unique(tag: &str) -> PipeName {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    PipeName::new(&format!(
        "ommatix-test-{}-{tag}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::SeqCst)
    ))
    .expect("a valid name")
}

fn bind(broker: &Broker, tag: &str) -> PipeServer {
    PipeServer::bind(broker.clone(), unique(tag)).expect("the pipe binds")
}

/// Drops a client stream without the crate's lingering flush. The crate hands a dropped stream
/// that it counts as unflushed to one process-wide thread that flushes it before closing it; a
/// flush waits until the peer has read everything, so one stream whose peer never reads would
/// hold every later stream of the process open behind it, and tests would stall each other.
#[allow(clippy::needless_pass_by_value)] // it takes the stream to close it
fn close(stream: PipeStream) {
    stream.assume_flushed();
}

/// How long [`watchdog`] waits for a test's body: generous against the body's own waits (at most
/// a few [`PATIENCE`]s, and the unmutated pipe target runs all its tests in about 40 s). Its only
/// possible effect is to fail a test.
const WATCHDOG: Duration = Duration::from_secs(120);

/// Runs a test's body on a thread of its own and waits for it at most [`WATCHDOG`], failing the
/// test with a message that names what was awaited when it has not finished. Nothing of the
/// listener runs on the waiting thread, so a defect that blocks the listener's lock or a join
/// cannot block this wait; a body stuck past it is leaked by the failing test. A panic of the
/// body is the test's failure, as if the body had run on the test's thread. On success the
/// runner is joined; no body this guards blocks after it has returned (their holds and gates
/// are released inside the body), so that join waits only for the thread's exit.
fn watchdog(what: &str, body: fn()) {
    let (done, finished) = mpsc::channel();
    let runner = thread::spawn(move || {
        let _ = done.send(std::panic::catch_unwind(body));
    });
    match finished.recv_timeout(WATCHDOG) {
        Ok(Ok(())) => runner.join().expect("the body's thread"),
        Ok(Err(panic)) => std::panic::resume_unwind(panic),
        Err(waited) => panic!("watchdog: {what} did not finish within {WATCHDOG:?} ({waited})"),
    }
}

/// Waits until `until` holds for the server's counts; fails the test after [`PATIENCE`].
fn wait(server: &PipeServer, what: &str, until: impl Fn(&PipeStats) -> bool) -> PipeStats {
    server
        .wait_until(PATIENCE, until)
        .unwrap_or_else(|| panic!("{what}: {:?}", server.stats()))
}

/// A reader over a shared pipe stream.
struct Shared(Arc<PipeStream>);

impl Read for Shared {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        (&*self.0).read(buf)
    }
}

/// One client of the pipe, through the production client.
struct Peer {
    stream: Arc<PipeStream>,
    lines: BufReader<Shared>,
    next_id: u64,
}

impl Peer {
    fn connect(name: &PipeName) -> Self {
        let stream = Arc::new(connect(name, CONNECT_ATTEMPTS, CONNECT_PAUSE).expect("connects"));
        Self {
            lines: BufReader::new(Shared(Arc::clone(&stream))),
            stream,
            next_id: 0,
        }
    }

    /// Connects and sends `handshake` as the first line.
    fn attach(name: &PipeName, handshake: &str) -> Self {
        let peer = Self::connect(name);
        peer.send(format!("{handshake}\n").as_bytes());
        peer
    }

    fn send(&self, bytes: &[u8]) {
        (&*self.stream).write_all(bytes).expect("the pipe takes it");
    }

    /// The next line without its terminator, or `None` at the end of the stream.
    fn line(&mut self) -> Option<String> {
        let mut line = Vec::new();
        match self.lines.read_until(b'\n', &mut line) {
            Ok(0) | Err(_) => None,
            Ok(_) => {
                assert_eq!(line.pop(), Some(b'\n'), "a whole line");
                Some(String::from_utf8(line).expect("UTF-8"))
            }
        }
    }

    /// Sends one message and reads its one reply.
    fn exchange(&mut self, message: &str) -> Value {
        self.send(format!("{message}\n").as_bytes());
        let line = self.line().expect("a reply");
        serde_json::from_str(&line).expect("JSON")
    }

    /// A tool call's whole response.
    fn call(&mut self, tool: &str, arguments: &Value) -> Value {
        self.next_id += 1;
        let id = self.next_id;
        self.exchange(&tool_line(id, tool, arguments))
    }

    /// `tools/list`, a request every admitted client is answered.
    fn list(&mut self) -> Value {
        self.next_id += 1;
        let id = self.next_id;
        self.exchange(
            &json!({ "jsonrpc": "2.0", "id": id, "method": "tools/list",
                     "params": { "_meta": modern_meta() } })
            .to_string(),
        )
    }
}

impl Drop for Peer {
    fn drop(&mut self) {
        // See `close`: the handle closes now, and the server sees the client leave.
        self.stream.assume_flushed();
    }
}

fn tool_line(id: u64, name: &str, arguments: &Value) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call",
            "params": { "_meta": modern_meta(), "name": name, "arguments": arguments } })
    .to_string()
}

fn padded_request(id: u64, len: usize) -> String {
    let base = json!({ "jsonrpc": "2.0", "id": id, "method": "tools/list",
                       "params": { "_meta": modern_meta() } })
    .to_string();
    let mut padded = base[..base.len() - 1].to_owned();
    padded.push_str(&" ".repeat(len - base.len()));
    padded.push('}');
    padded
}

fn click(page: &Value, request_id: &str, live: bool) -> Value {
    let button = page["elements"]
        .as_array()
        .expect("a page")
        .iter()
        .find(|e| e["role"] == "button")
        .unwrap_or_else(|| panic!("no button in {page}"));
    json!({
        "request_id": request_id,
        "session_id": page["session_id"],
        "snapshot_id": page["snapshot_id"],
        "geometry_epoch": page["geometry_epoch"],
        "target_id": button["target_id"],
        "expected_revision": button["revision"],
        "action": "click",
        "dry_run": !live
    })
}

fn clients(broker: &Broker) -> usize {
    broker.retained().expect("available").clients.len()
}

// ---- 1. a session through the pipe is the session over in-memory streams ----

/// A scripted session: observations, a dry and a live click, the live click's retry, metrics,
/// invalid arguments, a malformed line, a notification, a blank line, an oversized line and a
/// line with a byte-order mark; `\r\n` and `\n` alternate.
fn scripted_session() -> Vec<u8> {
    let prefix = [
        padded_request(1, 300),
        tool_line(2, "observe_screen", &json!({ "limit": 2 })),
        tool_line(3, "observe_screen", &json!({ "include_values": true })),
    ];
    let world = World::new();
    let broker = world.broker();
    let mut probe = broker.connect(Some(grant(true))).unwrap();
    let mut last = None;
    for line in &prefix {
        last = probe.connection().handle_message(line.as_bytes());
    }
    let page =
        serde_json::from_str::<Value>(&last.unwrap()).unwrap()["result"]["structuredContent"]
            .clone();
    let mut lines = prefix.to_vec();
    lines.extend([
        tool_line(4, "execute_action", &click(&page, "dry", false)),
        tool_line(5, "execute_action", &click(&page, "live", true)),
        tool_line(6, "execute_action", &click(&page, "live", true)),
        tool_line(7, "get_engine_metrics", &json!({})),
        tool_line(8, "execute_action", &json!({ "x": 1 })),
        "not json".to_owned(),
        r#"{"jsonrpc":"2.0","method":"notifications/cancelled"}"#.to_owned(),
        "   ".to_owned(),
        "x".repeat(DEFAULT_MAX_MESSAGE_BYTES + 10),
        format!("\u{feff}{}", padded_request(9, 300)),
    ]);
    let mut input = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        input.extend_from_slice(line.as_bytes());
        input.extend_from_slice(if i % 2 == 0 { b"\r\n" } else { b"\n" });
    }
    input
}

#[test]
fn it_27_a_ticket_client_through_the_pipe_gets_byte_for_byte_what_in_memory_streams_get() {
    let mut input = format!("{}\n", ticket(1)).into_bytes();
    input.extend_from_slice(&scripted_session());

    let memory_world = World::new();
    let memory = memory_world.broker();
    let mut expected = Vec::new();
    let admission = memory
        .serve_sync_with_handshake(io::Cursor::new(input.clone()), &mut expected)
        .unwrap();
    assert_eq!(admission, Admission::Served);
    assert_eq!(String::from_utf8_lossy(&expected).lines().count(), 11);

    let world = World::new();
    let broker = world.broker();
    let mut server = bind(&broker, "session");
    let peer = Peer::connect(server.name());
    let writer = {
        let stream = Arc::clone(&peer.stream);
        thread::spawn(move || (&*stream).write_all(&input))
    };
    let mut got = vec![0_u8; expected.len()];
    Shared(Arc::clone(&peer.stream))
        .read_exact(&mut got)
        .unwrap();
    writer.join().unwrap().unwrap();
    assert!(
        got == expected,
        "the pipe's bytes differ from the in-memory bytes"
    );
    assert_eq!(world.events(), memory_world.events());
    assert_eq!(world.events(), 1, "one delivery");
    assert_eq!(clients(&broker), 1);
    drop(peer);
    let stats = wait(&server, "the client leaves", |s| s.connections == 0);
    assert_eq!((stats.accepted, stats.admitted, stats.failed), (1, 1, 0));
    let retained = broker.retained().unwrap();
    assert!(retained.clients.is_empty());
    assert_eq!(retained.engine["snapshots"], 0, "the last client purged");
    assert!(server.shutdown(PATIENCE).complete());
}

// ---- 2. one listener per name ----

#[test]
fn it_28_a_second_listener_on_the_same_name_fails_and_a_new_one_binds_once_the_first_is_gone() {
    let world = World::new();
    let broker = world.broker();
    let mut first = bind(&broker, "second");
    let name = first.name().clone();
    let other_world = World::new();
    assert_eq!(
        PipeServer::bind(other_world.broker(), name.clone()).err(),
        Some(PipeError::NameTaken)
    );
    // The first is unharmed.
    let mut peer = Peer::attach(&name, "");
    assert!(peer.list()["result"].is_object());
    drop(peer);
    assert!(first.shutdown(PATIENCE).complete());
    drop(first);
    let mut again = PipeServer::bind(other_world.broker(), name.clone()).expect("binds again");
    let mut peer = Peer::attach(&name, "");
    assert!(peer.list()["result"].is_object());
    drop(peer);
    assert!(again.shutdown(PATIENCE).complete());
}

// ---- 3. the descriptor ----

/// What the test's own PowerShell reads about the pipe and about the token it runs with (the
/// test process's token: the child inherits it, and the pipe was created under it).
///
/// The descriptor is read through a connected client handle of .NET
/// (`System.IO.Pipes.NamedPipeClientStream` and `GetAccessControl()`), as the experiment read
/// the default pipe's: no path provider is involved. A client connection consumes the one
/// listening instance, so the read is retried while the instance is busy; the 100 ms of each
/// `Connect` only spaces the retries and decides nothing: the test asserts the descriptor it
/// finally reads, and fails, saying why, if none is read. `Get-Acl` is then tried as well, only
/// to report what it does on this machine: on CI's windows-latest (run 45814291197) Windows
/// PowerShell could not load its type data for `ObjectSecurity` ("The member Sddl is already
/// present") and never read the pipe; here it reads it at the second try.
///
/// Every line is `key=value`; the keys are those of [`DESCRIPTOR_KEYS`].
const DESCRIPTOR_SCRIPT: &str = r"
$leaf = '{LEAF}'; $p = '{PATH}'; $dir = $p.Substring(0, $p.Length - $leaf.Length)
function Show-Error($e) {
  while ($null -ne $e.InnerException) { $e = $e.InnerException }
  ($e.GetType().FullName + ' (0x' + $e.HResult.ToString('X8') + '): ' + $e.Message) -replace '\s+', ' '
}
'ps_edition=' + $PSVersionTable.PSEdition
'ps_version=' + $PSVersionTable.PSVersion
'os=' + [Environment]::OSVersion.VersionString
$id = [Security.Principal.WindowsIdentity]::GetCurrent()
'user_sid=' + $id.User.Value
'token_default_owner_sid=' + $id.Owner.Value
'administrators_enabled=' + ([Security.Principal.WindowsPrincipal]$id).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
try { 'integrity_sid=' + [regex]::Match(((whoami /groups) -join ' '), 'S-1-16-\d+').Value } catch { 'integrity_sid=error: ' + (Show-Error $_.Exception) }
try { 'pipe_listed=' + (@([IO.Directory]::GetFiles($dir)) -contains $p) } catch { 'pipe_listed=error: ' + (Show-Error $_.Exception) }
$read = $false; $last = 'none'; $tries = 0
for ($i = 0; $i -lt 150 -and -not $read; $i++) {
  $tries = $i + 1
  $c = New-Object System.IO.Pipes.NamedPipeClientStream('.', $leaf, [System.IO.Pipes.PipeDirection]::InOut)
  try {
    $c.Connect(100)
    $s = $c.GetAccessControl()
    'sddl=' + $s.GetSecurityDescriptorSddlForm('All')
    'access=' + $s.GetSecurityDescriptorSddlForm('Access')
    'owner=' + $s.GetSecurityDescriptorSddlForm('Owner')
    'owner_sid=' + $s.GetOwner([Security.Principal.SecurityIdentifier]).Value
    $read = $true
  } catch { $last = Show-Error $_.Exception } finally { $c.Dispose() }
}
'dotnet_tries=' + $tries
'dotnet_last_error=' + $(if ($read) { 'none' } else { $last })
$acl = 'not read'; $last = 'none'; $tries = 0
for ($i = 0; $i -lt 20; $i++) {
  $tries = $i + 1
  try { $a = Get-Acl -LiteralPath $p -ErrorAction Stop; $acl = $a.GetSecurityDescriptorSddlForm('Access') + ' ' + $a.GetSecurityDescriptorSddlForm('Owner'); break }
  catch { $last = Show-Error $_.Exception; Start-Sleep -Milliseconds 100 }
}
'get_acl=' + $acl + ' after ' + $tries + ' tries; last error: ' + $last
if ($read) { exit 0 } else { exit 1 }
";

/// The keys [`DESCRIPTOR_SCRIPT`] prints, in its order.
const DESCRIPTOR_KEYS: [&str; 15] = [
    "ps_edition",
    "ps_version",
    "os",
    "user_sid",
    "token_default_owner_sid",
    "administrators_enabled",
    "integrity_sid",
    "pipe_listed",
    "sddl",
    "access",
    "owner",
    "owner_sid",
    "dotnet_tries",
    "dotnet_last_error",
    "get_acl",
];

/// Reads the pipe's descriptor and the token's facts with [`DESCRIPTOR_SCRIPT`], writes every
/// line to the test's standard error (straight to the handle, so that it is in the log of a
/// passing run too; never to a file), and fails, with everything the script printed, if the
/// descriptor was not read.
fn descriptor_of(name: &PipeName) -> BTreeMap<String, String> {
    let script = DESCRIPTOR_SCRIPT
        .replace("{LEAF}", name.name())
        .replace("{PATH}", name.path());
    let output = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .expect("PowerShell runs");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let mut log = io::stderr().lock();
    for line in stdout.lines().filter(|l| !l.trim().is_empty()) {
        let _ = writeln!(log, "pipe-descriptor: {}", line.trim());
    }
    if !stderr.trim().is_empty() {
        let _ = writeln!(log, "pipe-descriptor: powershell stderr: {}", stderr.trim());
    }
    drop(log);
    let facts: BTreeMap<String, String> = stdout
        .lines()
        .filter_map(|l| l.trim().split_once('='))
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect();
    assert!(
        output.status.success(),
        "the pipe's descriptor was never read (exit {:?}).\nstdout:\n{stdout}\nstderr:\n{stderr}",
        output.status.code()
    );
    for key in DESCRIPTOR_KEYS {
        assert!(facts.contains_key(key), "no `{key}` in:\n{stdout}");
    }
    facts
}

/// `BUILTIN\Administrators`, the default owner Windows gives the objects of an elevated token.
const ADMINISTRATORS_SID: &str = "S-1-5-32-544";

/// The integrity level from which a token counts as elevated (`S-1-16-12288`, High).
const HIGH_INTEGRITY_RID: u32 = 12288;

/// Whether the token the pipe was created under is elevated: its integrity level is High or
/// above, or the Administrators group is enabled in it. Fails if either fact was not read.
fn elevated(facts: &BTreeMap<String, String>) -> bool {
    let integrity: u32 = facts["integrity_sid"]
        .strip_prefix("S-1-16-")
        .and_then(|rid| rid.parse().ok())
        .unwrap_or_else(|| panic!("the token's integrity level was not read; {facts:?}"));
    let administrators = match facts["administrators_enabled"].as_str() {
        "True" => true,
        "False" => false,
        other => panic!("`administrators_enabled` is {other:?}; {facts:?}"),
    };
    integrity >= HIGH_INTEGRITY_RID || administrators
}

#[test]
fn sec_08_the_pipe_s_descriptor_grants_its_owner_only_and_the_owner_is_this_user() {
    let world = World::new();
    let broker = world.broker();
    let mut server = bind(&broker, "acl");
    let facts = descriptor_of(server.name());

    // The shape, whatever the token: a protected DACL (`P`: nothing inherited from a parent)
    // with exactly the one entry the server writes, an allow of every right to OWNER RIGHTS,
    // with no flags (nothing inheritable), and no SACL; so nothing for Everyone (`WD`),
    // Authenticated Users (`AU`), the network (`NU`) or anonymous logons (`AN`).
    assert_eq!(
        facts["access"], "D:P(A;;FA;;;OW)",
        "one protected entry, for the owner; {facts:?}"
    );
    assert!(
        facts["sddl"].ends_with("D:P(A;;FA;;;OW)"),
        "the whole descriptor holds that DACL and nothing after it; {facts:?}"
    );
    for (alias, sid) in [
        ("WD", "S-1-1-0"),
        ("AU", "S-1-5-11"),
        ("NU", "S-1-5-2"),
        ("AN", "S-1-5-7"),
    ] {
        // An entry's trustee is the last field before its `)`, so the whole field is matched:
        // `S-1-5-2` is also the start of every local account's SID.
        assert!(
            !facts["sddl"].contains(&format!(";{alias})"))
                && !facts["sddl"].contains(&format!(";{sid})")),
            "nothing for {alias}; {facts:?}"
        );
    }
    // The server sets no owner, so the pipe's owner is the token's default owner.
    assert_eq!(
        facts["owner_sid"], facts["token_default_owner_sid"],
        "the owner is the token's default owner; {facts:?}"
    );

    if elevated(&facts) {
        // An elevated engine (seen on CI's windows-latest, run 45814291197: Windows Server 2025,
        // the built-in Administrator, integrity High, owner `O:BA`). Windows may give the pipe
        // to `BUILTIN\Administrators` instead of the user; `OW` then grants it to every process
        // whose token has the Administrators group enabled, every elevated process of every
        // administrator of the machine, and not to the user alone. That is not the narrow case
        // and the test does not treat it as one: it checks that the owner is one of the two
        // owners Windows can give and says which. Whether an elevated engine may serve the pipe
        // at all is ADR-0043's open point (decision 6, "how the engine refuses to start the
        // broker when elevated"), the owner's decision; the server is unchanged here.
        let owner = &facts["owner_sid"];
        let which = if *owner == facts["user_sid"] {
            "the user"
        } else if owner == ADMINISTRATORS_SID {
            "BUILTIN\\Administrators: owner-only means every elevated administrator"
        } else {
            panic!(
                "an elevated engine's pipe is owned by neither the user nor Administrators; {facts:?}"
            )
        };
        let _ = writeln!(
            io::stderr().lock(),
            "pipe-descriptor: elevated token; the pipe's owner is {which} ({owner})"
        );
    } else {
        assert_eq!(
            facts["owner_sid"], facts["user_sid"],
            "an engine that is not elevated owns it; {facts:?}"
        );
    }
    assert_eq!(clients(&broker), 0, "the readers were never admitted");
    assert!(server.shutdown(PATIENCE).complete());
}

// ---- 4. the squatter ----

/// Plays the squatter of ADR-0043's required validation: a pipe server created under a fresh
/// name with the crate's defaults, which reads one line from the client `open` connects, then
/// impersonates that client and opens a file the user can read. Returns that open's result.
fn squat(open: impl FnOnce(&PipeName) -> PipeStream) -> io::Result<()> {
    let name = unique("squat");
    let squatter: Listener = PipeListenerOptions::new()
        .path(name.path())
        .create_duplex::<pipe_mode::Bytes>()
        .expect("the squatter takes the name");
    let target = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    assert!(
        std::fs::File::open(&target).is_ok(),
        "the test itself reads it"
    );
    let server = thread::spawn(move || -> io::Result<()> {
        let stream = squatter.accept()?;
        // Impersonation needs a read first.
        let mut line = String::new();
        BufReader::new(&stream).read_line(&mut line)?;
        let guard = stream.impersonate_client()?;
        let opened = std::fs::File::open(&target).map(drop);
        drop(guard);
        opened
    });
    let client = open(&name);
    (&client).write_all(b"ticket line\n").unwrap();
    let opened = server.join().expect("the squatter's thread");
    close(client);
    opened
}

#[test]
fn sec_09_a_squatter_cannot_act_as_the_production_client_but_can_as_a_client_without_quality_of_service()
 {
    const ERROR_BAD_IMPERSONATION_LEVEL: i32 = 1346;
    let production = squat(|name| connect(name, CONNECT_ATTEMPTS, CONNECT_PAUSE).unwrap());
    assert_eq!(
        production.as_ref().map_err(io::Error::raw_os_error).err(),
        Some(Some(ERROR_BAD_IMPERSONATION_LEVEL)),
        "{production:?}"
    );
    // The positive control: the crate's own client sets no quality of service, and the squatter
    // then opens the file as the user.
    let control = squat(|name| PipeStream::connect_by_path(name.path()).unwrap());
    assert!(control.is_ok(), "{control:?}");
}

// ---- 5. the bound on clients ----

#[test]
fn it_29_connections_beyond_the_bound_are_refused_at_accept_and_never_stall_it() {
    let world = World::new();
    let broker = world.bounded(2);
    let mut server = bind(&broker, "bound");
    let name = server.name().clone();
    // Two connections that send nothing fill the bound: nobody is admitted.
    let silent = [Peer::connect(&name), Peer::connect(&name)];
    wait(&server, "two silent places", |s| s.connections == 2);
    // The next one is told the engine is full and closed, before anything is read from it.
    let mut over = Peer::connect(&name);
    over.send(format!("{}\n", ticket(1)).as_bytes());
    assert_eq!(over.line().as_deref(), Some(FULL_LINE));
    assert_eq!(over.line(), None);
    assert_eq!(clients(&broker), 0, "nothing was admitted");
    // When a silent one leaves, its place is free again.
    let [first, second] = silent;
    drop(first);
    wait(&server, "a place freed", |s| s.connections == 1);
    let mut admitted = Peer::attach(&name, &ticket(1));
    assert!(admitted.list()["result"].is_object());
    assert_eq!(clients(&broker), 1);
    // Refused peers that never read hold only their own short-lived threads, up to the bound
    // on refusals; past it, a refused peer is closed with no line; and accepting goes on.
    let stalled: Vec<Peer> = (0..MAX_REFUSING).map(|_| Peer::connect(&name)).collect();
    wait(&server, "refusals waiting on readers", |s| {
        s.refusing == MAX_REFUSING
    });
    let mut closed = Peer::connect(&name);
    assert_eq!(closed.line(), None, "closed with no line");
    let stats = wait(&server, "closed with no line", |s| s.closed_full == 1);
    assert_eq!(stats.told_full, 1 + MAX_REFUSING as u64);
    assert_eq!(stats.connections, 2);
    // The refusals consumed no client entry, and the admitted client is still served.
    assert_eq!(clients(&broker), 1);
    assert!(admitted.list()["result"].is_object());
    drop(second);
    wait(&server, "the second silent one leaves", |s| {
        s.connections == 1
    });
    let mut late = Peer::attach(&name, "");
    assert!(late.list()["result"].is_object(), "the place came back");
    assert_eq!(clients(&broker), 2);
    drop(stalled);
    wait(&server, "the refusals end", |s| s.refusing == 0);
    drop((admitted, late));
    assert!(server.shutdown(PATIENCE).complete());
    assert_eq!(clients(&broker), 0);
}

#[test]
fn it_24_a_peer_that_streams_a_handshake_without_end_is_dropped_after_a_bounded_read_and_its_place_freed()
 {
    let world = World::new();
    let broker = world.bounded(1);
    let mut server = bind(&broker, "endless");
    let name = server.name().clone();
    let bound = broker.handshake_read_bound();
    let endless = Peer::connect(&name);
    let writer = {
        let stream = Arc::clone(&endless.stream);
        thread::spawn(move || {
            let chunk = [b'x'; 4096];
            let mut written = 0_usize;
            // Stops when the server closes the pipe; the cap only keeps a wrong server from
            // hanging the test.
            while written < 64 * bound {
                if (&*stream).write_all(&chunk).is_err() {
                    return (written, true);
                }
                written += chunk.len();
            }
            (written, false)
        })
    };
    let (written, cut) = writer.join().unwrap();
    assert!(cut, "the server read on: {written} bytes written");
    assert!(
        written <= 4 * bound,
        "{written} bytes taken for a bound of {bound}"
    );
    // Its place comes back once the reaper has joined its worker, after the refusal is counted.
    wait(
        &server,
        "the endless peer is refused and its place is back",
        |s| s.refused == 1 && s.connections == 0,
    );
    drop(endless);
    // Its place, the only one, is free again.
    let mut next = Peer::attach(&name, &ticket(1));
    assert!(next.list()["result"].is_object());
    drop(next);
    assert!(server.shutdown(PATIENCE).complete());
}

// ---- 6. handshakes that admit nobody ----

#[test]
fn it_24_a_bad_handshake_admits_nobody_and_the_listener_keeps_serving_the_others() {
    let world = World::new();
    let broker = world.broker();
    let mut server = bind(&broker, "handshake");
    let name = server.name().clone();
    let mut keeper = Peer::attach(&name, &ticket(1));
    assert!(keeper.list()["result"].is_object());
    let oversized = "x".repeat(DEFAULT_MAX_MESSAGE_BYTES + 1);
    let cases: [(&str, Vec<u8>); 4] = [
        ("oversized", format!("{oversized}\n").into_bytes()),
        ("malformed", b"not a ticket\n".to_vec()),
        (
            "a message instead",
            format!("{}\n", padded_request(1, 300)).into_bytes(),
        ),
        ("cut short", b"Pp7-Tkt_000000000000000".to_vec()),
    ];
    for (n, (case, bytes)) in cases.into_iter().enumerate() {
        let mut peer = Peer::connect(&name);
        peer.send(&bytes);
        if case == "cut short" {
            drop(peer);
        } else {
            assert_eq!(peer.line(), None, "{case}: closed with nothing written");
        }
        let stats = wait(&server, case, |s| s.refused == n as u64 + 1);
        assert_eq!(stats.admitted, 0, "{case}");
        assert_eq!(clients(&broker), 1, "{case}: nobody admitted");
        assert!(
            keeper.list()["result"].is_object(),
            "{case}: the others served"
        );
    }
    drop(keeper);
    wait(&server, "the keeper leaves", |s| s.connections == 0);
    assert!(server.shutdown(PATIENCE).complete());
}

// ---- 7. lines after the handshake ----

#[test]
fn it_27_an_oversized_line_is_answered_and_a_client_leaving_mid_line_disturbs_nobody() {
    let world = World::new();
    let broker = world.broker();
    let mut server = bind(&broker, "lines");
    let name = server.name().clone();
    let mut a = Peer::attach(&name, &ticket(1));
    let page = a.call("observe_screen", &json!({}));
    assert!(page["result"]["structuredContent"]["elements"].is_array());
    let oversized = a.exchange(&"x".repeat(DEFAULT_MAX_MESSAGE_BYTES + 10));
    assert_eq!(error_code(&oversized), -32600);
    assert!(a.list()["result"].is_object(), "the connection survives");
    let mut b = Peer::attach(&name, "");
    assert!(b.list()["result"].is_object());
    b.send(br#"{"jsonrpc":"2.0","id":7,"method":"tools/li"#);
    drop(b);
    wait(&server, "b leaves", |s| s.connections == 1);
    assert_eq!(clients(&broker), 1);
    assert!(a.list()["result"].is_object(), "a is not disturbed");
    assert!(broker.retained().unwrap().engine["snapshots"] > 0);
    drop(a);
    wait(&server, "a leaves", |s| s.connections == 0);
    let retained = broker.retained().unwrap();
    assert!(retained.clients.is_empty());
    assert_eq!(retained.engine["snapshots"], 0, "the last one purged");
    assert!(server.shutdown(PATIENCE).complete());
}

// ---- 8. eight clients at once ----

struct Rng(u64);

impl Rng {
    fn below(&mut self, n: u64) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        (z ^ (z >> 31)) % n
    }
}

/// What a client of the concurrent run recorded: every tool result it received.
struct Record {
    may_deliver: bool,
    results: Vec<Value>,
}

#[test]
#[allow(clippy::too_many_lines)] // one run: the clients, then the checks of its record
fn it_27_eight_clients_through_the_pipe_at_once_keep_the_brokers_invariants() {
    const STEPS: usize = 60;
    let world = World::new();
    for n in 10..18 {
        world.grants.list(&ticket(n), grant(n < 14));
    }
    let broker = world.broker();
    let mut server = bind(&broker, "eight");
    let name = server.name().clone();
    let start = Arc::new(Barrier::new(8));
    // The turn of the closing phase: which delivering client acts alone.
    let turn = Arc::new((Mutex::new(0_u64), std::sync::Condvar::new()));
    let threads: Vec<_> = (0..8_u64)
        .map(|i| {
            let (name, start, turn) = (name.clone(), Arc::clone(&start), Arc::clone(&turn));
            thread::spawn(move || {
                // Clients 0-3 may deliver, 4-5 read only, 6 has no ticket, 7 an unlisted one.
                let handshake = match i {
                    6 => String::new(),
                    7 => ticket(99),
                    _ => ticket(10 + i),
                };
                let mut peer = Peer::attach(&name, &handshake);
                assert!(peer.list()["result"].is_object());
                start.wait();
                let mut rng = Rng(0x5EED_0000 + i);
                let mut page: Option<Value> = None;
                let mut results = Vec::new();
                for step in 0..STEPS {
                    let mut responses = Vec::new();
                    match rng.below(10) {
                        0..=2 => responses.push(peer.call("observe_screen", &json!({}))),
                        3 => responses
                            .push(peer.call("observe_screen", &json!({ "lease": "acquire" }))),
                        4 => responses
                            .push(peer.call("observe_screen", &json!({ "lease": "release" }))),
                        // A cycle: ask for the lease, act live on the page it gives, release.
                        5..=7 => {
                            let asked = peer.call("observe_screen", &json!({ "lease": "acquire" }));
                            let fresh = asked["result"]["structuredContent"].clone();
                            responses.push(asked);
                            if fresh["elements"].is_array() {
                                let live = click(&fresh, &format!("c{i}-{step}"), true);
                                responses.push(peer.call("execute_action", &live));
                            }
                            responses
                                .push(peer.call("observe_screen", &json!({ "lease": "release" })));
                        }
                        8 if page.is_some() => responses.push(peer.call(
                            "execute_action",
                            &click(page.as_ref().unwrap(), &format!("s{i}-{step}"), true),
                        )),
                        9 if page.is_some() => responses.push(peer.call(
                            "execute_action",
                            &click(page.as_ref().unwrap(), &format!("d{i}-{step}"), false),
                        )),
                        _ => responses.push(peer.call("get_engine_metrics", &json!({}))),
                    }
                    for response in responses {
                        let result = response
                            .get("result")
                            .unwrap_or_else(|| panic!("a tool result: {response}"))
                            .clone();
                        let structured = &result["structuredContent"];
                        if structured["elements"].is_array() {
                            page = Some(structured.clone());
                        }
                        results.push(result);
                    }
                }
                // The closing phase, the same for every run: everyone lets the lease go, then
                // the four delivering clients take turns, each alone, to ask for the lease, act
                // live and release, so the run always has deliveries for the checks to bite on.
                let released = peer.call("observe_screen", &json!({ "lease": "release" }));
                results.push(released["result"].clone());
                start.wait();
                if i < 4 {
                    let (number, next) = &*turn;
                    let mut current = number.lock().unwrap_or_else(PoisonError::into_inner);
                    while *current != i {
                        current = next.wait(current).unwrap_or_else(PoisonError::into_inner);
                    }
                    let asked = peer.call("observe_screen", &json!({ "lease": "acquire" }));
                    let fresh = asked["result"]["structuredContent"].clone();
                    let acted = peer.call("execute_action", &click(&fresh, &format!("t{i}"), true));
                    assert_eq!(
                        acted["result"]["structuredContent"]["input_delivered"], true,
                        "{acted}"
                    );
                    let back = peer.call("observe_screen", &json!({ "lease": "release" }));
                    results.extend([asked, acted, back].map(|r| r["result"].clone()));
                    *current += 1;
                    next.notify_all();
                }
                // Every client stays connected until all are done: with several clients the
                // broker reports where each stands under the lease (D27), which this checks.
                start.wait();
                Record {
                    may_deliver: i < 4,
                    results,
                }
            })
        })
        .collect();
    let records: Vec<Record> = threads.into_iter().map(|t| t.join().unwrap()).collect();
    let mut deliveries = 0;
    let mut refused = 0;
    let mut snapshots = BTreeSet::new();
    for record in &records {
        for result in &record.results {
            let structured = &result["structuredContent"];
            if structured["input_delivered"] == true {
                deliveries += 1;
                assert!(record.may_deliver, "a delivery without a grant to deliver");
                assert_eq!(
                    result["_meta"][LEASE_META_KEY]["state"], "holder",
                    "a delivery by a client that does not hold the lease: {result}"
                );
                assert!(
                    snapshots.insert(structured["snapshot_id"].to_string()),
                    "two deliveries on one snapshot: {structured}"
                );
            } else if structured["error"]["code"].is_string() {
                refused += 1;
            }
        }
    }
    assert_eq!(
        world.events(),
        deliveries,
        "the actuator's events are the deliveries"
    );
    assert!(
        deliveries >= 4,
        "{deliveries} deliveries: the run says too little"
    );
    assert!(refused >= 10, "{refused} refusals: the run says too little");
    // Each client left when its thread ended; the last one purged.
    let stats = wait(&server, "the clients leave", |s| s.connections == 0);
    assert_eq!((stats.accepted, stats.admitted, stats.failed), (8, 8, 0));
    let retained = broker.retained().unwrap();
    assert!(retained.clients.is_empty());
    assert_eq!(retained.engine["snapshots"], 0);
    assert!(server.shutdown(PATIENCE).complete());
}

// ---- 9. the client ----

#[test]
fn it_32_an_absent_pipe_is_no_engine_and_a_busy_one_is_retried_a_bounded_number_of_times() {
    let started = Instant::now();
    assert_eq!(
        connect(&unique("absent"), CONNECT_ATTEMPTS, CONNECT_PAUSE).err(),
        Some(ConnectError::NoEngine)
    );
    assert!(
        started.elapsed() < CONNECT_PAUSE * 4,
        "no retry for an absent pipe"
    );

    // Every instance busy: a listener of two instances, one accepted and one connected.
    let name = unique("busy");
    let listener: Listener = PipeListenerOptions::new()
        .path(name.path())
        .instance_limit(NonZeroU8::new(2))
        .create_duplex::<pipe_mode::Bytes>()
        .unwrap();
    let first = PipeStream::connect_by_path(name.path()).unwrap();
    let accepted = listener.accept().unwrap();
    let second = PipeStream::connect_by_path(name.path()).unwrap();
    let started = Instant::now();
    assert_eq!(
        connect(&name, 5, Duration::from_millis(20)).err(),
        Some(ConnectError::Busy)
    );
    let waited = started.elapsed();
    assert!(
        waited >= Duration::from_millis(80),
        "four pauses: {waited:?}"
    );
    assert!(waited < Duration::from_secs(5), "bounded: {waited:?}");

    // A busy pipe that frees an instance while the client retries: it connects. The pipe stays
    // busy until the client has told of a try that found it busy, so the retry is what connects
    // it; no pause of the test decides that.
    let (busy_try, is_busy) = mpsc::channel();
    let retrying = {
        let name = name.clone();
        thread::spawn(move || {
            connect_with(&name, 400, Duration::from_millis(10), |tries| {
                let _ = busy_try.send(tries);
            })
            .map(close)
        })
    };
    let first_busy = is_busy
        .recv_timeout(PATIENCE)
        .expect("the client tells of a busy try");
    assert_eq!(first_busy, 1, "the first try found it busy");
    close(first);
    close(accepted);
    // Accepting the second connection makes the listener create a free instance.
    let second_served = listener.accept().unwrap();
    assert_eq!(retrying.join().unwrap(), Ok(()), "connected by a retry");
    close(second);
    close(second_served);
}

// ---- 10. the relay ----

/// The caller's input, fed by the test through a channel; it ends when the sender is dropped.
struct ChannelInput {
    chunks: mpsc::Receiver<Vec<u8>>,
    pending: Vec<u8>,
}

impl Read for ChannelInput {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.pending.is_empty() {
            match self.chunks.recv() {
                Ok(chunk) => self.pending = chunk,
                Err(_) => return Ok(0),
            }
        }
        let n = buf.len().min(self.pending.len());
        buf[..n].copy_from_slice(&self.pending[..n]);
        self.pending.drain(..n);
        Ok(n)
    }
}

/// The caller's output, read by the test through a channel.
struct ChannelOutput(mpsc::Sender<Vec<u8>>);

impl Write for ChannelOutput {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0
            .send(buf.to_vec())
            .map_err(|_| io::Error::from(io::ErrorKind::BrokenPipe))?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// The experiment's load: request `i` asks for a reply whose length depends on `i`, up to
/// 60 000 bytes; every seventh is a notification and gets none.
fn request(i: usize) -> Vec<u8> {
    let kind = if i % 7 == 3 { 'N' } else { 'Q' };
    let pad = "p".repeat(i % 211);
    format!("{kind} {i} {} {pad}\n", reply_len(i)).into_bytes()
}

fn reply_len(i: usize) -> usize {
    if i.is_multiple_of(53) {
        60_000
    } else {
        10 + (i * 97) % 2_000
    }
}

fn reply(i: usize) -> Vec<u8> {
    let mut line = format!("A {i} ").into_bytes();
    line.resize(line.len() + reply_len(i), b'r');
    line.push(b'\n');
    line
}

/// A test engine on the pipe: after the handshake line, reads `requests` lines and answers each
/// `Q` line with its reply; then waits for `close` before the caller closes the pipe.
fn echo_engine(
    stream: &PipeStream,
    requests: usize,
    close: &mpsc::Receiver<()>,
) -> io::Result<(String, usize)> {
    let mut reader = BufReader::new(stream);
    let mut writer = stream;
    let mut handshake = String::new();
    reader.read_line(&mut handshake)?;
    let mut answered = 0;
    let mut line = String::new();
    for _ in 0..requests {
        line.clear();
        reader.read_line(&mut line)?;
        let mut words = line.split(' ');
        if words.next() == Some("Q") {
            let i: usize = words.next().unwrap().parse().unwrap();
            writer.write_all(&reply(i))?;
            answered += 1;
        }
    }
    writer.flush()?;
    let _ = close.recv();
    Ok((handshake, answered))
}

#[test]
fn it_31_the_relay_carries_pipelined_requests_and_large_replies_both_ways_at_once() {
    const REQUESTS: usize = 10_000;
    let name = unique("relay");
    let listener: Listener = PipeListenerOptions::new()
        .path(name.path())
        .create_duplex::<pipe_mode::Bytes>()
        .unwrap();
    let (close, closed) = mpsc::channel();
    let engine = thread::spawn(move || {
        let stream = listener.accept().unwrap();
        let outcome = echo_engine(&stream, REQUESTS, &closed);
        drop((stream, listener));
        outcome
    });
    let pipe = Arc::new(connect(&name, CONNECT_ATTEMPTS, CONNECT_PAUSE).unwrap());
    let (to_relay, input) = mpsc::channel();
    let (output, from_relay) = mpsc::channel();
    let relayed = thread::spawn(move || {
        relay(
            pipe,
            b"",
            ChannelInput {
                chunks: input,
                pending: Vec::new(),
            },
            ChannelOutput(output),
        )
    });
    // Everything is written without waiting for a reply.
    let writer = thread::spawn(move || {
        for i in 0..REQUESTS {
            to_relay.send(request(i)).unwrap();
        }
        to_relay
    });
    let mut expected = Vec::new();
    for i in (0..REQUESTS).filter(|i| i % 7 != 3) {
        expected.extend_from_slice(&reply(i));
    }
    let mut got = Vec::with_capacity(expected.len());
    while got.len() < expected.len() {
        got.extend(
            from_relay
                .recv_timeout(PATIENCE)
                .expect("replies keep coming"),
        );
    }
    assert!(got == expected, "every reply, in order");
    // The input ends: the relay reports it, while the engine still holds the pipe open.
    drop(writer.join().unwrap());
    assert_eq!(relayed.join().unwrap(), Ok(RelayEnd::InputEnded));
    // The direction from the engine, which the relay does not wait for, ends with its source.
    close.send(()).unwrap();
    let (handshake, answered) = engine.join().unwrap().unwrap();
    assert_eq!(handshake, "\n");
    assert_eq!(answered, expected_replies(REQUESTS));
}

fn expected_replies(requests: usize) -> usize {
    (0..requests).filter(|i| i % 7 != 3).count()
}

#[test]
fn it_31_the_relay_reports_the_engine_closing_the_pipe_and_refuses_a_bad_handshake_unwritten() {
    let name = unique("relay-end");
    let listener: Listener = PipeListenerOptions::new()
        .path(name.path())
        .create_duplex::<pipe_mode::Bytes>()
        .unwrap();
    let engine = thread::spawn(move || {
        let stream = listener.accept().unwrap();
        let mut handshake = String::new();
        BufReader::new(&stream).read_line(&mut handshake).unwrap();
        (&stream).write_all(b"bye\n").unwrap();
        stream.flush().unwrap();
        drop(stream);
        (handshake, listener)
    });
    let pipe = Arc::new(connect(&name, CONNECT_ATTEMPTS, CONNECT_PAUSE).unwrap());
    let (_keep_input_open, input) = mpsc::channel::<Vec<u8>>();
    let (output, from_relay) = mpsc::channel();
    let ended = relay(
        pipe,
        ticket(5).as_bytes(),
        ChannelInput {
            chunks: input,
            pending: Vec::new(),
        },
        ChannelOutput(output),
    );
    assert_eq!(ended, Ok(RelayEnd::PipeEnded));
    assert_eq!(from_relay.recv().unwrap(), b"bye\n");
    let (handshake, listener) = engine.join().unwrap();
    assert_eq!(handshake, format!("{}\n", ticket(5)));

    // A handshake with a line break or over the bound: nothing reaches the engine.
    // The client stays open until the engine has accepted it: a client that connects and leaves
    // before `accept` is a dead-on-arrival connection that `accept` drops and waits past.
    let (accepted, is_accepted) = mpsc::channel();
    let reader = thread::spawn(move || {
        let stream = listener.accept().unwrap();
        accepted.send(()).unwrap();
        let mut all = Vec::new();
        (&stream).read_to_end(&mut all).map(|_| all)
    });
    let pipe = Arc::new(connect(&name, CONNECT_ATTEMPTS, CONNECT_PAUSE).unwrap());
    is_accepted.recv().unwrap();
    for (line, refusal) in [
        (
            format!("{}\nsecond", ticket(5)).into_bytes(),
            RelayError::LineBreak,
        ),
        (
            format!("{}\r", ticket(5)).into_bytes(),
            RelayError::LineBreak,
        ),
        (vec![b'T'; 129], RelayError::TooLong),
    ] {
        let (_open, input) = mpsc::channel::<Vec<u8>>();
        let (output, _ignored) = mpsc::channel();
        let ended = relay(
            Arc::clone(&pipe),
            &line,
            ChannelInput {
                chunks: input,
                pending: Vec::new(),
            },
            ChannelOutput(output),
        );
        assert_eq!(ended, Err(refusal));
    }
    pipe.assume_flushed();
    drop(pipe);
    assert_eq!(reader.join().unwrap().unwrap(), Vec::<u8>::new());
}

// ---- 11. remote clients ----

/// Opens the pipe through the SMB loopback path, as a remote client would.
fn open_remote(name: &PipeName) -> io::Result<std::fs::File> {
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(format!(r"\\localhost\pipe\{}", name.name()))
}

#[test]
fn sec_10_a_client_through_the_smb_loopback_path_is_refused() {
    // The control: a pipe that accepts remote clients must be reachable this way, or this
    // machine cannot carry the test.
    let control_name = unique("remote-control");
    let control: Listener = PipeListenerOptions::new()
        .path(control_name.path())
        .accept_remote(true)
        .create_duplex::<pipe_mode::Bytes>()
        .unwrap();
    if let Err(error) = open_remote(&control_name) {
        // Shown with `--nocapture`; the test then passes without asserting anything.
        eprintln!(
            "SKIPPED: the SMB loopback path does not reach a pipe that accepts remote clients \
             on this machine ({:?}, os error {:?}); the refusal of remote clients is not tested \
             here",
            error.kind(),
            error.raw_os_error()
        );
        return;
    }
    drop(control);
    let world = World::new();
    let broker = world.broker();
    let mut server = bind(&broker, "remote");
    let refused = open_remote(server.name());
    assert_eq!(
        refused.as_ref().err().and_then(io::Error::raw_os_error),
        Some(5),
        "{refused:?}"
    );
    assert_eq!(server.stats().accepted, 0);
    assert!(server.shutdown(PATIENCE).complete());
}

// ---- 12. shutdown ----

#[test]
fn it_30_shutdown_with_no_client_leaves_nothing_and_the_name_binds_again_at_once() {
    let world = World::new();
    let broker = world.broker();
    let mut server = bind(&broker, "down");
    let name = server.name().clone();
    let started = Instant::now();
    let report = server.shutdown(PATIENCE);
    assert!(report.complete(), "{report:?}");
    assert!(started.elapsed() < Duration::from_secs(5));
    drop(server);
    assert_eq!(
        connect(&name, 1, CONNECT_PAUSE).err(),
        Some(ConnectError::NoEngine),
        "no instance is left"
    );
    let mut again = PipeServer::bind(broker.clone(), name).expect("binds at once");
    assert!(again.shutdown(PATIENCE).complete());
}

#[test]
fn it_30_shutdown_stops_accepting_at_once_and_connections_end_when_their_clients_act_or_leave() {
    let world = World::new();
    let broker = world.broker();
    let mut server = bind(&broker, "down-busy");
    let name = server.name().clone();
    let mut idle = Peer::attach(&name, &ticket(1));
    assert!(idle.call("observe_screen", &json!({}))["result"].is_object());
    let mut leaving = Peer::attach(&name, "");
    assert!(leaving.list()["result"].is_object());
    // A stalled client: it asks for a reply and reads one byte of it, never the rest. The byte
    // shows that its connection's thread has read the request and written the reply, so the
    // thread is now flushing a reply nobody reads, not reading.
    let mut stalled = Peer::attach(&name, "");
    assert!(stalled.list()["result"].is_object());
    stalled.send(format!("{}\n", padded_request(100, 300)).as_bytes());
    let mut first_byte = [0_u8; 1];
    assert_eq!((&*stalled.stream).read(&mut first_byte).unwrap(), 1);
    wait(&server, "three connections", |s| s.connections == 3);
    assert_eq!(clients(&broker), 3);

    let started = Instant::now();
    let report = server.shutdown(Duration::from_millis(300));
    assert!(started.elapsed() < Duration::from_secs(5), "prompt");
    assert!(report.accept_stopped(), "accepting stopped");
    assert_eq!(
        report.state,
        ShutdownState::Remaining {
            connections: 3,
            refusing: 0,
            only_reaper_left: false
        },
        "the count is right while they stay"
    );
    assert!(!report.complete());
    assert!(!server.accepting());
    // Nobody new is served, and the name is still held by the connected instances.
    assert!(connect(&name, 1, CONNECT_PAUSE).is_err());
    assert_eq!(
        PipeServer::bind(broker.clone(), name.clone()).err(),
        Some(PipeError::NameTaken)
    );
    // A client that acts after the shutdown is not served: its next byte ends its connection.
    idle.send(format!("{}\n", padded_request(50, 300)).as_bytes());
    assert_eq!(idle.line(), None, "nothing served after shutdown");
    drop(leaving);
    drop(stalled);
    let report = server.shutdown(PATIENCE);
    assert!(report.complete(), "{report:?}");
    drop(idle);
    let retained = broker.retained().unwrap();
    assert!(retained.clients.is_empty());
    assert_eq!(retained.engine["snapshots"], 0, "the last one purged");
    drop(server);
    let mut again = PipeServer::bind(broker.clone(), name).expect("binds once all are gone");
    assert!(again.shutdown(PATIENCE).complete());
}

// ---- 13. the review of the pipe (D39): bounds and shutdown held to what they say ----

/// R-T2-2 / D40 G1: under connect-and-leave churn, the places held, sampled from the listener
/// all along and recorded by it at each charge, never pass the bound, and the threads not yet
/// joined never pass `2 + places`; at the end every worker is joined.
#[test]
fn it_29_churn_never_holds_more_places_than_the_bound_on_clients() {
    const BOUND: usize = 2;
    const CHURNERS: usize = 8;
    const CYCLES: usize = 500;
    let world = World::new();
    let broker = world.bounded(BOUND);
    let mut server = bind(&broker, "churn");
    let name = server.name().clone();
    let done = std::sync::atomic::AtomicBool::new(false);
    let (connected, samples) = thread::scope(|scope| {
        let sampler = scope.spawn(|| {
            let mut samples = 0_usize;
            while !done.load(Ordering::SeqCst) {
                let s = server.stats();
                assert!(s.connections <= BOUND, "places over the bound: {s:?}");
                assert!(s.refusing <= MAX_REFUSING, "{s:?}");
                assert!(
                    s.threads <= 2 + s.connections + s.refusing,
                    "threads over 2 + places: {s:?}"
                );
                samples += 1;
                thread::yield_now();
            }
            samples
        });
        let churners: Vec<_> = (0..CHURNERS)
            .map(|_| {
                let name = name.clone();
                scope.spawn(move || {
                    let mut connected = 0_usize;
                    for _ in 0..CYCLES {
                        if let Ok(stream) = connect(&name, CONNECT_ATTEMPTS, CONNECT_PAUSE) {
                            // An empty handshake, then the client leaves at once.
                            let _ = (&stream).write_all(
                                b"
",
                            );
                            close(stream);
                            connected += 1;
                        }
                    }
                    connected
                })
            })
            .collect();
        let connected: usize = churners.into_iter().map(|t| t.join().unwrap()).sum();
        done.store(true, Ordering::SeqCst);
        (connected, sampler.join().unwrap())
    });
    let stats = wait(&server, "every worker is joined", |s| {
        s.connections == 0 && s.refusing == 0 && s.threads == 2
    });
    eprintln!("churn: {connected} connections made, {samples} samples; {stats:?}");
    assert!(
        stats.peak_connections <= BOUND,
        "{} places at once for a bound of {BOUND}",
        stats.peak_connections
    );
    assert_eq!(stats.peak_connections, BOUND, "the run reached the bound");
    assert!(
        connected >= CHURNERS * CYCLES / 2 && stats.accepted >= 500 && samples > 0,
        "the run says too little: {connected} connected, {stats:?}"
    );
    assert!(
        stats.told_full + stats.closed_full > 0,
        "the run went past the bound: {stats:?}"
    );
    assert_eq!(
        stats.joined,
        stats.accepted + stats.told_full,
        "every worker joined once"
    );
    assert_eq!(clients(&broker), 0);
    let report = server.shutdown(PATIENCE);
    assert!(report.complete(), "{report:?}");
    assert_eq!(
        report.stats.threads, 0,
        "the accept thread and the reaper joined too"
    );
}

/// R-T2-3: while one client's tool call holds the broker's lock (its capture waits at a gate the
/// test holds closed), the listener still accepts clients up to the bound and refuses the one
/// beyond it, and none of that waits for the call to finish.
#[test]
fn it_29_accepting_never_waits_on_a_call_that_holds_the_broker() {
    let world = World::new();
    let broker = world.bounded(3);
    let mut server = bind(&broker, "long-call");
    let name = server.name().clone();
    let mut caller = Peer::attach(&name, &ticket(1));
    assert!(caller.list()["result"].is_object());
    let before = world.gate.captures();
    world.gate.set_closed(true);
    let opens = Opens(Arc::clone(&world.gate));
    caller.next_id += 1;
    caller.send(
        format!(
            "{}\n",
            tool_line(caller.next_id, "observe_screen", &json!({}))
        )
        .as_bytes(),
    );
    world.gate.wait_captures(before + 1);
    // The call is inside the broker's lock, waiting at the gate. Two more clients are accepted
    // meanwhile: one attaches (its admission itself will wait for the lock), one sends nothing.
    let mut attaching = Peer::attach(&name, "");
    let silent = Peer::connect(&name);
    let stats = wait(&server, "accepted while a call holds the broker", |s| {
        s.accepted == 3
    });
    assert_eq!(stats.connections, 3);
    // The one beyond the bound is refused, still while the call waits.
    let mut over = Peer::connect(&name);
    assert_eq!(over.line().as_deref(), Some(FULL_LINE));
    assert_eq!(over.line(), None);
    assert_eq!(
        world.gate.captures(),
        before + 1,
        "the call has not finished"
    );
    drop(opens);
    let reply: Value = serde_json::from_str(&caller.line().expect("the call's reply")).unwrap();
    assert!(reply["result"]["structuredContent"].is_object(), "{reply}");
    assert!(
        attaching.list()["result"].is_object(),
        "admitted once the call is done"
    );
    drop((caller, attaching, silent, over));
    wait(&server, "all leave", |s| {
        s.connections == 0 && s.refusing == 0
    });
    assert!(server.shutdown(PATIENCE).complete());
}

/// Starts a PowerShell process that adds `instances` instances to the pipe `name` and keeps them
/// waiting for clients, as any process of the same user may (the owner has every right on the
/// pipe). Returns it once they exist; it ends when its input is closed.
fn competitor(name: &PipeName, instances: usize) -> std::process::Child {
    use std::process::{Command, Stdio};
    let script = format!(
        "$ErrorActionPreference = 'Stop'; $all = New-Object System.Collections.ArrayList; \
         for ($i = 0; $i -lt {instances}; $i++) {{ \
         $p = New-Object System.IO.Pipes.NamedPipeServerStream('{}', \
         [System.IO.Pipes.PipeDirection]::InOut, -1, \
         [System.IO.Pipes.PipeTransmissionMode]::Byte, \
         [System.IO.Pipes.PipeOptions]::Asynchronous); \
         [void]$p.BeginWaitForConnection($null, $null); [void]$all.Add($p) }}; \
         [Console]::Out.WriteLine('ready'); [Console]::Out.Flush(); \
         [void][Console]::In.ReadLine(); foreach ($p in $all) {{ $p.Dispose() }}",
        name.name()
    );
    let mut child = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("PowerShell runs");
    let mut line = String::new();
    BufReader::new(child.stdout.as_mut().unwrap())
        .read_line(&mut line)
        .unwrap();
    if line.trim() != "ready" {
        let mut error = String::new();
        let _ = child.stderr.as_mut().unwrap().read_to_string(&mut error);
        panic!("the competing instances were not created: {line:?} {error}");
    }
    child
}

/// R-T2-5: when every wake-up connection reaches another process's instance of the name,
/// shutdown says that its accept thread is still alive, keeps it, says so again when asked again,
/// and completes once the other instances are gone.
#[test]
fn it_30_shutdown_tells_when_its_accept_thread_could_not_be_woken_and_completes_once_it_can() {
    let world = World::new();
    let broker = world.broker();
    let mut server = bind(&broker, "compete");
    let name = server.name().clone();
    let mut other = competitor(&name, 64);
    // A client reaches the oldest instance that waits: this listener's first. Serving it makes
    // the listener create its next waiting instance, the newest of the name, behind the other
    // process's 64.
    let mut peer = Peer::attach(&name, "");
    wait(&server, "the client reached this listener", |s| {
        s.accepted == 1
    });
    assert!(peer.list()["result"].is_object());
    drop(peer);
    wait(&server, "the client left", |s| s.connections == 0);

    let report = server.shutdown(PATIENCE);
    assert_eq!(report.state, ShutdownState::AcceptAlive, "{report:?}");
    assert!(!report.accept_stopped() && !report.complete());
    assert!(!server.accepting());
    // Asked again while the other instances still take every wake-up: the same truth.
    let again = server.shutdown(PATIENCE);
    assert_eq!(again.state, ShutdownState::AcceptAlive, "{again:?}");
    assert_eq!(
        PipeServer::bind(broker.clone(), name.clone()).err(),
        Some(PipeError::NameTaken)
    );
    // The other process goes; the next shutdown wakes the accept thread and completes.
    drop(other.stdin.take());
    assert!(other.wait().unwrap().success());
    let report = server.shutdown(PATIENCE);
    assert!(report.complete(), "{report:?}");
    assert_eq!(report.failure, None);
    drop(server);
    let mut rebound = PipeServer::bind(broker.clone(), name).expect("the name is free");
    assert!(rebound.shutdown(PATIENCE).complete());
}

/// R-T2-6: requests the connection received before the stop and has not handed to the broker
/// yet are never served after it.
#[test]
fn it_30_requests_already_received_are_not_served_after_shutdown() {
    let world = World::new();
    let broker = world.broker();
    let mut server = bind(&broker, "buffered");
    let name = server.name().clone();
    let mut peer = Peer::attach(&name, &ticket(1));
    let before = world.gate.captures();
    assert!(peer.call("observe_screen", &json!({}))["result"].is_object());
    let per_call = world.gate.captures() - before;
    assert!(per_call >= 1, "an observation captures");
    // Three requests in one write: the connection receives them together.
    let start = world.gate.captures();
    let mut batch = String::new();
    for id in 100..103 {
        batch.push_str(&tool_line(id, "observe_screen", &json!({})));
        batch.push('\n');
    }
    peer.send(batch.as_bytes());
    // The first is being served once its captures began. Its reply then waits for the client to
    // read it (a flush on a pipe waits for its reader), and the client does not read yet.
    world.gate.wait_captures(start + per_call);
    let report = server.shutdown(Duration::ZERO);
    assert!(report.accept_stopped(), "{report:?}");
    let first: Value = serde_json::from_str(&peer.line().expect("the first reply")).unwrap();
    assert_eq!(first["id"], 100);
    assert_eq!(peer.line(), None, "nothing more is served");
    assert_eq!(
        world.gate.captures(),
        start + per_call,
        "the two later requests never reached the engine"
    );
    drop(peer);
    assert!(server.shutdown(PATIENCE).complete());
    assert_eq!(clients(&broker), 0);
}

// ---- 14. the second review of the pipe (D40) ----

/// D40 G1: a worker held after its connection is closed and before its thread is joined keeps
/// its place: a client over the bound is refused, and shutdown says `Remaining`; the hold does
/// not block admission's refusal or another worker's end. Once the hold is released and the
/// reaper has joined it, the place is back, and a second shutdown says `Complete`, with every
/// worker joined once and no thread of the listener left.
#[test]
fn it_29_a_worker_s_place_is_free_only_after_its_thread_is_joined_and_shutdown_says_so() {
    watchdog(
        "the held worker's places, refusal and shutdowns",
        a_worker_s_place_is_free_only_after_its_thread_is_joined_and_shutdown_says_so_body,
    );
}

fn a_worker_s_place_is_free_only_after_its_thread_is_joined_and_shutdown_says_so_body() {
    let world = World::new();
    let broker = world.bounded(2);
    let mut server = bind(&broker, "joined");
    let name = server.name().clone();
    let release = Arc::new(Release::default());
    let (began, exiting) = mpsc::channel();
    world.gate.arm(ExitHold {
        began,
        release: Arc::clone(&release),
    });
    // A observes, so its worker thread registers the hold; then A leaves.
    let mut a = Peer::attach(&name, &ticket(1));
    assert!(a.call("observe_screen", &json!({}))["result"].is_object());
    let mut c = Peer::attach(&name, "");
    assert!(c.list()["result"].is_object());
    drop(a);
    exiting
        .recv_timeout(PATIENCE)
        .expect("A's worker thread is exiting, held");
    // The reaper enters A's join after all its bookkeeping; the exit hold keeps it there.
    let held = wait(&server, "the reaper is in A's join", |s| s.joining == 1);
    assert_eq!(
        (held.serving, held.connections, held.joined),
        (1, 2, 0),
        "A's place stays while its join is in progress"
    );
    // The bound is full: a client over it is refused while the join is held.
    let mut over = Peer::connect(&name);
    assert_eq!(over.line().as_deref(), Some(FULL_LINE));
    assert_eq!(over.line(), None);
    // Another worker ends its connection while the join is held.
    drop(c);
    let ended = wait(&server, "C's worker closed its connection", |s| {
        s.serving == 0
    });
    assert_eq!(
        (ended.connections, ended.refusing),
        (2, 1),
        "the places wait behind A's join: one reaper, one join at a time"
    );
    let report = server.shutdown(Duration::ZERO);
    assert_eq!(
        report.state,
        ShutdownState::Remaining {
            connections: 2,
            refusing: 1,
            only_reaper_left: false
        },
        "{report:?}"
    );
    assert!(report.accept_stopped() && !report.complete());
    assert_eq!(
        report.stats.threads, 4,
        "the reaper and three workers, none joined"
    );
    release.open();
    let report = server.shutdown(PATIENCE);
    assert!(report.complete(), "{report:?}");
    assert_eq!(
        (
            report.stats.connections,
            report.stats.refusing,
            report.stats.threads
        ),
        (0, 0, 0)
    );
    assert_eq!(
        report.stats.joined, 3,
        "A, C and the refusal, each joined once"
    );
    assert!(
        !release.failsafe_fired(),
        "nothing waited on the held join: the hold was released by the test"
    );
}

/// D40 G1: the caller's predicate runs outside the listener's lock, so one that blocks does not
/// stop a new client from being accepted and served.
#[test]
fn it_29_a_blocking_predicate_of_wait_until_does_not_stop_admission() {
    watchdog(
        "a client served while a predicate of wait_until blocks",
        a_blocking_predicate_of_wait_until_does_not_stop_admission_body,
    );
}

fn a_blocking_predicate_of_wait_until_does_not_stop_admission_body() {
    let world = World::new();
    let broker = world.broker();
    let mut server = bind(&broker, "predicate");
    let name = server.name().clone();
    let release = Release::default();
    let (entered, is_entered) = mpsc::channel();
    thread::scope(|scope| {
        let waiting = scope.spawn(|| {
            server.wait_until(PATIENCE, |_| {
                let _ = entered.send(());
                release.pass();
                true
            })
        });
        is_entered
            .recv_timeout(PATIENCE)
            .expect("the predicate is running");
        let mut peer = Peer::attach(&name, "");
        assert!(peer.list()["result"].is_object(), "served meanwhile");
        assert_eq!(server.stats().accepted, 1);
        release.open();
        assert!(waiting.join().unwrap().is_some());
        drop(peer);
    });
    assert!(
        !release.failsafe_fired(),
        "the client was served while the predicate blocked"
    );
    assert!(server.shutdown(PATIENCE).complete());
}

/// D40 G1: a change made after `wait_until` took its copy and before it waits is not missed.
/// The predicate, on its first call, has a silent client connect and waits until the listener
/// has made its last change for it (the worker counted among the threads); then it answers
/// false. Nothing changes after that, so a wait for a notification would wait out its time. The
/// predicate watches the counts through a helper thread and a channel with a deadline, never by
/// taking the listener's lock on its own thread: called under that lock (a defect), it fails
/// by assertion instead of deadlocking.
#[test]
fn a_change_between_the_copy_and_the_wait_is_not_missed() {
    watchdog(
        "wait_until seeing a change made between its copy and its wait",
        a_change_between_the_copy_and_the_wait_is_not_missed_body,
    );
}

fn a_change_between_the_copy_and_the_wait_is_not_missed_body() {
    let world = World::new();
    let broker = world.broker();
    let mut server = bind(&broker, "missed");
    let name = server.name().clone();
    let calls = AtomicUsize::new(0);
    let silent: Mutex<Option<Peer>> = Mutex::new(None);
    let got = thread::scope(|scope| {
        server.wait_until(PATIENCE, |s| {
            if calls.fetch_add(1, Ordering::SeqCst) == 0 {
                *silent.lock().unwrap() = Some(Peer::connect(&name));
                let (counted, is_counted) = mpsc::channel();
                let server = &server;
                scope.spawn(move || {
                    let _ = counted.send(server.wait_until(PATIENCE, |s| s.threads == 3));
                });
                let counted = is_counted.recv_timeout(PATIENCE);
                assert!(
                    matches!(counted, Ok(Some(_))),
                    "the client was not counted within the deadline: is the predicate called                      under the listener's lock? {counted:?}"
                );
                return false;
            }
            s.accepted == 1 && s.threads == 3
        })
    });
    assert!(got.is_some(), "the change after the copy was seen");
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    drop(silent);
    assert!(server.shutdown(PATIENCE).complete());
}

/// D40 G2: a request begun before the stop and ended after it is discarded: the stop reaches
/// the reader as the stop, not as an end of input that would complete the line.
#[test]
fn it_30_a_request_begun_before_the_stop_is_discarded_not_dispatched() {
    let world = World::new();
    let broker = world.broker();
    let mut server = bind(&broker, "begun");
    let name = server.name().clone();
    let mut peer = Peer::attach(&name, &ticket(1));
    assert!(peer.list()["result"].is_object());
    let captures = world.gate.captures();
    let before = server.stats().bytes_in;
    let message = tool_line(9, "observe_screen", &json!({}));
    peer.send(message.as_bytes());
    wait(&server, "the message's bytes entered the framer", |s| {
        s.bytes_in == before + message.len() as u64
    });
    let report = server.shutdown(Duration::ZERO);
    assert!(report.accept_stopped(), "{report:?}");
    // The line's end arrives after the stop. The worker may already have ended at the stop (it
    // asked for more bytes after the stop was set) and closed the pipe, so the write may fail:
    // either way the request is not dispatched.
    let _ = (&*peer.stream).write_all(b"\n");
    assert_eq!(peer.line(), None, "nothing written back");
    assert_eq!(world.gate.captures(), captures, "the engine was not called");
    let stats = wait(&server, "the connection ended at the stop", |s| {
        s.stopped == 1
    });
    assert_eq!(stats.failed, 0);
    drop(peer);
    assert!(server.shutdown(PATIENCE).complete());
}

/// D40 G2: a request that passed the stop check before the stop runs to its end: it delivers
/// and replies after the stop, and its connection is `Remaining` meanwhile.
#[test]
fn it_30_a_request_that_passed_the_check_before_the_stop_runs_to_its_end() {
    let world = World::new();
    let broker = world.broker();
    let mut server = bind(&broker, "passed");
    let name = server.name().clone();
    let mut peer = Peer::attach(&name, &ticket(1));
    let page = peer.call("observe_screen", &json!({}))["result"]["structuredContent"].clone();
    world.delivery.set_closed(true);
    let opens = Opens(Arc::clone(&world.delivery));
    peer.next_id += 1;
    peer.send(
        format!(
            "{}\n",
            tool_line(peer.next_id, "execute_action", &click(&page, "late", true))
        )
        .as_bytes(),
    );
    world.delivery.wait_captures(1);
    let report = server.shutdown(Duration::ZERO);
    assert_eq!(
        report.state,
        ShutdownState::Remaining {
            connections: 1,
            refusing: 0,
            only_reaper_left: false
        },
        "{report:?}"
    );
    drop(opens);
    let reply: Value = serde_json::from_str(&peer.line().expect("its reply")).unwrap();
    assert_eq!(
        reply["result"]["structuredContent"]["input_delivered"], true,
        "{reply}"
    );
    assert_eq!(world.events(), 1, "delivered after the stop");
    assert_eq!(peer.line(), None, "and nothing after it");
    drop(peer);
    assert!(server.shutdown(PATIENCE).complete());
}

// ---- 15. the third review of the pipe (D43) ----

/// D43 G7: the broker's teardown (here, a port's destructor that says where it runs and then
/// blocks) never runs on a thread of the listener. After a `Complete` shutdown it has not run;
/// it runs when the test drops the server, on the test's own thread. The test keeps no other
/// owner of the broker.
#[test]
fn it_30_the_broker_s_teardown_runs_on_the_thread_that_drops_a_complete_server() {
    watchdog(
        "the broker's teardown after a complete shutdown",
        the_broker_s_teardown_runs_on_the_thread_that_drops_a_complete_server_body,
    );
}

fn the_broker_s_teardown_runs_on_the_thread_that_drops_a_complete_server_body() {
    let world = World::new();
    let (ran, teardown) = mpsc::channel();
    let release = Arc::new(Release::default());
    let broker = world.broker_with_teardown(ran, Arc::clone(&release));
    let mut server = PipeServer::bind(broker, unique("teardown")).expect("the pipe binds");
    let name = server.name().clone();
    // A client is served (its worker held the broker through its reservation) and leaves.
    let mut peer = Peer::attach(&name, &ticket(1));
    assert!(peer.call("observe_screen", &json!({}))["result"].is_object());
    drop(peer);
    let report = server.shutdown(PATIENCE);
    assert!(report.complete(), "{report:?}");
    assert!(
        matches!(teardown.try_recv(), Err(mpsc::TryRecvError::Empty)),
        "the broker's teardown ran before the server was dropped, on a thread of the listener"
    );
    // A helper lets the destructor go once it has said where it runs.
    let opener = {
        let release = Arc::clone(&release);
        thread::spawn(move || {
            let at = teardown.recv_timeout(PATIENCE);
            release.open();
            at
        })
    };
    let here = thread::current().id();
    drop(server);
    let at = opener
        .join()
        .unwrap()
        .expect("the broker's teardown ran when the server was dropped");
    assert_eq!(at, here, "on the thread that dropped the server");
    assert!(!release.failsafe_fired());
}

/// D43 A2: a wait too long to represent is no deadline, not an overflow: `wait_until` and
/// `shutdown` take it and return as they would with any long wait.
#[test]
fn a_wait_too_long_to_represent_means_no_deadline() {
    let world = World::new();
    let broker = world.broker();
    let mut server = bind(&broker, "forever");
    assert!(
        server
            .wait_until(Duration::MAX, |s| s.threads == 2)
            .is_some()
    );
    let report = server.shutdown(Duration::MAX);
    assert!(report.complete(), "{report:?}");
}
