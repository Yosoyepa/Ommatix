//! The broker (ADR-0039, ADR-0040): several MCP connections on one engine under one input lease
//! and each client's grant. Every behaviour that is on the wire is driven through real MCP
//! messages on the broker's connections, on the simulated desktop, with a manual clock and fixed
//! seeds. Nothing injects real input and nothing captures a screen.

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, mpsc};
use std::task::{Context, Poll};
use std::thread;
use std::time::Duration;

use common::{call, call_ok, error_code, modern_meta};
use ommatix_adapter_mcp::{
    Admission, Broker, BrokerConnection, BrokerError, BrokerState, Connection,
    DEFAULT_MAX_MESSAGE_BYTES, LEASE_META_KEY, MAX_SPENT_TICKETS, McpConfig, McpServer,
    TICKET_MAX_LEN, TICKET_MIN_LEN,
};
use ommatix_adapter_sim::fixture::{FixtureActuator, FixtureCapturer};
use ommatix_adapter_sim::{FixtureDesktop, FixtureScene, confirmed};
use ommatix_application::contract::ValidatedAction;
use ommatix_application::lease::{
    Acquired, ClientId, LeaseChange, LeaseRefusal, Released, Standing,
};
use ommatix_application::{
    CapturedFrame, Clock, DeliveryReceipt, Engine, EngineConfig, FrameCapturer, Grant, GrantStore,
    LeaseTable, LocalPolicy, ManualClock, NoAccessibility, OsActuator, PolicyStore, PortError,
    Ports, TopLevelWindow, WindowOpacity,
};
use ommatix_domain::bbox::BBox;
use ommatix_domain::fusion::Role;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

const SESSION: &str = "broker-session";
/// The application the live test actuator attests and the operator allows.
const APP: &str = "broker-test-app";
/// The screen's text: distinctive words that no refusal may quote.
const BUTTON: &str = "Zephyrquartz";
const FIELD_LABEL: &str = "Vermilionthistle";
const FIELD_VALUE: &str = "Obsidianmarrow";
const FREE_TEXT: &str = "Saffronglacier";
const SCREEN_TEXT: [&str; 4] = [BUTTON, FIELD_LABEL, FIELD_VALUE, FREE_TEXT];
/// The lease term of the tests.
const TERM: Duration = Duration::from_secs(2);
/// `[engine] max_snapshot_age_ms` by default (ADR-0025).
const MAX_AGE: Duration = EngineConfig::DEFAULT_MAX_SNAPSHOT_AGE;

// ---- the world: the simulated desktop, the operator and a live test actuator ----

fn bx(x0: f64, y0: f64, x1: f64, y1: f64) -> BBox {
    BBox::new(x0, y0, x1, y1).expect("test boxes are valid")
}

/// A window with a button, a labelled input field that holds a value and a free text.
fn scene() -> FixtureScene {
    let mut scene = FixtureScene::new(320, 200);
    scene.add_control(confirmed(Role::Button, bx(10.0, 10.0, 130.0, 40.0)));
    scene.add_word(BUTTON, bx(15.0, 15.0, 125.0, 35.0));
    scene.add_word(FIELD_LABEL, bx(10.0, 50.0, 160.0, 65.0));
    scene.add_control(confirmed(Role::Input, bx(10.0, 70.0, 220.0, 100.0)));
    scene.add_word(FIELD_VALUE, bx(15.0, 75.0, 215.0, 95.0));
    scene.add_word(FREE_TEXT, bx(180.0, 120.0, 310.0, 140.0));
    scene
}

/// The operator's policy: live input to the test application, values allowed.
fn live_policy() -> LocalPolicy {
    LocalPolicy {
        actuation_enabled: true,
        include_values: true,
        allowed_applications: vec![APP.to_owned()],
        ..LocalPolicy::default()
    }
}

/// The operator's store: the policy, the stop and the purge file, switched by the test.
struct Operator {
    policy: Mutex<LocalPolicy>,
    purge: AtomicBool,
}

impl Operator {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            policy: Mutex::new(live_policy()),
            purge: AtomicBool::new(false),
        })
    }

    fn policy(&self) -> MutexGuard<'_, LocalPolicy> {
        self.policy.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn set_stopped(&self, stopped: bool) {
        self.policy().stopped = stopped;
    }

    fn request_purge(&self) {
        self.purge.store(true, Ordering::SeqCst);
    }

    fn purge_pending(&self) -> bool {
        self.purge.load(Ordering::SeqCst)
    }
}

impl PolicyStore for Operator {
    fn current(&self) -> LocalPolicy {
        self.policy().clone()
    }

    fn purge_requested(&self) -> bool {
        self.purge_pending()
    }

    fn purge_done(&self) {
        self.purge.store(false, Ordering::SeqCst);
    }
}

/// The desktop's capturer, counting captures.
struct Counted(FixtureCapturer, Arc<AtomicUsize>);

impl FrameCapturer for Counted {
    fn name(&self) -> &'static str {
        self.0.name()
    }

    fn capture(&mut self) -> Result<CapturedFrame, PortError> {
        self.1.fetch_add(1, Ordering::SeqCst);
        self.0.capture()
    }
}

/// The desktop's actuator made live: it attests the test application and reports every event
/// inserted. What it is handed is recorded by the simulated desktop and goes nowhere else.
struct Live {
    inner: FixtureActuator,
    fail: Arc<AtomicBool>,
}

impl OsActuator for Live {
    fn name(&self) -> &'static str {
        "broker-test-live"
    }

    fn is_live(&self) -> bool {
        true
    }

    fn foreground_application(&self) -> Option<String> {
        Some(APP.to_owned())
    }

    fn foreground_window(&self) -> Option<TopLevelWindow> {
        self.inner.foreground_window()
    }

    fn window_at(&self, point: (f64, f64)) -> Option<TopLevelWindow> {
        self.inner.window_at(point)
    }

    fn top_level_windows(&self) -> Option<Vec<TopLevelWindow>> {
        self.inner.top_level_windows()
    }

    fn window_opacity(&self, window: &TopLevelWindow) -> Option<WindowOpacity> {
        self.inner.window_opacity(window)
    }

    fn deliver(&mut self, action: &ValidatedAction) -> Result<DeliveryReceipt, PortError> {
        assert!(
            !self.fail.load(Ordering::SeqCst),
            "the test actuator fails while the broker holds its lock"
        );
        self.inner.deliver(action)?;
        Ok(DeliveryReceipt {
            submitted: 1,
            inserted: 1,
        })
    }
}

/// One simulated desktop with its clock, operator and counters.
struct World {
    desktop: FixtureDesktop,
    clock: Arc<ManualClock>,
    operator: Arc<Operator>,
    captures: Arc<AtomicUsize>,
    fail: Arc<AtomicBool>,
}

impl World {
    fn new() -> Self {
        Self::with_scene(scene())
    }

    fn with_scene(scene: FixtureScene) -> Self {
        let clock = Arc::new(ManualClock::new());
        Self {
            desktop: FixtureDesktop::new(scene, clock.clone()),
            clock,
            operator: Operator::new(),
            captures: Arc::default(),
            fail: Arc::default(),
        }
    }

    fn ports(&self) -> Ports {
        Ports {
            capturer: Box::new(Counted(self.desktop.capturer(), self.captures.clone())),
            detector: Box::new(self.desktop.detector()),
            recognizer: Box::new(self.desktop.recognizer()),
            accessibility: Box::new(NoAccessibility),
            actuator: Box::new(Live {
                inner: self.desktop.actuator(),
                fail: self.fail.clone(),
            }),
            policy: self.operator.clone(),
            clock: self.clock.clone(),
        }
    }

    fn broker(&self, config: McpConfig, max_clients: usize) -> Broker {
        Broker::new(
            EngineConfig::new(SESSION),
            self.ports(),
            config,
            TERM,
            max_clients,
        )
        .expect("a valid broker")
    }

    /// A broker that asks `grants` for the grant of every client admitted by a ticket.
    fn ticket_broker(&self, grants: Arc<dyn GrantStore>, max_clients: usize) -> Broker {
        self.ticket_broker_with(McpConfig::default(), grants, max_clients)
    }

    fn ticket_broker_with(
        &self,
        config: McpConfig,
        grants: Arc<dyn GrantStore>,
        max_clients: usize,
    ) -> Broker {
        Broker::with_grant_store(
            EngineConfig::new(SESSION),
            self.ports(),
            config,
            TERM,
            max_clients,
            grants,
        )
        .expect("a valid broker")
    }

    /// The stdio server over the same kind of desktop.
    fn server(&self, config: McpConfig) -> McpServer {
        McpServer::new(
            Engine::new(EngineConfig::new(SESSION), self.ports()),
            config,
        )
    }

    /// The input events the actuator was handed.
    fn events(&self) -> usize {
        self.desktop.handed_actions().len()
    }

    fn captures(&self) -> usize {
        self.captures.load(Ordering::SeqCst)
    }

    fn advance(&self, by: Duration) {
        self.clock.advance(by);
    }
}

struct Rig {
    world: World,
    broker: Broker,
}

fn rig() -> Rig {
    let world = World::new();
    let broker = world.broker(McpConfig::default(), 8);
    Rig { world, broker }
}

impl Rig {
    fn connect(&self, grant: Option<Grant>) -> BrokerConnection {
        self.broker.connect(grant).expect("the client is admitted")
    }

    fn engine_sizes(&self) -> BTreeMap<&'static str, usize> {
        self.broker
            .retained()
            .expect("the broker is available")
            .engine
    }
}

// ---- grants ----

fn grant(deliver_input: bool, read_values: bool) -> Grant {
    Grant {
        applications: BTreeSet::from([APP.to_owned()]),
        deliver_input,
        read_values,
        ..Grant::default()
    }
}

/// Everything the operator's policy allows.
fn full() -> Grant {
    grant(true, true)
}

/// Reads values, delivers nothing.
fn reader() -> Grant {
    grant(false, true)
}

/// Delivers, reads no values.
fn blind() -> Grant {
    grant(true, false)
}

/// Everything, but for another application than the one in front.
fn elsewhere() -> Grant {
    Grant {
        applications: BTreeSet::from(["another-app".to_owned()]),
        deliver_input: true,
        read_values: true,
        ..Grant::default()
    }
}

// ---- tickets and the operator's grants by ticket ----

/// A ticket of the exact form (ADR-0043, decision 9): 43 base64url characters, distinct for
/// each `n`, with a distinctive prefix that the leak checks search for.
fn ticket(n: u64) -> String {
    format!("Qv7-Tkt_{n:035}")
}

fn sha256(ticket: &str) -> [u8; 32] {
    Sha256::digest(ticket.as_bytes()).into()
}

/// The operator's grants by ticket hash, changed by the test between calls. It counts every
/// question and can be made to panic.
#[derive(Default)]
struct Grants {
    listed: Mutex<BTreeMap<[u8; 32], Grant>>,
    asks: AtomicUsize,
    panics: AtomicBool,
    /// Every hash it was asked about.
    asked: Mutex<Vec<[u8; 32]>>,
}

impl Grants {
    fn listed(&self) -> MutexGuard<'_, BTreeMap<[u8; 32], Grant>> {
        self.listed.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Lists `grant` for `ticket`, or withdraws what is listed for it.
    fn set(&self, ticket: &str, grant: Option<Grant>) {
        match grant {
            Some(grant) => self.listed().insert(sha256(ticket), grant),
            None => self.listed().remove(&sha256(ticket)),
        };
    }

    fn asks(&self) -> usize {
        self.asks.load(Ordering::SeqCst)
    }
}

impl GrantStore for Grants {
    fn grant(&self, ticket_sha256: &[u8; 32]) -> Option<Grant> {
        self.asks.fetch_add(1, Ordering::SeqCst);
        self.asked
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(*ticket_sha256);
        assert!(
            !self.panics.load(Ordering::SeqCst),
            "the test's grant store fails"
        );
        self.listed().get(ticket_sha256).cloned()
    }
}

// ---- MCP on a broker connection ----

fn observe_with(conn: &mut BrokerConnection, arguments: Value) -> Value {
    call_ok(conn.connection(), 1, "observe_screen", arguments)
}

/// An observation's `structuredContent`.
fn observe(conn: &mut BrokerConnection) -> Value {
    observe_with(conn, json!({}))["structuredContent"].clone()
}

fn element<'a>(page: &'a Value, role: &str) -> &'a Value {
    page["elements"]
        .as_array()
        .expect("a page")
        .iter()
        .find(|e| e["role"] == role)
        .unwrap_or_else(|| panic!("no {role} in {page}"))
}

/// A click on the button of `page`.
fn click(page: &Value, request_id: &str, live: bool) -> Value {
    target(page, "button", request_id, live)
}

fn target(page: &Value, role: &str, request_id: &str, live: bool) -> Value {
    let element = element(page, role);
    json!({
        "request_id": request_id,
        "session_id": page["session_id"],
        "snapshot_id": page["snapshot_id"],
        "geometry_epoch": page["geometry_epoch"],
        "target_id": element["target_id"],
        "expected_revision": element["revision"],
        "action": "click",
        "dry_run": !live
    })
}

/// An action's whole tool result.
fn act_result(conn: &mut BrokerConnection, arguments: Value) -> Value {
    call_ok(conn.connection(), 1, "execute_action", arguments)
}

/// An action's `structuredContent`.
fn act(conn: &mut BrokerConnection, arguments: Value) -> Value {
    act_result(conn, arguments)["structuredContent"].clone()
}

fn metrics(conn: &mut BrokerConnection) -> Value {
    call_ok(conn.connection(), 1, "get_engine_metrics", json!({}))["structuredContent"].clone()
}

fn code(structured: &Value) -> Option<&str> {
    structured["error"]["code"].as_str()
}

fn delivered(structured: &Value) -> bool {
    structured["input_delivered"] == true
}

/// A refusal before the gate for want of the lease.
fn refused_without_lease(structured: &Value) -> bool {
    code(structured) == Some("POLICY_DENIED")
        && structured["status"] == "rejected"
        && !delivered(structured)
        && structured["error"]["message"]
            .as_str()
            .is_some_and(|m| m.contains("input lease"))
}

fn value_of(page: &Value) -> Option<&str> {
    element(page, "input")["text"].as_str()
}

// ---- 1. a delivery spends the snapshot for every client ----

#[test]
fn it_09_after_one_clients_delivery_another_clients_action_on_that_snapshot_is_stale() {
    let rig = rig();
    let mut a = rig.connect(Some(full()));
    let mut b = rig.connect(Some(full()));
    let page = observe(&mut a);
    assert_eq!(observe(&mut b)["snapshot_id"], page["snapshot_id"]);

    assert!(delivered(&act(&mut a, click(&page, "a1", true))));
    assert_eq!(rig.world.events(), 1);

    let dry = act(&mut b, click(&page, "b1", false));
    assert_eq!(code(&dry), Some("STALE_SNAPSHOT"), "{dry}");
    a.release().expect("a holds the lease");
    assert!(matches!(b.acquire(), Ok(Acquired::Granted { .. })));
    let live = act(&mut b, click(&page, "b2", true));
    assert_eq!(code(&live), Some("STALE_SNAPSHOT"), "{live}");
    assert!(!delivered(&live));
    assert_eq!(
        rig.world.events(),
        1,
        "the simulated actuator recorded nothing"
    );
}

// ---- 2. while another client holds the lease, a non-holder is served the published page ----

#[test]
fn it_10_a_non_holders_observation_is_the_published_page_and_moves_nothing() {
    let rig = rig();
    let mut a = rig.connect(Some(full()));
    let mut b = rig.connect(Some(full()));
    let holder = observe_with(&mut a, json!({ "include_values": true }));
    let observations = metrics(&mut a)["observations"].clone();
    let captures = rig.world.captures();

    let served = observe_with(&mut b, json!({ "include_values": true }));
    assert_eq!(served["structuredContent"], holder["structuredContent"]);
    let keys = |r: &Value| -> BTreeSet<String> {
        r["_meta"].as_object().unwrap().keys().cloned().collect()
    };
    assert_eq!(keys(&served), keys(&holder), "the same _meta keys");
    // That the page is shared is said only in `_meta`; its age is also the page's own `age_ms`
    // in `structuredContent`, which `io.ommatix/lease` repeats (ADR-0039; decision D30).
    assert_eq!(
        holder["_meta"][LEASE_META_KEY],
        json!({ "state": "holder", "number": 1, "position": null, "shared": false,
                "age_ms": null })
    );
    assert_eq!(
        served["_meta"][LEASE_META_KEY],
        json!({ "state": "none", "number": null, "position": null, "shared": true,
                "age_ms": 0 })
    );
    for key in ["shared", LEASE_META_KEY] {
        assert!(
            !served["structuredContent"].to_string().contains(key),
            "{key}"
        );
        assert!(!served["content"].to_string().contains(key), "{key}");
    }
    assert_eq!(rig.world.captures(), captures, "nothing is captured");
    assert_eq!(metrics(&mut b)["observations"], observations);

    let page = holder["structuredContent"].clone();
    assert!(delivered(&act(&mut a, click(&page, "a1", true))));
    assert_eq!(rig.world.events(), 1);
}

// ---- 3. no lease, no input, no screen text ----

#[test]
fn it_11_a_live_action_without_the_lease_delivers_nothing_and_quotes_no_screen_text() {
    let rig = rig();
    let mut a = rig.connect(Some(full()));
    let mut b = rig.connect(Some(full()));
    let page = observe_with(&mut a, json!({ "include_values": true }))["structuredContent"].clone();
    for text in SCREEN_TEXT {
        assert!(page.to_string().contains(text), "the screen shows {text}");
    }
    let page = observe_with(&mut b, json!({ "include_values": true }))["structuredContent"].clone();
    let agent_text = "agent-chosen-id";
    let arguments = target(&page, "input", agent_text, true);
    let response = call(b.connection(), 7, "execute_action", arguments.clone());
    let structured = &response["result"]["structuredContent"];
    assert!(refused_without_lease(structured), "{response}");
    assert_eq!(rig.world.events(), 0, "no input event");
    let whole = response.to_string();
    for text in SCREEN_TEXT {
        assert!(!whole.contains(text), "{text} in {whole}");
    }
    let message = structured["error"]["message"].as_str().unwrap();
    assert!(!message.contains(agent_text), "{message}");

    // Nothing was recorded for b: once it holds the lease, the same request identifier on a
    // new page is a new request (a recorded one would be a REQUEST_CONFLICT) and delivers.
    a.release().unwrap();
    assert!(matches!(b.acquire(), Ok(Acquired::Granted { .. })));
    let page = observe(&mut b);
    assert!(delivered(&act(
        &mut b,
        target(&page, "input", agent_text, true)
    )));
}

// ---- 4. the term ends ----

#[test]
fn it_12_when_the_term_ends_and_another_client_acquires_the_former_holder_is_refused() {
    let rig = rig();
    let mut a = rig.connect(Some(full()));
    let mut b = rig.connect(Some(full()));
    let page = observe(&mut a);
    assert_eq!(b.acquire(), Ok(Acquired::Queued { position: 1 }));
    rig.world.advance(TERM);

    let refused = act(&mut a, click(&page, "a1", true));
    assert!(refused_without_lease(&refused), "{refused}");
    // The lease passed to b and spent the page: b observes before it acts.
    let stale = act(&mut b, click(&page, "b1", true));
    assert_eq!(code(&stale), Some("STALE_SNAPSHOT"), "{stale}");
    let fresh = observe(&mut b);
    assert_ne!(fresh["snapshot_id"], page["snapshot_id"]);
    assert!(delivered(&act(&mut b, click(&fresh, "b2", true))));
    assert_eq!(rig.world.events(), 1);
}

// ---- 5. a disconnect, the stop and a purge end the lease ----

#[test]
fn it_13_the_holder_disconnecting_ends_the_lease() {
    let rig = rig();
    let a = rig.connect(Some(full()));
    let mut b = rig.connect(Some(full()));
    let mut c = rig.connect(Some(full()));
    assert_eq!(b.acquire(), Ok(Acquired::Queued { position: 1 }));
    a.close();
    let page = observe(&mut b);
    assert!(delivered(&act(&mut b, click(&page, "b1", true))));
    let page = observe(&mut b);
    assert!(refused_without_lease(&act(
        &mut c,
        click(&page, "c1", true)
    )));
}

#[test]
fn it_13_the_operators_stop_ends_the_lease_and_refuses_every_delivery_whoever_holds_it() {
    let rig = rig();
    let mut a = rig.connect(Some(full()));
    let mut b = rig.connect(Some(full()));
    let page = observe(&mut a);
    rig.world.operator.set_stopped(true);
    let _ = metrics(&mut b);
    rig.world.operator.set_stopped(false);
    assert!(refused_without_lease(&act(
        &mut a,
        click(&page, "a1", true)
    )));
    assert!(
        matches!(a.acquire(), Ok(Acquired::Granted { .. })),
        "a new lease"
    );
    rig.world.operator.set_stopped(true);
    for (conn, id) in [(&mut a, "a2"), (&mut b, "b2")] {
        let _ = conn.acquire();
        let page = observe(conn);
        let refused = act(conn, click(&page, id, true));
        assert_eq!(code(&refused), Some("POLICY_DENIED"), "{refused}");
    }
    drop(b);
    // A sole client holds the lease implicitly: the gate refuses it under the stop.
    let page = observe(&mut a);
    let refused = act(&mut a, click(&page, "a3", true));
    assert_eq!(code(&refused), Some("POLICY_DENIED"));
    assert!(
        !refused_without_lease(&refused),
        "the gate's own refusal: {refused}"
    );
    assert_eq!(rig.world.events(), 0);
    rig.world.operator.set_stopped(false);
}

#[test]
fn it_13_an_operator_purge_honoured_at_the_next_request_ends_the_lease() {
    let rig = rig();
    let mut a = rig.connect(Some(full()));
    let mut b = rig.connect(Some(full()));
    let page = observe(&mut a);
    rig.world.operator.request_purge();
    let refused = act(&mut a, click(&page, "a1", true));
    assert!(
        !rig.world.operator.purge_pending(),
        "the purge was honoured"
    );
    assert!(refused_without_lease(&refused), "{refused}");
    assert_eq!(rig.engine_sizes()["snapshots"], 0);
    assert!(
        matches!(a.acquire(), Ok(Acquired::Granted { .. })),
        "a new lease"
    );
    let page = observe(&mut a);
    assert!(delivered(&act(&mut a, click(&page, "a2", true))));
    let _ = observe(&mut b);
    assert_eq!(rig.world.events(), 1);
}

// ---- 6. grants ----

#[test]
fn it_14_a_client_without_a_grant_observes_without_values_and_never_delivers() {
    // Alone: it holds the lease implicitly, and still delivers nothing.
    let rig = rig();
    let mut alone = rig.connect(None);
    let page =
        observe_with(&mut alone, json!({ "include_values": true }))["structuredContent"].clone();
    assert_eq!(value_of(&page), None, "no values");
    let refused = act(&mut alone, click(&page, "n1", true));
    assert_eq!(code(&refused), Some("POLICY_DENIED"), "{refused}");
    assert!(
        !refused_without_lease(&refused),
        "refused at the gate: {refused}"
    );
    let dry = act(&mut alone, click(&page, "n2", false));
    assert_eq!(dry["status"], "simulated", "a dry run is not live input");

    // With others, as the holder.
    let mut other = rig.connect(Some(full()));
    assert_eq!(alone.acquire(), Ok(Acquired::Renewed { number: 1 }));
    let page =
        observe_with(&mut alone, json!({ "include_values": true }))["structuredContent"].clone();
    assert_eq!(value_of(&page), None);
    let refused = act(&mut alone, click(&page, "n3", true));
    assert_eq!(code(&refused), Some("POLICY_DENIED"));
    let shown =
        observe_with(&mut other, json!({ "include_values": true }))["structuredContent"].clone();
    assert_eq!(
        value_of(&shown),
        Some(FIELD_VALUE),
        "the other client's own view"
    );
    assert_eq!(rig.world.events(), 0);
}

#[test]
fn it_14_a_grant_that_leaves_the_application_out_is_refused_at_the_gate() {
    let rig = rig();
    let mut conn = rig.connect(Some(elsewhere()));
    let page = observe(&mut conn);
    let refused = act(&mut conn, click(&page, "e1", true));
    assert_eq!(code(&refused), Some("POLICY_DENIED"), "{refused}");
    assert!(
        !refused_without_lease(&refused),
        "the gate's refusal: {refused}"
    );
    assert_eq!(rig.world.events(), 0);
}

#[test]
fn it_14_two_clients_with_different_read_values_get_their_own_view_of_one_snapshot() {
    let rig = rig();
    let mut shown = rig.connect(Some(full()));
    let mut hidden = rig.connect(Some(blind()));
    let with =
        observe_with(&mut shown, json!({ "include_values": true }))["structuredContent"].clone();
    let without =
        observe_with(&mut hidden, json!({ "include_values": true }))["structuredContent"].clone();
    assert_eq!(with["snapshot_id"], without["snapshot_id"]);
    assert_eq!(value_of(&with), Some(FIELD_VALUE));
    assert_eq!(value_of(&without), None);
    // And the other way round: the client without values holds the lease and publishes.
    shown.release().unwrap();
    assert!(matches!(hidden.acquire(), Ok(Acquired::Granted { .. })));
    let without =
        observe_with(&mut hidden, json!({ "include_values": true }))["structuredContent"].clone();
    let with =
        observe_with(&mut shown, json!({ "include_values": true }))["structuredContent"].clone();
    assert_eq!(with["snapshot_id"], without["snapshot_id"]);
    assert_eq!(value_of(&with), Some(FIELD_VALUE));
    assert_eq!(value_of(&without), None);
}

// ---- 7. a non-holder's dry run ----

#[test]
fn it_15_a_non_holders_dry_run_is_judged_by_the_gate_on_the_current_page_only() {
    let rig = rig();
    let mut a = rig.connect(Some(full()));
    let mut b = rig.connect(Some(full()));
    let first = observe(&mut a);
    let served = observe(&mut b);
    assert_eq!(
        act(&mut b, click(&served, "b1", false))["status"],
        "simulated"
    );

    // Replaced: the holder observes again.
    let second = observe(&mut a);
    let stale = act(&mut b, click(&first, "b2", false));
    assert_eq!(code(&stale), Some("STALE_SNAPSHOT"), "{stale}");
    assert_eq!(
        act(&mut b, click(&second, "b3", false))["status"],
        "simulated"
    );

    // Spent: the holder delivers on it.
    assert!(delivered(&act(&mut a, click(&second, "a1", true))));
    let stale = act(&mut b, click(&second, "b4", false));
    assert_eq!(code(&stale), Some("STALE_SNAPSHOT"), "{stale}");
    assert_eq!(rig.world.events(), 1);
}

// ---- 8. request identifiers per client ----

#[test]
fn it_16_two_clients_with_one_request_id_get_their_own_results_and_each_retry_replays_its_own() {
    let rig = rig();
    let mut a = rig.connect(Some(full()));
    let mut b = rig.connect(Some(full()));
    let page = observe(&mut a);
    let a_first = act_result(&mut a, click(&page, "same", false));
    // Other arguments under the same identifier: b's own request, not a conflict.
    let b_args = target(&page, "input", "same", false);
    let b_first = act_result(&mut b, b_args.clone());
    assert_eq!(b_first["structuredContent"]["status"], "simulated");
    assert_ne!(a_first["structuredContent"], b_first["structuredContent"]);
    assert!(delivered(&act(&mut a, click(&page, "live", true))));

    // The page is spent now, so only a replay can answer "simulated" again.
    let a_retry = act_result(&mut a, click(&page, "same", false));
    let b_retry = act_result(&mut b, b_args);
    assert_eq!(a_retry["structuredContent"], a_first["structuredContent"]);
    assert_eq!(b_retry["structuredContent"], b_first["structuredContent"]);
    let a_live_retry = act(&mut a, click(&page, "live", true));
    assert!(delivered(&a_live_retry), "a replay of the delivered result");
    assert_eq!(rig.world.events(), 1, "the replay delivered nothing new");
}

// ---- a client's own retry is answered before the lease is asked for ----

/// A client that delivered and then lost the lease to another client, which observed again.
struct LostLease {
    rig: Rig,
    a: BrokerConnection,
    b: BrokerConnection,
    /// The arguments of a's delivered request and the whole tool result it got.
    a_request: Value,
    a_first: Value,
    /// b's lease number and its page.
    b_lease: u64,
    b_page: Value,
}

fn delivered_then_lost_the_lease() -> LostLease {
    let rig = rig();
    let mut a = rig.connect(Some(full()));
    let mut b = rig.connect(Some(full()));
    let page = observe(&mut a);
    let a_request = click(&page, "a-live", true);
    let a_first = act_result(&mut a, a_request.clone());
    assert!(delivered(&a_first["structuredContent"]), "{a_first}");
    a.release().unwrap();
    let Ok(Acquired::Granted { number: b_lease }) = b.acquire() else {
        panic!("b is granted the lease");
    };
    let b_page = observe(&mut b);
    assert_eq!(rig.world.events(), 1);
    LostLease {
        rig,
        a,
        b,
        a_request,
        a_first,
        b_lease,
        b_page,
    }
}

/// A tool result without the stage timings of the gate: a replay runs no gate, so it has none,
/// as on the stdio server. Without the lease's `_meta` too: it tells where the client stands
/// now, which is not part of the replayed result.
fn without_action_timings(result: &Value) -> Value {
    let mut result = result.clone();
    if let Some(meta) = result.get_mut("_meta").and_then(Value::as_object_mut) {
        meta.remove(ommatix_adapter_mcp::ACTION_TIMINGS_META_KEY);
        meta.remove(LEASE_META_KEY);
    }
    result
}

#[test]
fn it_16_a_client_that_lost_the_lease_gets_its_own_delivered_result_back_on_a_retry() {
    let mut s = delivered_then_lost_the_lease();
    let retry = act_result(&mut s.a, s.a_request.clone());
    assert_eq!(retry["structuredContent"], s.a_first["structuredContent"]);
    assert_eq!(
        retry["content"], s.a_first["content"],
        "the same text block"
    );
    assert_eq!(
        without_action_timings(&retry),
        without_action_timings(&s.a_first)
    );
    assert_eq!(s.a_first["_meta"][LEASE_META_KEY]["state"], "holder");
    assert_eq!(
        retry["_meta"][LEASE_META_KEY]["state"], "none",
        "the replay comes with where a stands now"
    );
    assert!(
        delivered(&retry["structuredContent"]),
        "it learns it was delivered"
    );
    assert_eq!(s.rig.world.events(), 1, "nothing new is delivered");

    // The other client's lease and page are untouched.
    assert_eq!(s.b.acquire(), Ok(Acquired::Renewed { number: s.b_lease }));
    assert!(delivered(&act(&mut s.b, click(&s.b_page, "b-live", true))));
    assert_eq!(s.rig.world.events(), 2);
}

#[test]
fn it_16_a_retry_with_other_arguments_after_losing_the_lease_is_a_request_conflict() {
    let mut s = delivered_then_lost_the_lease();
    let mut other = target(&s.b_page, "input", "a-live", true);
    other["request_id"] = s.a_request["request_id"].clone();
    let refused = act(&mut s.a, other);
    assert_eq!(code(&refused), Some("REQUEST_CONFLICT"), "{refused}");
    assert!(!delivered(&refused));
    assert_eq!(s.rig.world.events(), 1);
    assert!(delivered(&act(&mut s.b, click(&s.b_page, "b-live", true))));
}

#[test]
fn it_16_a_live_request_new_to_the_clients_journal_still_needs_the_lease() {
    let mut s = delivered_then_lost_the_lease();
    for _ in 0..2 {
        // Refused, so never recorded: the retry is refused again.
        let refused = act(&mut s.a, click(&s.b_page, "a-new", true));
        assert!(refused_without_lease(&refused), "{refused}");
    }
    assert_eq!(s.rig.world.events(), 1);
}

#[test]
fn it_16_a_client_cannot_replay_another_clients_request_id() {
    let s = delivered_then_lost_the_lease();
    let mut c = s.rig.connect(Some(full()));
    let refused = act(&mut c, s.a_request.clone());
    assert!(refused_without_lease(&refused), "{refused}");
    assert_eq!(s.rig.world.events(), 1);
}

#[test]
fn it_16_a_retry_from_a_new_connection_finds_nothing() {
    let s = delivered_then_lost_the_lease();
    let LostLease {
        rig, a, a_request, ..
    } = s;
    a.close();
    let mut again = rig.connect(Some(full()));
    let refused = act(&mut again, a_request);
    assert!(refused_without_lease(&refused), "{refused}");
    assert_eq!(rig.world.events(), 1);
}

// ---- 9. the bound, and leaving frees everything ----

#[test]
fn it_17_a_ninth_client_is_refused_and_a_client_that_left_leaves_nothing_behind() {
    let world = World::new();
    for (bound, term) in [(9, TERM), (0, TERM), (1, Duration::ZERO)] {
        let error = Broker::new(
            EngineConfig::new(SESSION),
            world.ports(),
            McpConfig::default(),
            term,
            bound,
        )
        .err();
        let expected = if term.is_zero() {
            BrokerError::ZeroTerm
        } else {
            BrokerError::ClientBound
        };
        assert_eq!(error, Some(expected), "bound {bound}, term {term:?}");
    }

    let rig = rig();
    let mut first = rig.connect(Some(full()));
    let mut leaving = rig.connect(Some(reader()));
    let third = rig.connect(Some(full()));
    let mut others: Vec<BrokerConnection> = (3..8).map(|_| rig.connect(Some(full()))).collect();
    assert_eq!(
        rig.broker.connect(Some(full())).err(),
        Some(BrokerError::Lease(LeaseRefusal::TooManyClients))
    );

    assert_eq!(leaving.acquire(), Ok(Acquired::Queued { position: 1 }));
    assert_eq!(third.acquire(), Ok(Acquired::Queued { position: 2 }));
    observe(&mut first);
    let served = observe(&mut leaving);
    let _ = act(&mut leaving, click(&served, "l1", false));
    let _ = act(&mut leaving, click(&served, "l2", false));
    let before = rig.broker.retained().unwrap();
    let leaving_id = leaving.client();
    assert!(before.clients.contains(&leaving_id));
    leaving.close();

    let after = rig.broker.retained().unwrap();
    assert!(!after.clients.contains(&leaving_id), "its grant is gone");
    assert_eq!(after.clients.len(), 7);
    assert!(!after.grant_installed, "no grant stays installed");
    assert_eq!(
        after.engine["journal_entries"],
        before.engine["journal_entries"] - 2,
        "its journal is gone"
    );
    // Its place in the queue is gone: the lease passes to the client behind it.
    first.release().unwrap();
    assert!(matches!(third.acquire(), Ok(Acquired::Renewed { .. })));
    // Its place among the clients is free again.
    let newcomer = rig.connect(Some(full()));
    assert_eq!(newcomer.acquire(), Ok(Acquired::Queued { position: 1 }));
    others.clear();
    assert_eq!(rig.broker.retained().unwrap().clients.len(), 3);
}

// ---- 10. the last client purges, the others do not ----

#[test]
fn it_18_only_the_last_client_to_leave_purges_the_engine() {
    let rig = rig();
    let mut a = rig.connect(Some(full()));
    let mut b = rig.connect(Some(full()));
    let mut c = rig.connect(Some(reader()));
    let page = observe(&mut a);
    let b_dry = act(&mut b, click(&page, "r1", false));
    let _ = act(&mut c, click(&page, "r1", false));
    let entries = rig.engine_sizes()["journal_entries"];

    c.close();
    let sizes = rig.engine_sizes();
    assert_eq!(sizes["snapshots"], 1, "no purge while others remain");
    assert_eq!(sizes["journal_entries"], entries - 1);
    assert!(!rig.broker.retained().unwrap().grant_installed);
    let served = observe(&mut b);
    assert_eq!(
        served["snapshot_id"], page["snapshot_id"],
        "the published page"
    );
    assert_eq!(
        act(&mut b, click(&page, "r1", false)),
        b_dry,
        "its own journal"
    );
    assert_eq!(
        act(&mut b, click(&page, "r2", false))["status"],
        "simulated"
    );

    a.close();
    assert_eq!(rig.engine_sizes()["snapshots"], 1, "b remains");
    b.close();
    let sizes = rig.engine_sizes();
    for key in ["snapshots", "base_frames", "journal_entries", "live_tracks"] {
        assert_eq!(sizes[key], 0, "{key} after the last client left");
    }
    assert!(rig.broker.retained().unwrap().clients.is_empty());
}

// ---- 11. one client is the stdio server ----

/// The stdio server and a broker with one client, fed the same messages in lockstep.
struct Twin {
    _stdio_world: World,
    _broker_world: World,
    stdio: Connection,
    broker: BrokerConnection,
    next_id: u64,
    modern: bool,
    compared: usize,
}

impl Twin {
    fn new(config: McpConfig, modern: bool) -> Self {
        let stdio_world = World::new();
        let broker_world = World::new();
        let stdio = stdio_world.server(config).connect();
        let policy = live_policy();
        let equal_to_policy = Grant {
            applications: policy.allowed_applications.iter().cloned().collect(),
            deliver_input: policy.actuation_enabled,
            read_values: policy.include_values,
            ..Grant::default()
        };
        let broker = broker_world
            .broker(config, 8)
            .connect(Some(equal_to_policy))
            .unwrap();
        Self {
            _stdio_world: stdio_world,
            _broker_world: broker_world,
            stdio,
            broker,
            next_id: 0,
            modern,
            compared: 0,
        }
    }

    /// Sends one raw message to both; the two response lines must be identical.
    fn raw(&mut self, message: &str) -> Option<Value> {
        let stdio = self.stdio.handle_message(message.as_bytes());
        let broker = self.broker.connection().handle_message(message.as_bytes());
        assert_eq!(stdio, broker, "the broker answered {message} differently");
        self.compared += 1;
        stdio.map(|line| serde_json::from_str(&line).unwrap())
    }

    fn request(&mut self, method: &str, mut params: Value) -> Value {
        self.next_id += 1;
        if self.modern {
            params["_meta"] = modern_meta();
        }
        let message = json!({ "jsonrpc": "2.0", "id": self.next_id, "method": method,
                              "params": params });
        self.raw(&message.to_string()).expect("a response")
    }

    #[allow(clippy::needless_pass_by_value)] // call sites build the arguments inline
    fn tool(&mut self, name: &str, arguments: Value) -> Value {
        self.request(
            "tools/call",
            json!({ "name": name, "arguments": arguments }),
        )
    }

    fn structured(&mut self, name: &str, arguments: Value) -> Value {
        self.tool(name, arguments)["result"]["structuredContent"].clone()
    }
}

fn scripted_session(twin: &mut Twin) {
    twin.request("tools/list", json!({}));
    let first = twin.structured("observe_screen", json!({ "limit": 2 }));
    assert_eq!(first["truncated"], true);
    let next = twin.structured(
        "observe_screen",
        json!({ "limit": 2, "cursor": first["next_cursor"] }),
    );
    assert_eq!(next["snapshot_id"], first["snapshot_id"]);
    twin.structured("observe_screen", json!({ "query": BUTTON }));
    let page = twin.structured("observe_screen", json!({ "include_values": true }));
    assert_eq!(value_of(&page), Some(FIELD_VALUE));
    assert_eq!(
        twin.structured("execute_action", click(&page, "dry", false))["status"],
        "simulated"
    );
    twin.structured("execute_action", click(&page, "dry", false));
    let conflict = twin.structured("execute_action", target(&page, "input", "dry", false));
    assert_eq!(code(&conflict), Some("REQUEST_CONFLICT"));
    assert!(delivered(
        &twin.structured("execute_action", click(&page, "live", true))
    ));
    assert!(delivered(
        &twin.structured("execute_action", click(&page, "live", true))
    ));
    let stale = twin.structured("execute_action", click(&page, "again", true));
    assert_eq!(code(&stale), Some("STALE_SNAPSHOT"));
    let page = twin.structured("observe_screen", json!({}));
    let mut elsewhere = click(&page, "bad-target", true);
    elsewhere["target_id"] = json!("e999");
    assert_eq!(
        code(&twin.structured("execute_action", elsewhere)),
        Some("TARGET_NOT_FOUND")
    );
    assert!(delivered(
        &twin.structured("execute_action", click(&page, "live-2", true))
    ));
    twin.tool("execute_action", json!({ "x": 1, "y": 2 }));
    twin.tool("no_such_tool", json!({}));
    twin.request("no/such/method", json!({}));
    let metrics = twin.structured("get_engine_metrics", json!({}));
    assert_eq!(metrics["delivered_actions"], 2);
    twin.raw("not json");
    twin.raw(r#"{"jsonrpc":"2.0","method":"notifications/cancelled"}"#);
}

#[test]
fn it_19_one_client_whose_grant_equals_the_policy_gets_the_stdio_servers_responses_byte_for_byte() {
    // The modern profile.
    let mut twin = Twin::new(McpConfig::default(), true);
    twin.request("server/discover", json!({}));
    scripted_session(&mut twin);
    assert!(twin.compared >= 20);

    // The legacy profile, with its handshake.
    let mut twin = Twin::new(McpConfig::with_legacy(), false);
    twin.raw(r#"{"jsonrpc":"2.0","id":0,"method":"ping"}"#);
    twin.request(
        "initialize",
        json!({ "protocolVersion": "2025-11-25", "capabilities": {},
                "clientInfo": { "name": "broker-tests", "version": "1.0" } }),
    );
    twin.raw(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#);
    twin.request("ping", json!({}));
    scripted_session(&mut twin);
    assert!(twin.compared >= 20);
}

// ---- 12. message bounds on a broker connection ----

/// A modern `tools/list` request padded with spaces to exactly `len` bytes.
fn padded_request(id: u64, len: usize) -> String {
    let base = json!({ "jsonrpc": "2.0", "id": id, "method": "tools/list",
                       "params": { "_meta": modern_meta() } })
    .to_string();
    let mut padded = base[..base.len() - 1].to_owned();
    padded.push_str(&" ".repeat(len - base.len()));
    padded.push('}');
    padded
}

fn served_lines(output: Vec<u8>) -> Vec<String> {
    let output = String::from_utf8(output).expect("UTF-8 output");
    assert!(output.is_empty() || output.ends_with('\n'));
    output.lines().map(str::to_owned).collect()
}

#[test]
fn it_19_oversized_and_malformed_messages_keep_the_stdio_servers_bounds_and_codes() {
    let observe = json!({ "jsonrpc": "2.0", "id": 9, "method": "tools/call",
                          "params": { "_meta": modern_meta(), "name": "observe_screen",
                                      "arguments": {} } })
    .to_string();
    let mut input = Vec::new();
    for line in [
        padded_request(1, DEFAULT_MAX_MESSAGE_BYTES),
        padded_request(2, DEFAULT_MAX_MESSAGE_BYTES + 1),
        "x".repeat(4 * 1024 * 1024),
        "not json".to_owned(),
        "[1, 2]".to_owned(),
        r#"{"jsonrpc":"1.0","id":3,"method":"tools/list"}"#.to_owned(),
        r#"{"jsonrpc":"2.0","id":{"a":1},"method":"tools/list"}"#.to_owned(),
        "   ".to_owned(),
        format!("\u{feff}{}", padded_request(4, 300)),
        observe,
        padded_request(5, 300),
    ] {
        input.extend_from_slice(line.as_bytes());
        input.push(b'\n');
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let stdio_world = World::new();
    let mut stdio = Vec::new();
    runtime
        .block_on(
            stdio_world
                .server(McpConfig::default())
                .serve(input.as_slice(), &mut stdio),
        )
        .unwrap();
    let world = World::new();
    let broker = world.broker(McpConfig::default(), 8);
    let conn = broker.connect(Some(full())).unwrap();
    let mut output = Vec::new();
    runtime
        .block_on(conn.serve(input.as_slice(), &mut output))
        .unwrap();

    let lines = served_lines(output);
    assert_eq!(lines, served_lines(stdio));
    assert_eq!(lines.len(), 10, "{lines:?}");
    let responses: Vec<Value> = lines
        .iter()
        .map(|l| {
            assert!(l.len() <= DEFAULT_MAX_MESSAGE_BYTES);
            serde_json::from_str(l).unwrap()
        })
        .collect();
    assert_eq!(responses[0]["id"], 1, "exactly the limit is accepted");
    for oversized in &responses[1..3] {
        assert_eq!(error_code(oversized), -32600);
        assert_eq!(oversized["id"], Value::Null);
        assert_eq!(
            oversized["error"]["data"]["maxMessageBytes"],
            DEFAULT_MAX_MESSAGE_BYTES
        );
    }
    assert_eq!(error_code(&responses[3]), -32700);
    for invalid in &responses[4..7] {
        assert_eq!(error_code(invalid), -32600, "{invalid}");
    }
    assert_eq!(responses[7]["id"], 4, "a byte-order mark is skipped");
    assert_eq!(responses[8]["result"]["isError"], false);
    assert_eq!(responses[9]["id"], 5, "the stream recovers");
    // The connection ended, and it was the only one: the engine is purged.
    assert_eq!(broker.retained().unwrap().engine["snapshots"], 0);
}

// ---- the lock poisoned by a panic fails closed ----

#[test]
fn it_20_a_call_that_panics_under_the_lock_leaves_the_broker_closed_to_every_request() {
    let rig = rig();
    let mut a = rig.connect(Some(full()));
    let page = observe(&mut a);
    rig.world.fail.store(true, Ordering::SeqCst);
    let message = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call",
                          "params": { "_meta": modern_meta(), "name": "execute_action",
                                      "arguments": click(&page, "boom", true) } })
    .to_string();
    let panicked = catch_unwind(AssertUnwindSafe(|| {
        a.connection().handle_message(message.as_bytes())
    }));
    assert!(panicked.is_err(), "the actuator panicked");
    rig.world.fail.store(false, Ordering::SeqCst);

    let page_again = call(a.connection(), 2, "observe_screen", json!({}));
    assert_eq!(error_code(&page_again), -32603, "{page_again}");
    let live = call(
        a.connection(),
        3,
        "execute_action",
        click(&page, "after", true),
    );
    assert_eq!(error_code(&live), -32603, "{live}");
    let tools = common::modern(a.connection(), 4, "tools/list", json!({}));
    assert!(
        tools["result"]["tools"].is_array(),
        "the protocol still answers"
    );
    assert_eq!(a.acquire(), Err(BrokerError::Unavailable));
    assert_eq!(a.release(), Err(BrokerError::Unavailable));
    assert_eq!(
        rig.broker.connect(Some(full())).err(),
        Some(BrokerError::Unavailable)
    );
    assert!(rig.broker.retained().is_none());
    assert_eq!(rig.world.events(), 0, "nothing was delivered");
    a.close();
}

// ---- 13. generated interleavings, one thread ----

/// Fixed-seed generator (splitmix64): the same sequence on every run, with no random crate.
struct Rng(u64);

impl Rng {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A number in `0..n`, for `n > 0`.
    fn below(&mut self, n: usize) -> usize {
        usize::try_from(self.next_u64() % u64::try_from(n).unwrap()).unwrap()
    }

    fn chance(&mut self, percent: usize) -> bool {
        self.below(100) < percent
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Full,
    Reader,
    Blind,
    Elsewhere,
    Without,
}

impl Kind {
    const ALL: [Kind; 5] = [
        Kind::Full,
        Kind::Reader,
        Kind::Blind,
        Kind::Elsewhere,
        Kind::Without,
    ];

    fn grant(self) -> Option<Grant> {
        match self {
            Kind::Full => Some(full()),
            Kind::Reader => Some(reader()),
            Kind::Blind => Some(blind()),
            Kind::Elsewhere => Some(elsewhere()),
            Kind::Without => None,
        }
    }

    /// Whether the gate lets this client deliver.
    fn delivers(self) -> bool {
        matches!(self, Kind::Full | Kind::Blind)
    }

    fn reads_values(self) -> bool {
        matches!(self, Kind::Full | Kind::Reader | Kind::Elsewhere)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Seen {
    Connected,
    ConnectRefused,
    Left,
    LastLeft,
    Granted,
    Queued,
    Renewed,
    Implicit,
    RenewalRefused,
    Released,
    LeftQueue,
    ReleaseRefused,
    Published,
    Served,
    NothingPublished,
    ValuesShown,
    ValuesHidden,
    Simulated,
    DryStale,
    Delivered,
    NoLease,
    GateDenied,
    LiveStale,
    Replayed,
    ReplayedWithoutLease,
    Stopped,
    Resumed,
    Purged,
    TimeMoved,
    LeaseSpent,
    Metrics,
}

const EVERY_OUTCOME: [Seen; 31] = [
    Seen::Connected,
    Seen::ConnectRefused,
    Seen::Left,
    Seen::LastLeft,
    Seen::Granted,
    Seen::Queued,
    Seen::Renewed,
    Seen::Implicit,
    Seen::RenewalRefused,
    Seen::Released,
    Seen::LeftQueue,
    Seen::ReleaseRefused,
    Seen::Published,
    Seen::Served,
    Seen::NothingPublished,
    Seen::ValuesShown,
    Seen::ValuesHidden,
    Seen::Simulated,
    Seen::DryStale,
    Seen::Delivered,
    Seen::NoLease,
    Seen::GateDenied,
    Seen::LiveStale,
    Seen::Replayed,
    Seen::ReplayedWithoutLease,
    Seen::Stopped,
    Seen::Resumed,
    Seen::Purged,
    Seen::TimeMoved,
    Seen::LeaseSpent,
    Seen::Metrics,
];

/// The published snapshot as the sequential run sees it.
#[derive(Debug, Clone)]
struct Snap {
    id: String,
    captured: Duration,
    spent: bool,
}

/// The sequential run: the lease table driven at the instants and in the order the broker is
/// specified to drive it, and the engine's state as far as a delivery depends on it.
struct Model {
    leases: LeaseTable,
    connected: BTreeSet<ClientId>,
    stopped: bool,
    purge: bool,
    current: Option<Snap>,
    /// Per client, the results the engine journaled, by request identifier.
    journals: BTreeMap<ClientId, BTreeMap<String, Value>>,
    /// How many times each kind of outcome occurred.
    seen: BTreeMap<Seen, usize>,
}

impl Model {
    fn new(max_clients: usize) -> Self {
        Self {
            leases: LeaseTable::new(max_clients.try_into().unwrap(), TERM).unwrap(),
            connected: BTreeSet::new(),
            stopped: false,
            purge: false,
            current: None,
            journals: BTreeMap::new(),
            seen: BTreeMap::new(),
        }
    }

    fn saw(&mut self, outcome: Seen) {
        *self.seen.entry(outcome).or_default() += 1;
    }

    fn purge_engine(&mut self) {
        self.current = None;
        self.journals.clear();
    }

    fn enter(&mut self) {
        let purged = std::mem::take(&mut self.purge);
        if purged {
            self.purge_engine();
            self.saw(Seen::Purged);
        }
        if purged || self.stopped {
            let change = self.leases.revoke();
            self.apply(&change);
        }
    }

    fn apply(&mut self, change: &LeaseChange) {
        if change.spend_snapshot {
            if let Some(snap) = self.current.as_mut() {
                snap.spent = true;
            }
            self.saw(Seen::LeaseSpent);
        }
    }

    /// A tool call: the request boundary, then the call's own entry and settlement.
    fn tool(&mut self, client: ClientId, now: Duration, observe: bool) -> bool {
        self.enter();
        self.enter();
        let outcome = if observe {
            self.leases.publishes(client, now)
        } else {
            self.leases.may_deliver(client, now)
        };
        self.apply(&outcome.change);
        outcome.result.unwrap()
    }

    /// Whether the gate passes an action that cites `snapshot` at `now`.
    fn current_and_fresh(&self, snapshot: &Value, now: Duration) -> bool {
        self.current.as_ref().is_some_and(|snap| {
            snapshot == snap.id.as_str()
                && !snap.spent
                && now.saturating_sub(snap.captured) <= MAX_AGE
        })
    }

    /// The clients that may deliver at `now`, asked on a copy, so that nothing settles.
    fn holders(&self, now: Duration) -> Vec<ClientId> {
        let mut copy = self.leases.clone();
        self.connected
            .iter()
            .copied()
            .filter(|client| copy.may_deliver(*client, now).result == Ok(true))
            .collect()
    }
}

struct Slot {
    conn: BrokerConnection,
    kind: Kind,
    page: Option<Value>,
    last: Option<Value>,
    /// The ticket the client attached with, when it did ([`Drive::Tickets`]).
    ticket: Option<String>,
}

/// Every action response must be free of screen text.
fn assert_no_screen_text(response: &Value) {
    let text = response.to_string();
    for word in SCREEN_TEXT {
        assert!(!text.contains(word), "{word} in {text}");
    }
}

/// How the clients of a generated run ask for the lease.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Drive {
    /// Through [`BrokerConnection::acquire`] and [`BrokerConnection::release`].
    Methods,
    /// Only through the `lease` argument of `observe_screen` (ADR-0039, decision 5), with every
    /// `io.ommatix/lease` `_meta` compared with the sequential run's prediction.
    Wire,
    /// Through the methods, with about half the clients admitted by a ticket whose grant the
    /// operator's store changes between calls (ADR-0040, decision 5): the kind of a ticket's
    /// client is the grant the store lists at that call.
    Tickets,
}

/// The `io.ommatix/lease` `_meta` a response must carry, built here from the sequential run's
/// table, independently of the broker's serializer.
fn predicted_lease(standing: Standing, shared_age: Option<Duration>) -> Value {
    let (state, number, position) = match standing {
        Standing::Holder { number } => ("holder", json!(number), Value::Null),
        Standing::Waiting { position } => ("waiting", Value::Null, json!(position)),
        Standing::None => ("none", Value::Null, Value::Null),
    };
    json!({
        "state": state,
        "number": number,
        "position": position,
        "shared": shared_age.is_some(),
        "age_ms": shared_age.map(|age| u64::try_from(age.as_millis()).unwrap()),
    })
}

/// Checks a response's `io.ommatix/lease` against the sequential run: present exactly when the
/// request carried `lease` or more than one client is connected (ADR-0039, decision 5).
fn check_lease_meta(
    model: &Model,
    client: ClientId,
    result: &Value,
    asked: bool,
    shared_age: Option<Duration>,
    step: usize,
) {
    let meta = result
        .get("_meta")
        .and_then(|meta| meta.get(LEASE_META_KEY));
    if asked || model.connected.len() > 1 {
        let expected = predicted_lease(model.leases.standing(client), shared_age);
        assert_eq!(meta, Some(&expected), "step {step}: {result}");
    } else {
        assert_eq!(
            meta, None,
            "step {step}: a sole client that did not ask: {result}"
        );
    }
}

/// What an observation answered, checked against the sequential run, which it updates: a new
/// snapshot for a client that publishes, the published page served again otherwise, or
/// `STALE_SNAPSHOT` with nothing published. Returns the served page's age when it was served.
fn observed(
    model: &mut Model,
    slot: &mut Slot,
    result: &Value,
    publishes: bool,
    values: bool,
    now: Duration,
    step: usize,
) -> Option<Duration> {
    let page = result["structuredContent"].clone();
    let mut shared_age = None;
    if publishes {
        let snapshot = page["snapshot_id"].as_str().expect("a page").to_owned();
        if let Some(previous) = &model.current {
            assert_ne!(previous.id, snapshot, "step {step}: a new snapshot");
        }
        model.current = Some(Snap {
            id: snapshot,
            captured: now,
            spent: false,
        });
        model.saw(Seen::Published);
    } else if let Some(snap) = &model.current {
        assert_eq!(page["snapshot_id"], snap.id.as_str(), "step {step}: served");
        shared_age = Some(now.saturating_sub(snap.captured));
        model.saw(Seen::Served);
    } else {
        assert_eq!(code(&page), Some("STALE_SNAPSHOT"), "step {step}: {page}");
        model.saw(Seen::NothingPublished);
    }
    if page["elements"].is_array() {
        let shown = value_of(&page).is_some();
        assert_eq!(shown, values && slot.kind.reads_values(), "step {step}");
        model.saw(if shown {
            Seen::ValuesShown
        } else {
            Seen::ValuesHidden
        });
        slot.page = Some(page);
    }
    shared_age
}

#[test]
fn it_21_generated_interleavings_deliver_once_per_snapshot_only_from_the_holder_under_its_grant() {
    generated_run(Drive::Methods);
}

#[test]
fn it_21_generated_interleavings_that_ask_for_the_lease_only_on_the_wire_keep_every_invariant() {
    generated_run(Drive::Wire);
}

#[test]
fn generated_interleavings_with_clients_admitted_by_ticket_and_grants_changed_between_calls_keep_every_invariant()
 {
    generated_run(Drive::Tickets);
}

#[allow(clippy::too_many_lines)] // one generated run: each kind of step beside its model
fn generated_run(drive: Drive) {
    const STEPS: usize = 5_000;
    const SLOTS: usize = 5;
    const MAX_CLIENTS: usize = 4;
    let world = World::new();
    let grants = Arc::new(Grants::default());
    let broker = match drive {
        Drive::Tickets => world.ticket_broker(grants.clone(), MAX_CLIENTS),
        Drive::Methods | Drive::Wire => world.broker(McpConfig::default(), MAX_CLIENTS),
    };
    let mut model = Model::new(MAX_CLIENTS);
    let mut rng = Rng(match drive {
        Drive::Methods => 0x0B40_4E55,
        Drive::Wire => 0x5EED_0006,
        Drive::Tickets => 0x71C4_E709,
    });
    // Tickets issued, grants changed in the store, and tool calls by clients with a ticket.
    let (mut issued, mut changes, mut ticket_calls) = (0_u64, 0_usize, 0_usize);
    let mut slots: Vec<Option<Slot>> = (0..SLOTS).map(|_| None).collect();
    let mut next_request = 0_u64;
    // The witness: every delivery, in the order the broker's lock was taken (one thread).
    let mut witness: Vec<(usize, ClientId, String)> = Vec::new();
    let mut per_snapshot: BTreeMap<String, usize> = BTreeMap::new();

    for step in 0..STEPS {
        // The operator changes the grant of a client with a ticket, between two calls.
        if drive == Drive::Tickets && rng.chance(8) {
            let ticketed: Vec<usize> = (0..SLOTS)
                .filter(|i| slots[*i].as_ref().is_some_and(|s| s.ticket.is_some()))
                .collect();
            if !ticketed.is_empty() {
                let slot = slots[ticketed[rng.below(ticketed.len())]].as_mut().unwrap();
                let kind = Kind::ALL[rng.below(Kind::ALL.len())];
                grants.set(slot.ticket.as_deref().unwrap(), kind.grant());
                slot.kind = kind;
                changes += 1;
            }
        }
        let now = world.clock.now();
        // Half the steps go to the holder, if any, so that deliveries are frequent.
        let holder = model.holders(now).first().copied();
        let index = slots
            .iter()
            .position(|slot| slot.as_ref().map(|s| s.conn.client()) == holder)
            .filter(|_| holder.is_some() && rng.chance(50))
            .unwrap_or_else(|| rng.below(SLOTS));
        let dice = rng.below(100);
        match (dice, slots[index].is_some()) {
            (0..=9, false) => {
                let kind = Kind::ALL[rng.below(Kind::ALL.len())];
                model.enter();
                let expected = model.leases.connect(now);
                model.apply(&expected.change);
                let ticket = (drive == Drive::Tickets && rng.chance(50)).then(|| {
                    issued += 1;
                    let ticket = ticket(issued);
                    grants.set(&ticket, kind.grant());
                    ticket
                });
                let actual = match &ticket {
                    Some(ticket) => broker.admit(ticket.clone().into_bytes()),
                    None => broker.connect(kind.grant()),
                };
                match (expected.result, actual) {
                    (Ok(id), Ok(conn)) => {
                        assert_eq!(conn.client(), id);
                        model.connected.insert(id);
                        slots[index] = Some(Slot {
                            conn,
                            kind,
                            page: None,
                            last: None,
                            ticket,
                        });
                        model.saw(Seen::Connected);
                    }
                    (Err(LeaseRefusal::TooManyClients), Err(error)) => {
                        assert_eq!(error, BrokerError::Lease(LeaseRefusal::TooManyClients));
                        model.saw(Seen::ConnectRefused);
                    }
                    (expected, actual) => {
                        panic!("step {step}: expected {expected:?}, got {:?}", actual.err())
                    }
                }
            }
            (0..=9, true) => {
                let slot = slots[index].take().unwrap();
                let id = slot.conn.client();
                slot.conn.close();
                let change = model.leases.disconnect(id, now);
                assert_eq!(change.result, Ok(()));
                model.apply(&change.change);
                model.connected.remove(&id);
                model.journals.remove(&id);
                if model.connected.is_empty() {
                    model.purge_engine();
                    model.saw(Seen::LastLeft);
                } else {
                    model.saw(Seen::Left);
                }
            }
            (_, false) => {}
            (10..=19, true) if drive != Drive::Wire => {
                let slot = slots[index].as_ref().unwrap();
                let id = slot.conn.client();
                model.enter();
                let expected = model.leases.acquire(id, now);
                model.apply(&expected.change);
                let actual = slot.conn.acquire();
                assert_eq!(
                    actual,
                    expected.result.map_err(BrokerError::Lease),
                    "step {step}"
                );
                model.saw(match actual {
                    Ok(Acquired::Granted { .. }) => Seen::Granted,
                    Ok(Acquired::Queued { .. }) => Seen::Queued,
                    Ok(Acquired::Renewed { .. }) => Seen::Renewed,
                    Ok(Acquired::Implicit) => Seen::Implicit,
                    Err(_) => Seen::RenewalRefused,
                });
            }
            (20..=25, true) if drive != Drive::Wire => {
                let slot = slots[index].as_ref().unwrap();
                let id = slot.conn.client();
                model.enter();
                let expected = model.leases.release(id, now);
                model.apply(&expected.change);
                let actual = slot.conn.release();
                assert_eq!(
                    actual,
                    expected.result.map_err(BrokerError::Lease),
                    "step {step}"
                );
                model.saw(match actual {
                    Ok(Released::LeftQueue) => Seen::LeftQueue,
                    Ok(_) => Seen::Released,
                    Err(_) => Seen::ReleaseRefused,
                });
            }
            (26..=35, true) => {
                let millis = 100 + rng.below(1_400);
                world.advance(Duration::from_millis(u64::try_from(millis).unwrap()));
                model.saw(Seen::TimeMoved);
            }
            // The lease asked for on the wire: the request boundary, the call's entry, the
            // lease operation at the call's instant, then the settlement and the observation.
            (10..=25, true) => {
                let acquire = dice <= 19;
                let slot = slots[index].as_mut().unwrap();
                let id = slot.conn.client();
                let values = rng.chance(50);
                model.enter();
                model.enter();
                let mut left_queue = false;
                let answer = if acquire {
                    let expected = model.leases.acquire(id, now);
                    model.apply(&expected.change);
                    expected.result.map(Some)
                } else {
                    let expected = model.leases.release(id, now);
                    model.apply(&expected.change);
                    left_queue = expected.result == Ok(Released::LeftQueue);
                    expected.result.map(|_| None)
                };
                let settled = model.leases.publishes(id, now);
                model.apply(&settled.change);
                let publishes = settled.result.unwrap();
                let lease = if acquire { "acquire" } else { "release" };
                let result = observe_with(
                    &mut slot.conn,
                    json!({ "include_values": values, "lease": lease }),
                );
                let shared_age = observed(&mut model, slot, &result, publishes, values, now, step);
                check_lease_meta(&model, id, &result, true, shared_age, step);
                // The table's own answer, told independently of `standing`.
                let state = &result["_meta"][LEASE_META_KEY];
                model.saw(match answer {
                    Ok(Some(Acquired::Granted { number })) => {
                        assert_eq!(state["number"], number, "step {step}");
                        Seen::Granted
                    }
                    Ok(Some(Acquired::Renewed { number })) => {
                        assert_eq!(state["number"], number, "step {step}");
                        Seen::Renewed
                    }
                    Ok(Some(Acquired::Queued { position })) => {
                        assert_eq!(state["position"], position, "step {step}");
                        Seen::Queued
                    }
                    Ok(Some(Acquired::Implicit)) => {
                        assert_eq!(state["state"], "holder", "step {step}");
                        Seen::Implicit
                    }
                    Ok(None) if left_queue => Seen::LeftQueue,
                    Ok(None) => Seen::Released,
                    Err(_) if acquire => Seen::RenewalRefused,
                    Err(_) => Seen::ReleaseRefused,
                });
            }
            (36..=52, true) => {
                let slot = slots[index].as_mut().unwrap();
                let id = slot.conn.client();
                let values = rng.chance(50);
                let publishes = model.tool(id, now, true);
                ticket_calls += usize::from(slot.ticket.is_some());
                let result = observe_with(&mut slot.conn, json!({ "include_values": values }));
                let shared_age = observed(&mut model, slot, &result, publishes, values, now, step);
                if drive == Drive::Wire {
                    check_lease_meta(&model, id, &result, false, shared_age, step);
                }
            }
            (53..=86, true) => {
                let live = dice >= 63;
                let retry = dice >= 80;
                let slot = slots[index].as_mut().unwrap();
                let id = slot.conn.client();
                let arguments = if retry {
                    match &slot.last {
                        Some(last) => last.clone(),
                        None => continue,
                    }
                } else {
                    let Some(page) = &slot.page else { continue };
                    next_request += 1;
                    let request_id = format!("r{next_request}");
                    let role = if rng.chance(50) { "button" } else { "input" };
                    target(page, role, &request_id, live)
                };
                let live = arguments["dry_run"] == false;
                let request_id = arguments["request_id"].as_str().unwrap().to_owned();
                let holds = model.tool(id, now, false);
                ticket_calls += usize::from(slot.ticket.is_some());
                let whole = act_result(&mut slot.conn, arguments.clone());
                if drive == Drive::Wire {
                    check_lease_meta(&model, id, &whole, false, None, step);
                }
                let response = whole["structuredContent"].clone();
                assert_no_screen_text(&response);
                slot.last = Some(arguments.clone());
                // A client's own retry reaches the engine whoever holds the lease (ADR-0039,
                // decision 11); anything else live needs the lease.
                let replay = model
                    .journals
                    .get(&id)
                    .and_then(|journal| journal.get(&request_id))
                    .cloned();
                let reaches_engine = !live || holds || replay.is_some();
                let outcome = if let Some(stored) = replay {
                    assert_eq!(
                        response, stored,
                        "step {step}: a retry replays its own result"
                    );
                    if live && !holds {
                        Seen::ReplayedWithoutLease
                    } else {
                        Seen::Replayed
                    }
                } else if !reaches_engine {
                    assert!(refused_without_lease(&response), "step {step}: {response}");
                    Seen::NoLease
                } else {
                    let fresh = model.current_and_fresh(&arguments["snapshot_id"], now);
                    let outcome = match (live, fresh) {
                        (_, false) => {
                            assert_eq!(code(&response), Some("STALE_SNAPSHOT"), "step {step}");
                            if live {
                                Seen::LiveStale
                            } else {
                                Seen::DryStale
                            }
                        }
                        (false, true) => {
                            assert_eq!(response["status"], "simulated", "step {step}");
                            Seen::Simulated
                        }
                        (true, true) if model.stopped || !slot.kind.delivers() => {
                            assert_eq!(code(&response), Some("POLICY_DENIED"), "step {step}");
                            assert!(!refused_without_lease(&response));
                            Seen::GateDenied
                        }
                        (true, true) => {
                            assert!(delivered(&response), "step {step}: {response}");
                            model.current.as_mut().unwrap().spent = true;
                            Seen::Delivered
                        }
                    };
                    model
                        .journals
                        .entry(id)
                        .or_default()
                        .insert(request_id, response.clone());
                    outcome
                };
                let replayed = matches!(outcome, Seen::Replayed | Seen::ReplayedWithoutLease);
                if delivered(&response) && !replayed {
                    assert!(
                        slot.kind.delivers(),
                        "step {step}: a {:?} client",
                        slot.kind
                    );
                    let snapshot = response["snapshot_id"].as_str().unwrap().to_owned();
                    *per_snapshot.entry(snapshot.clone()).or_default() += 1;
                    witness.push((step, id, snapshot));
                }
                model.saw(outcome);
            }
            // The stop: engaged on a quarter of these steps, released on all.
            (87..=90, true) if model.stopped || rng.chance(25) => {
                let stopped = !model.stopped;
                world.operator.set_stopped(stopped);
                model.stopped = stopped;
                model.saw(if stopped {
                    Seen::Stopped
                } else {
                    Seen::Resumed
                });
            }
            (91..=92, true) => {
                world.operator.request_purge();
                model.purge = true;
            }
            (_, true) => {
                let slot = slots[index].as_mut().unwrap();
                let id = slot.conn.client();
                model.tool(id, now, false);
                ticket_calls += usize::from(slot.ticket.is_some());
                let result = call_ok(slot.conn.connection(), 1, "get_engine_metrics", json!({}));
                if drive == Drive::Wire {
                    assert!(result["_meta"].get(LEASE_META_KEY).is_none(), "step {step}");
                }
                model.saw(Seen::Metrics);
            }
        }

        // After every step.
        let now = world.clock.now();
        assert!(
            model.holders(now).len() <= 1,
            "step {step}: at most one holder"
        );
        assert!(per_snapshot.values().all(|n| *n == 1), "step {step}");
        assert_eq!(
            world.events(),
            witness.len(),
            "step {step}: events are deliveries"
        );
        assert_eq!(world.operator.purge_pending(), model.purge, "step {step}");
        let retained = broker.retained().unwrap();
        assert_eq!(retained.clients, model.connected, "step {step}");
        assert!(!retained.grant_installed, "step {step}");
        // The store is asked once per call of a client with a ticket, and for nobody else.
        assert_eq!(grants.asks(), ticket_calls, "step {step}");
    }
    if drive == Drive::Tickets {
        assert!(
            issued >= 20 && changes >= 50 && ticket_calls >= 500,
            "{issued} tickets, {changes} changes, {ticket_calls} calls"
        );
    }

    let missing: Vec<_> = EVERY_OUTCOME
        .iter()
        .filter(|o| !model.seen.contains_key(o))
        .collect();
    assert!(
        missing.is_empty(),
        "outcomes that never occurred: {missing:?}"
    );
    assert!(
        witness.len() >= 20,
        "{} deliveries; outcomes {:?}",
        witness.len(),
        model.seen
    );
}

// ---- 14. real threads ----

#[test]
#[allow(clippy::too_many_lines)] // each thread's steps beside the checks of their log
fn it_21_concurrent_connections_never_deliver_twice_on_one_snapshot_nor_outside_their_grant() {
    const STEPS: usize = 500;
    let world = World::new();
    // Two clients of each kind of grant, the bound of the engine's journals.
    let kinds = [
        Kind::Full,
        Kind::Blind,
        Kind::Without,
        Kind::Reader,
        Kind::Full,
        Kind::Blind,
        Kind::Without,
        Kind::Reader,
    ];
    let broker = world.broker(McpConfig::default(), kinds.len());
    let connections: Vec<BrokerConnection> = kinds
        .iter()
        .map(|kind| broker.connect(kind.grant()).unwrap())
        .collect();
    let clock = world.clock.clone();

    let logs: Vec<(Kind, Vec<(Value, bool)>)> = std::thread::scope(|scope| {
        let handles: Vec<_> = connections
            .into_iter()
            .zip(kinds)
            .enumerate()
            .map(|(thread, (mut conn, kind))| {
                let clock = clock.clone();
                scope.spawn(move || {
                    let mut rng = Rng(0xC0FF_EE00 + u64::try_from(thread).unwrap());
                    // Each action's response, and whether it is a new request rather than a
                    // replay of a recorded one.
                    let mut log: Vec<(Value, bool)> = Vec::new();
                    let mut page: Option<Value> = None;
                    let mut last: Option<(Value, Value)> = None;
                    for step in 0..STEPS {
                        match rng.below(12) {
                            0 => {
                                let _ = conn.acquire();
                            }
                            1 => {
                                let _ = conn.release();
                            }
                            2 => clock.advance(Duration::from_millis(200)),
                            3..=5 => {
                                let observed = observe_with(
                                    &mut conn,
                                    json!({ "include_values": true }),
                                )["structuredContent"]
                                    .clone();
                                if observed["elements"].is_array() {
                                    // Each view is decided under this client's own grant.
                                    assert_eq!(
                                        value_of(&observed).is_some(),
                                        kind.reads_values(),
                                        "a {kind:?} client's view"
                                    );
                                    page = Some(observed);
                                }
                            }
                            6..=9 => {
                                let Some(page) = &page else { continue };
                                let live = rng.chance(80);
                                let id = format!("t{thread}-{step}");
                                let arguments = click(page, &id, live);
                                let response = act(&mut conn, arguments.clone());
                                assert_no_screen_text(&response);
                                log.push((response.clone(), true));
                                last = Some((arguments, response));
                            }
                            _ => {
                                // A retry of this client's last request, whoever holds the lease.
                                let Some((arguments, first)) = last.clone() else {
                                    continue;
                                };
                                let response = act(&mut conn, arguments.clone());
                                assert_no_screen_text(&response);
                                if refused_without_lease(&first) {
                                    // Never recorded: the retry is a new request.
                                    log.push((response.clone(), true));
                                    last = Some((arguments, response));
                                } else {
                                    assert_eq!(
                                        response, first,
                                        "a {kind:?} client's retry replays its own result"
                                    );
                                    log.push((response, false));
                                }
                            }
                        }
                    }
                    conn.close();
                    (kind, log)
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });

    let mut per_snapshot: BTreeMap<String, usize> = BTreeMap::new();
    let mut deliveries = 0;
    for (kind, log) in &logs {
        for (response, _) in log.iter().filter(|(r, new)| *new && delivered(r)) {
            assert!(kind.delivers(), "a {kind:?} client delivered: {response}");
            let snapshot = response["snapshot_id"].as_str().unwrap().to_owned();
            *per_snapshot.entry(snapshot).or_default() += 1;
            deliveries += 1;
        }
    }
    assert!(
        per_snapshot.values().all(|n| *n == 1),
        "a snapshot authorised two deliveries: {per_snapshot:?}"
    );
    assert_eq!(world.events(), deliveries, "the events are the deliveries");
    assert!(deliveries > 0, "something was delivered");
    let refusals = logs
        .iter()
        .flat_map(|(_, log)| log)
        .filter(|(r, _)| refused_without_lease(r))
        .count();
    assert!(refusals > 0, "the lease refused something");
    let replays = logs
        .iter()
        .flat_map(|(_, log)| log)
        .filter(|(_, new)| !new)
        .count();
    assert!(replays > 0, "retries were replayed");
    let retained = broker.retained().unwrap();
    assert!(retained.clients.is_empty());
    assert!(
        !retained.grant_installed,
        "every grant was installed and removed inside one critical section"
    );
}

// ---- CT-12: where a client stands, in `_meta` (ADR-0039, proposed, decision 5) ----

/// A result's `io.ommatix/lease`, if it has one.
fn lease_of(result: &Value) -> Option<&Value> {
    result
        .get("_meta")
        .and_then(|meta| meta.get(LEASE_META_KEY))
}

/// The `io.ommatix/lease` value a test expects.
fn standing(state: &str, number: Option<u64>, position: Option<u64>, age_ms: Option<u64>) -> Value {
    json!({
        "state": state,
        "number": number,
        "position": position,
        "shared": age_ms.is_some(),
        "age_ms": age_ms,
    })
}

#[test]
fn ct_12_a_sole_client_is_silent_until_it_sends_lease_and_is_then_told_it_holds_the_lease() {
    let rig = rig();
    let mut a = rig.connect(Some(full()));
    let observed = observe_with(&mut a, json!({}));
    assert_eq!(lease_of(&observed), None, "{observed}");
    let page = observed["structuredContent"].clone();
    let acted = act_result(&mut a, click(&page, "a1", false));
    assert_eq!(lease_of(&acted), None, "{acted}");
    for lease in ["acquire", "release"] {
        let asked = observe_with(&mut a, json!({ "lease": lease }));
        assert_eq!(asked["isError"], false, "{asked}");
        assert_eq!(
            lease_of(&asked),
            Some(&standing("holder", None, None, None)),
            "the implicit lease has no number: {asked}"
        );
        let page = asked["structuredContent"].clone();
        let acted = act_result(&mut a, click(&page, &format!("after-{lease}"), true));
        assert!(delivered(&acted["structuredContent"]), "{acted}");
        assert_eq!(lease_of(&acted), None, "the action did not ask: {acted}");
    }
    let metrics = call_ok(a.connection(), 1, "get_engine_metrics", json!({}));
    assert_eq!(lease_of(&metrics), None);
}

#[test]
#[allow(clippy::too_many_lines)] // one story of three clients, each step beside its `_meta`
fn ct_12_with_two_clients_every_observation_and_action_says_where_the_client_stands() {
    let rig = rig();
    let mut a = rig.connect(Some(full()));
    let mut b = rig.connect(Some(full()));

    // a's implicit lease became lease 1 when b connected. With nothing published yet, b gets
    // the broker's STALE_SNAPSHOT and its standing; a, the holder, publishes.
    let first = observe_with(&mut b, json!({}));
    assert_eq!(code(&first["structuredContent"]), Some("STALE_SNAPSHOT"));
    assert_eq!(
        lease_of(&first),
        Some(&standing("none", None, None, None)),
        "{first}"
    );
    let held = observe_with(&mut a, json!({}));
    assert_eq!(
        lease_of(&held),
        Some(&standing("holder", Some(1), None, None))
    );
    rig.world.advance(Duration::from_millis(300));

    // b is served a's page, 300 ms old; asking for the lease, it waits first in line.
    let served = observe_with(&mut b, json!({}));
    assert_eq!(
        lease_of(&served),
        Some(&standing("none", None, None, Some(300)))
    );
    assert_eq!(
        served["structuredContent"]["snapshot_id"],
        held["structuredContent"]["snapshot_id"]
    );
    let waiting = observe_with(&mut b, json!({ "lease": "acquire" }));
    assert_eq!(
        lease_of(&waiting),
        Some(&standing("waiting", None, Some(1), Some(300)))
    );
    let mut c = rig.connect(Some(full()));
    let second = observe_with(&mut c, json!({ "lease": "acquire" }));
    assert_eq!(
        lease_of(&second),
        Some(&standing("waiting", None, Some(2), Some(300)))
    );

    // An action is never a shared page; metrics never carry the lease.
    let page = held["structuredContent"].clone();
    let dry = act_result(&mut b, click(&page, "b1", false));
    assert_eq!(dry["structuredContent"]["status"], "simulated");
    assert_eq!(
        lease_of(&dry),
        Some(&standing("waiting", None, Some(1), None))
    );
    let refused = act_result(&mut c, click(&page, "c1", true));
    assert!(refused_without_lease(&refused["structuredContent"]));
    assert_eq!(
        lease_of(&refused),
        Some(&standing("waiting", None, Some(2), None))
    );
    let live = act_result(&mut a, click(&page, "a1", true));
    assert!(delivered(&live["structuredContent"]), "{live}");
    assert_eq!(
        lease_of(&live),
        Some(&standing("holder", Some(1), None, None))
    );
    let metrics = call_ok(a.connection(), 1, "get_engine_metrics", json!({}));
    assert_eq!(lease_of(&metrics), None);

    // a releases on the wire: the lease passes to b as lease 2 and spends the page; a is served
    // that spent page and b, the new holder, publishes.
    let released = observe_with(&mut a, json!({ "lease": "release" }));
    assert_eq!(
        lease_of(&released),
        Some(&standing("none", None, None, Some(300)))
    );
    let stale = act_result(&mut b, click(&page, "b2", true));
    assert_eq!(code(&stale["structuredContent"]), Some("STALE_SNAPSHOT"));
    assert_eq!(
        lease_of(&stale),
        Some(&standing("holder", Some(2), None, None))
    );
    let fresh = observe_with(&mut b, json!({}));
    assert_eq!(
        lease_of(&fresh),
        Some(&standing("holder", Some(2), None, None))
    );
    assert_ne!(
        fresh["structuredContent"]["snapshot_id"],
        page["snapshot_id"]
    );
    let moved_up = observe_with(&mut c, json!({}));
    assert_eq!(
        lease_of(&moved_up),
        Some(&standing("waiting", None, Some(1), Some(0)))
    );

    // b's term ends: c is granted lease 3 when it next enters, and publishes.
    rig.world.advance(TERM);
    let turn = observe_with(&mut c, json!({}));
    assert_eq!(
        lease_of(&turn),
        Some(&standing("holder", Some(3), None, None))
    );
    let after = observe_with(&mut b, json!({}));
    assert_eq!(
        lease_of(&after),
        Some(&standing("none", None, None, Some(0)))
    );

    // Down to one client, which never asked: silent again.
    drop(b);
    drop(c);
    let alone = observe_with(&mut a, json!({}));
    assert_eq!(lease_of(&alone), None, "{alone}");
}

#[test]
fn ct_12_what_the_lease_table_refuses_is_told_in_meta_and_the_observation_is_answered() {
    let rig = rig();
    let mut a = rig.connect(Some(full()));
    let mut b = rig.connect(Some(full()));
    // The holder renews while nobody waits.
    let renewed = observe_with(&mut a, json!({ "lease": "acquire" }));
    assert_eq!(
        lease_of(&renewed),
        Some(&standing("holder", Some(1), None, None)),
        "a renewal keeps its number"
    );
    assert_eq!(b.acquire(), Ok(Acquired::Queued { position: 1 }));

    // A renewal while b waits is refused by the table: a still holds lease 1 and observes.
    let refused = observe_with(&mut a, json!({ "lease": "acquire" }));
    assert_eq!(refused["isError"], false, "{refused}");
    assert!(refused["structuredContent"]["elements"].is_array());
    assert_eq!(
        lease_of(&refused),
        Some(&standing("holder", Some(1), None, None))
    );
    // c neither holds the lease nor waits, and releases: refused by the table, nothing changes.
    let mut c = rig.connect(Some(full()));
    let not_holder = observe_with(&mut c, json!({ "lease": "release" }));
    assert_eq!(not_holder["isError"], false, "{not_holder}");
    assert_eq!(
        lease_of(&not_holder),
        Some(&standing("none", None, None, Some(0)))
    );
    // b asks again and keeps its place.
    let again = observe_with(&mut b, json!({ "lease": "acquire" }));
    assert_eq!(
        lease_of(&again),
        Some(&standing("waiting", None, Some(1), Some(0)))
    );
}

#[test]
fn ct_12_a_waiter_that_releases_leaves_the_queue_and_asks_again_at_the_back() {
    let rig = rig();
    let mut a = rig.connect(Some(full()));
    let mut b = rig.connect(Some(full()));
    let mut c = rig.connect(Some(full()));
    let mut d = rig.connect(Some(full()));
    let a_page = observe_with(&mut a, json!({}))["structuredContent"].clone();
    for (conn, position) in [(&mut b, 1), (&mut c, 2), (&mut d, 3)] {
        let asked = observe_with(conn, json!({ "lease": "acquire" }));
        assert_eq!(
            lease_of(&asked),
            Some(&standing("waiting", None, Some(position), Some(0)))
        );
    }

    // b leaves the queue: it is told it neither holds nor waits; the others move up one place.
    let left = observe_with(&mut b, json!({ "lease": "release" }));
    assert_eq!(left["isError"], false, "{left}");
    assert_eq!(
        lease_of(&left),
        Some(&standing("none", None, None, Some(0)))
    );
    // A waiter leaving passes the lease to nobody and spends nothing: the holder still delivers
    // on the snapshot it already has.
    let held = act(&mut a, click(&a_page, "a1", true));
    assert!(
        delivered(&held),
        "a waiter's leaving spent the snapshot: {held}"
    );
    for (conn, position) in [(&mut c, 1), (&mut d, 2)] {
        let moved = observe_with(conn, json!({}));
        assert_eq!(
            lease_of(&moved),
            Some(&standing("waiting", None, Some(position), Some(0)))
        );
    }
    // b asks again and waits at the back.
    let back = observe_with(&mut b, json!({ "lease": "acquire" }));
    assert_eq!(
        lease_of(&back),
        Some(&standing("waiting", None, Some(3), Some(0)))
    );

    // a's term ends: the lease goes to c, which waited first, not to b, which had been first.
    rig.world.advance(TERM);
    let turn = observe_with(&mut c, json!({}));
    assert_eq!(
        lease_of(&turn),
        Some(&standing("holder", Some(2), None, None))
    );
    let behind = observe_with(&mut b, json!({}));
    assert_eq!(
        lease_of(&behind),
        Some(&standing("waiting", None, Some(2), Some(0)))
    );
    // c releases on the wire: the lease passes to d, then b is first.
    let _ = observe_with(&mut c, json!({ "lease": "release" }));
    let next = observe_with(&mut d, json!({}));
    assert_eq!(
        lease_of(&next),
        Some(&standing("holder", Some(3), None, None))
    );
    let page = next["structuredContent"].clone();
    assert!(delivered(&act(&mut d, click(&page, "d1", true))));
    let first = observe_with(&mut b, json!({}));
    assert_eq!(
        lease_of(&first),
        Some(&standing("waiting", None, Some(1), Some(0)))
    );
    assert_eq!(rig.world.events(), 2, "a's delivery and d's");
}

#[test]
fn ct_12_with_nothing_published_a_waiter_gets_stale_snapshot_and_its_standing() {
    let rig = rig();
    let a = rig.connect(Some(full()));
    let mut b = rig.connect(Some(full()));
    assert!(matches!(a.acquire(), Ok(Acquired::Renewed { number: 1 })));
    let nothing = observe_with(&mut b, json!({ "lease": "acquire" }));
    assert_eq!(nothing["isError"], true, "{nothing}");
    assert_eq!(
        code(&nothing["structuredContent"]),
        Some("STALE_SNAPSHOT"),
        "{nothing}"
    );
    assert_eq!(
        lease_of(&nothing),
        Some(&standing("waiting", None, Some(1), None)),
        "no page was served"
    );
    let lease = lease_of(&nothing).unwrap();
    assert_eq!(lease["shared"], false);
    assert_eq!(lease["age_ms"], Value::Null);
    common::assert_valid("lease.meta", lease);
}

#[test]
fn ct_12_lease_is_applied_before_the_observation_and_goes_with_a_cursor() {
    let rig = rig();
    let mut a = rig.connect(Some(full()));
    let mut b = rig.connect(Some(full()));
    let first = observe_with(&mut a, json!({ "limit": 1 }))["structuredContent"].clone();
    let cursor = first["next_cursor"].clone();
    assert!(cursor.is_string(), "{first}");

    // b waits and reads the second page of a's snapshot through the cursor.
    let paged = observe_with(
        &mut b,
        json!({ "limit": 1, "cursor": cursor, "lease": "acquire" }),
    );
    assert_eq!(paged["isError"], false, "{paged}");
    assert_eq!(
        paged["structuredContent"]["snapshot_id"],
        first["snapshot_id"]
    );
    assert_ne!(
        paged["structuredContent"]["elements"], first["elements"],
        "the page the cursor names"
    );
    assert_eq!(
        lease_of(&paged),
        Some(&standing("waiting", None, Some(1), Some(0)))
    );

    // a's term ends; b's next `acquire` finds the lease passed to it before it observes, so
    // this very observation publishes a new snapshot.
    rig.world.advance(TERM);
    let granted = observe_with(&mut b, json!({ "lease": "acquire" }));
    assert_eq!(
        lease_of(&granted),
        Some(&standing("holder", Some(2), None, None))
    );
    assert_ne!(
        granted["structuredContent"]["snapshot_id"],
        first["snapshot_id"]
    );
    let page = granted["structuredContent"].clone();
    assert!(delivered(&act(&mut b, click(&page, "b1", true))));
}

/// A window with many labelled buttons: its first page does not fit in a 4 KiB message.
fn crowded_scene() -> FixtureScene {
    let mut scene = FixtureScene::new(640, 640);
    for i in 0..40_u32 {
        let y = f64::from(i) * 15.0;
        scene.add_control(confirmed(Role::Button, bx(10.0, y, 600.0, y + 12.0)));
        scene.add_word(
            format!("Button-number-{i:02}-with-a-long-label-for-size"),
            bx(15.0, y + 1.0, 590.0, y + 11.0),
        );
    }
    scene
}

#[test]
fn ct_12_an_observation_too_large_for_one_message_keeps_its_lease_meta() {
    let world = World::with_scene(crowded_scene());
    let config = McpConfig {
        max_message_bytes: 4096,
        ..McpConfig::default()
    };
    let broker = world.broker(config, 8);
    let mut a = broker.connect(Some(full())).unwrap();
    let mut b = broker.connect(Some(full())).unwrap();
    let too_large = observe_with(&mut a, json!({}));
    assert_eq!(
        code(&too_large["structuredContent"]),
        Some("LIMIT_EXCEEDED")
    );
    assert_eq!(
        lease_of(&too_large),
        Some(&standing("holder", Some(1), None, None))
    );
    // Non-holders whose shared page is too large: the lease was applied and each client learns
    // where it stands, but the error carries no page, so no shared page and no age (R-U4c-1).
    // Time passes first, so that a page's age copied onto the error would not be 0.
    let mut c = broker.connect(Some(full())).unwrap();
    world.advance(Duration::from_millis(300));
    let asked = observe_with(&mut b, json!({ "lease": "acquire" }));
    assert_eq!(asked["isError"], true, "{asked}");
    assert_eq!(code(&asked["structuredContent"]), Some("LIMIT_EXCEEDED"));
    assert_eq!(
        lease_of(&asked),
        Some(&standing("waiting", None, Some(1), None)),
        "the lease was applied, and the client learns where it stands"
    );
    let plain = observe_with(&mut c, json!({}));
    assert_eq!(code(&plain["structuredContent"]), Some("LIMIT_EXCEEDED"));
    assert_eq!(lease_of(&plain), Some(&standing("none", None, None, None)));
    // The holder's own page, too large again.
    let again = observe_with(&mut a, json!({}));
    assert_eq!(code(&again["structuredContent"]), Some("LIMIT_EXCEEDED"));
    assert_eq!(
        lease_of(&again),
        Some(&standing("holder", Some(1), None, None))
    );
    for result in [&too_large, &asked, &plain, &again] {
        common::assert_valid("lease.meta", lease_of(result).unwrap());
    }
    // The stdio server's own LIMIT_EXCEEDED carries no lease.
    let mut stdio = world.server(config).connect();
    let response = call_ok(
        &mut stdio,
        1,
        "observe_screen",
        json!({ "lease": "acquire" }),
    );
    assert_eq!(code(&response["structuredContent"]), Some("LIMIT_EXCEEDED"));
    assert_eq!(lease_of(&response), None);
}

// ---- CT-13: the stdio server accepts `lease` and changes nothing ----

/// One modern `tools/call` line.
fn tool_line(id: u64, name: &str, arguments: &Value) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call",
            "params": { "_meta": modern_meta(), "name": name, "arguments": arguments } })
    .to_string()
}

#[test]
fn ct_13_the_stdio_server_accepts_lease_and_answers_byte_for_byte_as_without_it() {
    {
        let config = McpConfig::default();
        let (with_world, without_world) = (World::new(), World::new());
        let mut with = with_world.server(config).connect();
        let mut without = without_world.server(config).connect();
        let mut id = 0;
        let mut both = |name: &str, plain: Value, lease: Option<&str>| -> Value {
            id += 1;
            let mut asked = plain.clone();
            if let Some(lease) = lease {
                asked["lease"] = json!(lease);
            }
            let line = with.handle_message(tool_line(id, name, &asked).as_bytes());
            let other = without.handle_message(tool_line(id, name, &plain).as_bytes());
            assert_eq!(line, other, "{name} with lease {lease:?}");
            let response: Value = serde_json::from_str(&line.unwrap()).unwrap();
            assert_eq!(lease_of(&response["result"]), None, "{response}");
            response["result"].clone()
        };
        let first =
            both("observe_screen", json!({ "limit": 2 }), Some("acquire"))["structuredContent"]
                .clone();
        let cursor = first["next_cursor"].clone();
        both(
            "observe_screen",
            json!({ "limit": 2, "cursor": cursor }),
            Some("release"),
        );
        let page = both(
            "observe_screen",
            json!({ "include_values": true }),
            Some("release"),
        )["structuredContent"]
            .clone();
        both("execute_action", click(&page, "dry", false), None);
        assert!(delivered(
            &both("execute_action", click(&page, "live", true), None)["structuredContent"]
        ));
        both("observe_screen", json!({}), Some("acquire"));
        both("get_engine_metrics", json!({}), None);
        // The invalid value is refused on the stdio server too.
        let mut bad = with_world.server(config).connect();
        let refused = call(&mut bad, 1, "observe_screen", json!({ "lease": "x" }));
        assert_eq!(error_code(&refused), -32602, "{refused}");
    }
}

// ---- the blocking loop agrees with the asynchronous one (ADR-0015, ADR-0043) ----

/// A reader that hands out at most `chunk` bytes per read and fails once it reaches `fails_at`.
struct Trickle {
    data: Vec<u8>,
    at: usize,
    chunk: usize,
    fails_at: Option<usize>,
}

impl Trickle {
    fn new(data: &[u8], chunk: usize, fails_at: Option<usize>) -> Self {
        Self {
            data: data.to_vec(),
            at: 0,
            chunk,
            fails_at,
        }
    }

    /// The range of `data` the next read hands out, at most `room` bytes.
    fn take(&mut self, room: usize) -> io::Result<std::ops::Range<usize>> {
        if self.fails_at.is_some_and(|end| self.at >= end) {
            return Err(io::Error::new(
                io::ErrorKind::ConnectionReset,
                "the test reader fails",
            ));
        }
        let end = self.fails_at.unwrap_or(usize::MAX).min(self.data.len());
        let n = self.chunk.min(room).min(end - self.at);
        let range = self.at..self.at + n;
        self.at += n;
        Ok(range)
    }
}

impl io::Read for Trickle {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let range = self.take(buf.len())?;
        let n = range.len();
        buf[..n].copy_from_slice(&self.data[range]);
        Ok(n)
    }
}

impl AsyncRead for Trickle {
    fn poll_read(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        Poll::Ready(
            this.take(buf.remaining())
                .map(|range| buf.put_slice(&this.data[range])),
        )
    }
}

/// A writer that keeps what it is given and fails once it holds `budget` bytes.
struct Sink {
    out: Vec<u8>,
    budget: Option<usize>,
}

impl Sink {
    fn new(budget: Option<usize>) -> Self {
        Self {
            out: Vec::new(),
            budget,
        }
    }

    fn accept(&mut self, buf: &[u8]) -> io::Result<usize> {
        let room = self
            .budget
            .map_or(usize::MAX, |budget| budget.saturating_sub(self.out.len()));
        if room == 0 && !buf.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "the test writer fails",
            ));
        }
        let n = buf.len().min(room);
        self.out.extend_from_slice(&buf[..n]);
        Ok(n)
    }
}

impl io::Write for Sink {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.accept(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl AsyncWrite for Sink {
    fn poll_write(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Poll::Ready(self.get_mut().accept(buf))
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Loop {
    Asynchronous,
    Blocking,
}

/// One stream and how its two ends behave.
#[derive(Debug, Clone)]
struct Stream {
    input: Vec<u8>,
    chunk: usize,
    read_fails_at: Option<usize>,
    write_budget: Option<usize>,
}

impl Stream {
    fn plain(input: Vec<u8>, chunk: usize) -> Self {
        Self {
            input,
            chunk,
            read_fails_at: None,
            write_budget: None,
        }
    }
}

/// What one loop did with a stream, on a fresh broker: what it wrote, how it ended, what the
/// broker retains afterwards, what reached the desktop, and whether the handshake's ticket is
/// spent.
#[derive(Debug, PartialEq, Eq)]
struct Run {
    output: Vec<u8>,
    ended: Result<Option<Admission>, io::ErrorKind>,
    retained: BrokerState,
    events: usize,
    captures: usize,
    spent: bool,
}

/// The ticket the loop runs' store lists with [`full`].
fn listed_ticket() -> String {
    ticket(1)
}

/// Serves `stream` with one loop on a fresh broker: a client connected with [`full`], or, with
/// `handshake`, a client admitted by the stream's first line.
fn run_loop(which: Loop, handshake: bool, config: McpConfig, stream: &Stream) -> Run {
    let world = World::new();
    let grants = Arc::new(Grants::default());
    grants.set(&listed_ticket(), Some(full()));
    let broker = world.ticket_broker_with(config, grants, 8);
    let reader = Trickle::new(&stream.input, stream.chunk, stream.read_fails_at);
    let mut sink = Sink::new(stream.write_budget);
    let ended = match (which, handshake) {
        (Loop::Blocking, false) => broker
            .connect(Some(full()))
            .unwrap()
            .serve_sync(reader, &mut sink)
            .map(|()| None),
        (Loop::Blocking, true) => broker
            .serve_sync_with_handshake(reader, &mut sink)
            .map(Some),
        (Loop::Asynchronous, false) => runtime()
            .block_on(
                broker
                    .connect(Some(full()))
                    .unwrap()
                    .serve(reader, &mut sink),
            )
            .map(|()| None),
        (Loop::Asynchronous, true) => runtime()
            .block_on(broker.serve_with_handshake(reader, &mut sink))
            .map(Some),
    }
    .map_err(|error| error.kind());
    let retained = broker.retained().expect("the broker is available");
    let spent = broker.admit(listed_ticket().into_bytes()).err() == Some(BrokerError::Refused);
    Run {
        output: sink.out,
        ended,
        retained,
        events: world.events(),
        captures: world.captures(),
        spent,
    }
}

/// Runs both loops and checks that they agree, byte for byte and in the state they leave.
fn both_loops(handshake: bool, config: McpConfig, stream: &Stream) -> Run {
    let blocking = run_loop(Loop::Blocking, handshake, config, stream);
    let asynchronous = run_loop(Loop::Asynchronous, handshake, config, stream);
    assert_eq!(blocking, asynchronous, "{stream:?}");
    blocking
}

/// A session of every kind of line, with a delivery, its replay and the stdio server's bounds,
/// ended alternately by `\r\n` and `\n`. The page the actions cite is learnt on a probe world,
/// which publishes the same identifiers as every fresh world.
fn scripted_stream() -> Vec<u8> {
    let prefix = [
        padded_request(1, 300),
        tool_line(2, "observe_screen", &json!({ "limit": 2 })),
        tool_line(3, "observe_screen", &json!({ "include_values": true })),
    ];
    let world = World::new();
    let broker = world.broker(McpConfig::default(), 8);
    let mut probe = broker.connect(Some(full())).unwrap();
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
fn the_blocking_loop_serves_a_scripted_session_byte_for_byte_as_the_asynchronous_loop() {
    let script = scripted_stream();
    for handshake in [false, true] {
        for chunk in [1, 7, 8192] {
            let mut input = Vec::new();
            if handshake {
                input.extend_from_slice(listed_ticket().as_bytes());
                input.push(b'\n');
            }
            input.extend_from_slice(&script);
            let run = both_loops(
                handshake,
                McpConfig::default(),
                &Stream::plain(input, chunk),
            );
            assert_eq!(run.ended, Ok(handshake.then_some(Admission::Served)));
            let lines = served_lines(run.output);
            assert_eq!(lines.len(), 11, "{lines:?}");
            let responses: Vec<Value> = lines
                .iter()
                .map(|l| serde_json::from_str(l).unwrap())
                .collect();
            assert_eq!(
                value_of(&responses[2]["result"]["structuredContent"]),
                Some(FIELD_VALUE)
            );
            assert!(delivered(&responses[4]["result"]["structuredContent"]));
            assert_eq!(
                responses[5]["result"]["structuredContent"],
                responses[4]["result"]["structuredContent"],
                "the retry replays"
            );
            assert_eq!(error_code(&responses[7]), -32602);
            assert_eq!(error_code(&responses[8]), -32700);
            assert_eq!(error_code(&responses[9]), -32600, "oversized");
            assert_eq!(responses[10]["id"], 9, "the stream recovers");
            assert_eq!(run.events, 1, "one delivery");
            assert!(run.retained.clients.is_empty(), "the client left");
            assert_eq!(run.retained.engine["snapshots"], 0, "the last one purged");
            assert_eq!(run.spent, handshake, "the handshake spent its ticket");
        }
    }
}

#[test]
#[allow(clippy::too_many_lines)] // one generator: each kind of line beside the others
fn the_blocking_and_asynchronous_loops_agree_on_generated_byte_streams() {
    const CASES: usize = 160;
    // The smallest limit, so that oversized lines stay cheap.
    let config = McpConfig {
        legacy_enabled: false,
        max_message_bytes: 4096,
    };
    let limit = config.message_limit();
    let mut rng = Rng(0x0B10_C4ED);
    let mut endings: BTreeMap<String, usize> = BTreeMap::new();
    for case in 0..CASES {
        let handshake = rng.chance(50);
        let mut input = Vec::new();
        if handshake {
            let line = match rng.below(5) {
                0 | 1 => listed_ticket(),
                2 => String::new(),
                3 => ticket(2),
                _ => "not a ticket".to_owned(),
            };
            input.extend_from_slice(line.as_bytes());
            input.extend_from_slice(if rng.chance(30) { b"\r\n" } else { b"\n" });
        }
        let mut id = 0_u64;
        for _ in 0..rng.below(8) {
            id += 1;
            let line: Vec<u8> = match rng.below(10) {
                0 | 1 => tool_line(
                    id,
                    "observe_screen",
                    &json!({ "include_values": rng.chance(50) }),
                )
                .into_bytes(),
                2 => tool_line(id, "get_engine_metrics", &json!({})).into_bytes(),
                3 => padded_request(id, 300).into_bytes(),
                4 => padded_request(id, limit).into_bytes(),
                5 => vec![b'x'; limit + 1 + rng.below(3 * limit)],
                6 => b"{not json".to_vec(),
                7 => tool_line(id, "execute_action", &json!({ "x": 1 })).into_bytes(),
                8 => br#"{"jsonrpc":"2.0","method":"notifications/cancelled"}"#.to_vec(),
                _ => b"   ".to_vec(),
            };
            input.extend_from_slice(&line);
            input.extend_from_slice(if rng.chance(30) { b"\r\n" } else { b"\n" });
        }
        match rng.below(4) {
            // A final line without its terminator.
            0 => input.extend_from_slice(padded_request(99, 300).as_bytes()),
            // An oversized tail at the end of input.
            1 => input.extend_from_slice(&vec![b'y'; limit + 5]),
            _ => {}
        }
        // An early end of input, anywhere, mid-line included.
        if rng.chance(25) && !input.is_empty() {
            let at = rng.below(input.len());
            input.truncate(at);
        }
        let length = input.len();
        let stream = Stream {
            input,
            chunk: [1, 3, 64, 8192][rng.below(4)],
            read_fails_at: rng.chance(15).then(|| rng.below(length + 1)),
            write_budget: rng.chance(20).then(|| rng.below(3_000)),
        };
        let run = both_loops(handshake, config, &stream);
        let ending = match &run.ended {
            Ok(None) => "served".to_owned(),
            Ok(Some(admission)) => format!("{admission:?}"),
            Err(kind) => format!("{kind:?}"),
        };
        *endings.entry(ending).or_default() += 1;
        assert!(run.retained.clients.is_empty(), "case {case}");
        assert_eq!(run.retained.engine["snapshots"], 0, "case {case}");
        assert!(!run.retained.grant_installed, "case {case}");
    }
    for ending in [
        "served",
        "Served",
        "Refused(Refused)",
        "BrokenPipe",
        "ConnectionReset",
    ] {
        assert!(endings.contains_key(ending), "{ending} never: {endings:?}");
    }
}

// ---- grants asked on every call (ADR-0040, decision 5) ----

/// An observation with values asked for: its page.
fn see(conn: &mut BrokerConnection) -> Value {
    observe_with(conn, json!({ "include_values": true }))["structuredContent"].clone()
}

#[test]
fn a_grant_withdrawn_or_changed_in_the_store_takes_effect_at_the_clients_next_call() {
    let world = World::new();
    let grants = Arc::new(Grants::default());
    let broker = world.ticket_broker(grants.clone(), 8);
    let presented = ticket(1);
    grants.set(&presented, Some(full()));
    let mut conn = broker.admit(presented.clone().into_bytes()).unwrap();
    assert_eq!(grants.asks(), 0, "the handshake asks nothing");

    let page = see(&mut conn);
    assert_eq!(value_of(&page), Some(FIELD_VALUE));
    assert!(delivered(&act(&mut conn, click(&page, "g1", true))));
    assert_eq!(grants.asks(), 2, "once per call");

    // Withdrawn: the next observation hides values and the next live action is refused.
    grants.set(&presented, None);
    let page = see(&mut conn);
    assert_eq!(value_of(&page), None);
    let refused = act(&mut conn, click(&page, "g2", true));
    assert_eq!(code(&refused), Some("POLICY_DENIED"), "{refused}");
    assert!(!refused_without_lease(&refused), "the gate's refusal");
    assert_eq!(world.events(), 1);

    // Restored: the call after that passes.
    grants.set(&presented, Some(full()));
    let page = see(&mut conn);
    assert_eq!(value_of(&page), Some(FIELD_VALUE));
    assert!(delivered(&act(&mut conn, click(&page, "g3", true))));
    assert_eq!(world.events(), 2);

    // Changed: the application removed from the grant.
    grants.set(
        &presented,
        Some(Grant {
            applications: BTreeSet::new(),
            ..full()
        }),
    );
    let page = see(&mut conn);
    let refused = act(&mut conn, click(&page, "g4", true));
    assert_eq!(code(&refused), Some("POLICY_DENIED"), "{refused}");
    assert_eq!(world.events(), 2);
    grants.set(&presented, Some(full()));
    let page = see(&mut conn);
    assert!(delivered(&act(&mut conn, click(&page, "g5", true))));
    assert_eq!(world.events(), 3);

    // Every tool call asks once; a request that never reaches a tool asks nothing.
    metrics(&mut conn);
    let invalid = call(conn.connection(), 9, "execute_action", json!({ "x": 1 }));
    assert_eq!(error_code(&invalid), -32602);
    common::modern(conn.connection(), 10, "tools/list", json!({}));
    assert_eq!(grants.asks(), 11);
    let asked = grants.asked.lock().unwrap().clone();
    assert!(asked.iter().all(|hash| *hash == sha256(&presented)));
    assert!(!broker.retained().unwrap().grant_installed);
}

/// A store that checks, at every question, that the broker's lock is held: a thread that asks
/// the broker what it retains must still be waiting when the store answers.
#[derive(Default)]
struct Probing {
    broker: Mutex<Option<Broker>>,
    held: Mutex<Vec<bool>>,
    waiters: Mutex<Vec<thread::JoinHandle<()>>>,
}

impl GrantStore for Probing {
    fn grant(&self, _: &[u8; 32]) -> Option<Grant> {
        let broker = self
            .broker
            .lock()
            .unwrap()
            .clone()
            .expect("the broker is set");
        let (sent, received) = mpsc::channel();
        let waiter = thread::spawn(move || {
            let _ = broker.retained();
            let _ = sent.send(());
        });
        let held = received.recv_timeout(Duration::from_millis(300)).is_err();
        self.held.lock().unwrap().push(held);
        self.waiters.lock().unwrap().push(waiter);
        Some(full())
    }
}

#[test]
fn the_store_is_asked_once_per_call_inside_the_brokers_one_lock() {
    let world = World::new();
    let probing = Arc::new(Probing::default());
    let broker = world.ticket_broker(probing.clone(), 8);
    *probing.broker.lock().unwrap() = Some(broker.clone());
    let mut conn = broker.admit(ticket(1).into_bytes()).unwrap();
    let page = observe(&mut conn);
    act(&mut conn, click(&page, "l1", false));
    metrics(&mut conn);
    let waiters: Vec<_> = probing.waiters.lock().unwrap().drain(..).collect();
    for waiter in waiters {
        waiter.join().unwrap();
    }
    assert_eq!(*probing.held.lock().unwrap(), vec![true, true, true]);
    // The store holds the broker: break the cycle.
    probing.broker.lock().unwrap().take();
}

#[test]
fn a_client_connected_with_a_grant_handed_in_is_never_looked_up_in_the_store() {
    let world = World::new();
    let grants = Arc::new(Grants::default());
    grants.set(&ticket(1), Some(full()));
    let broker = world.ticket_broker(grants.clone(), 8);
    let mut handed = broker.connect(Some(full())).unwrap();
    let page = see(&mut handed);
    assert_eq!(value_of(&page), Some(FIELD_VALUE));
    assert!(delivered(&act(&mut handed, click(&page, "h1", true))));
    metrics(&mut handed);
    handed.close();
    // An empty handshake: no ticket, no grant, nothing asked.
    let mut empty = broker.admit(Vec::new()).unwrap();
    let page = see(&mut empty);
    assert_eq!(value_of(&page), None);
    let refused = act(&mut empty, click(&page, "h2", true));
    assert_eq!(code(&refused), Some("POLICY_DENIED"), "{refused}");
    assert_eq!(grants.asks(), 0);
    assert_eq!(world.events(), 1);
}

#[test]
fn a_store_that_lists_nothing_or_panics_grants_nothing_and_the_broker_keeps_serving() {
    for panics in [false, true] {
        let world = World::new();
        let grants = Arc::new(Grants::default());
        grants.panics.store(panics, Ordering::SeqCst);
        let broker = world.ticket_broker(grants.clone(), 8);
        let mut ticketed = broker.admit(ticket(1).into_bytes()).unwrap();
        let page = see(&mut ticketed);
        assert_eq!(value_of(&page), None, "panics {panics}");
        let refused = act(&mut ticketed, click(&page, "p1", true));
        assert_eq!(code(&refused), Some("POLICY_DENIED"), "{refused}");
        assert!(!refused_without_lease(&refused));
        assert_eq!(grants.asks(), 2);
        let retained = broker.retained().expect("the broker is still available");
        assert!(!retained.grant_installed);

        // Another client goes on, while the first stays connected.
        let mut other = broker.connect(Some(full())).unwrap();
        assert_eq!(ticketed.acquire(), Ok(Acquired::Renewed { number: 1 }));
        ticketed.release().unwrap();
        assert!(matches!(other.acquire(), Ok(Acquired::Granted { .. })));
        let page = see(&mut other);
        assert_eq!(value_of(&page), Some(FIELD_VALUE));
        assert!(delivered(&act(&mut other, click(&page, "p2", true))));
        let served = see(&mut ticketed);
        assert_eq!(served["snapshot_id"], page["snapshot_id"]);
        assert_eq!(value_of(&served), None);
        assert_eq!(world.events(), 1);
        assert_eq!(grants.asks(), 3);
        assert!(broker.retained().is_some());
    }
}

// ---- the ticket handshake (ADR-0040 decision 3, ADR-0043 decision 9) ----

/// Serves `input` with a handshake on one loop of `broker`: how it ended and what was written.
fn shake(broker: &Broker, input: &[u8], which: Loop) -> (Admission, Vec<u8>) {
    let mut sink = Sink::new(None);
    let reader = Trickle::new(input, 5, None);
    let admission = match which {
        Loop::Blocking => broker.serve_sync_with_handshake(reader, &mut sink),
        Loop::Asynchronous => runtime().block_on(broker.serve_with_handshake(reader, &mut sink)),
    }
    .expect("no I/O error");
    (admission, sink.out)
}

/// A handshake line followed by an observation that asks for values.
fn handshake_then_observe(line: &[u8]) -> Vec<u8> {
    let mut input = line.to_vec();
    input.push(b'\n');
    input.extend_from_slice(
        tool_line(1, "observe_screen", &json!({ "include_values": true })).as_bytes(),
    );
    input.push(b'\n');
    input
}

/// A fresh broker with a bound of one client and a store that lists [`listed_ticket`].
fn one_client_broker() -> (World, Arc<Grants>, Broker) {
    let world = World::new();
    let grants = Arc::new(Grants::default());
    grants.set(&listed_ticket(), Some(full()));
    let broker = world.ticket_broker(grants.clone(), 1);
    (world, grants, broker)
}

#[test]
fn a_handshake_admits_a_listed_ticket_an_unlisted_one_and_an_empty_line() {
    let (listed, unlisted) = (listed_ticket(), ticket(2));
    let at_most = "Z".repeat(TICKET_MAX_LEN);
    let at_least = "-".repeat(TICKET_MIN_LEN);
    let mut crlf = listed.clone().into_bytes();
    crlf.push(b'\r');
    let admitted: [(&[u8], bool); 6] = [
        (listed.as_bytes(), true),
        (unlisted.as_bytes(), false),
        (b"", false),
        (at_most.as_bytes(), true),
        (at_least.as_bytes(), true),
        (&crlf, true),
    ];
    for (line, values) in admitted {
        for which in [Loop::Asynchronous, Loop::Blocking] {
            let (_world, grants, broker) = one_client_broker();
            grants.set(&at_most, Some(full()));
            grants.set(&at_least, Some(full()));
            let (admission, output) = shake(&broker, &handshake_then_observe(line), which);
            assert_eq!(admission, Admission::Served, "{line:?}");
            let lines = served_lines(output);
            assert_eq!(lines.len(), 1);
            let page =
                serde_json::from_str::<Value>(&lines[0]).unwrap()["result"]["structuredContent"]
                    .clone();
            assert_eq!(value_of(&page).is_some(), values, "{line:?} {which:?}");
            assert_eq!(grants.asks(), usize::from(!line.is_empty()));
            assert!(broker.retained().unwrap().clients.is_empty(), "it left");
        }
    }
}

#[test]
fn a_handshake_of_any_other_form_ends_the_connection_and_grants_nothing() {
    let (_world, _grants, probe) = one_client_broker();
    let limit = probe.config().message_limit();
    let a = "A".repeat(20);
    let listed = listed_ticket();
    let mut refused: Vec<(&str, Vec<u8>)> = vec![
        ("too short", "A".repeat(TICKET_MIN_LEN - 1).into_bytes()),
        ("too long", "A".repeat(TICKET_MAX_LEN + 1).into_bytes()),
        ("a space only", b" ".to_vec()),
        ("a trailing space", format!("{listed} ").into_bytes()),
        (
            "a byte-order mark",
            format!("\u{feff}{listed}").into_bytes(),
        ),
        ("a JSON-RPC message", padded_request(1, 300).into_bytes()),
    ];
    for forbidden in [" ", "+", "/", "=", ".", "\r", "\t", "\u{e9}"] {
        refused.push((
            "a forbidden character",
            format!("{a}{forbidden}{a}").into_bytes(),
        ));
    }
    let mut non_utf8 = a.clone().into_bytes();
    non_utf8.push(0xFF);
    non_utf8.extend_from_slice(a.as_bytes());
    refused.push(("a byte that is not UTF-8", non_utf8));
    let mut inputs: Vec<(&str, Vec<u8>)> = refused
        .into_iter()
        .map(|(what, line)| (what, handshake_then_observe(&line)))
        .collect();
    inputs.push((
        "over the bound",
        handshake_then_observe(&vec![b'A'; limit + 1]),
    ));
    inputs.push((
        "a listed ticket the input cuts short",
        listed.clone().into_bytes(),
    ));
    inputs.push(("no input", Vec::new()));

    let mut written: BTreeSet<Vec<u8>> = BTreeSet::new();
    for (what, input) in &inputs {
        for which in [Loop::Asynchronous, Loop::Blocking] {
            let (world, grants, broker) = one_client_broker();
            let (admission, output) = shake(&broker, input, which);
            assert_eq!(
                admission,
                Admission::Refused(BrokerError::Refused),
                "{what} {which:?}"
            );
            written.insert(output);
            let retained = broker.retained().unwrap();
            assert!(retained.clients.is_empty(), "{what}: no client entry");
            assert_eq!(grants.asks(), 0, "{what}");
            assert_eq!(world.captures(), 0, "{what}: no MCP message was read");
            // The bound on clients was not consumed, and nothing was spent.
            let admitted = broker
                .admit(listed.clone().into_bytes())
                .unwrap_or_else(|e| panic!("{what}: {e:?}"));
            admitted.close();
        }
    }
    assert_eq!(
        written,
        BTreeSet::from([Vec::new()]),
        "nothing is written back"
    );

    // The same lines handed to `admit` directly, where they are lines.
    for line in [
        "A".repeat(TICKET_MIN_LEN - 1),
        "A".repeat(TICKET_MAX_LEN + 1),
        format!("{a}+{a}"),
        format!("{a}\r{a}"),
        " ".to_owned(),
    ] {
        assert_eq!(
            probe.admit(line.into_bytes()).err(),
            Some(BrokerError::Refused)
        );
    }
    assert!(probe.retained().unwrap().clients.is_empty());
}

#[test]
fn a_spent_ticket_is_refused_while_its_first_connection_lasts_and_after_it_left() {
    let world = World::new();
    let grants = Arc::new(Grants::default());
    let listed = listed_ticket();
    let unlisted = ticket(2);
    grants.set(&listed, Some(full()));
    let broker = world.ticket_broker(grants.clone(), 8);
    let invalid = handshake_then_observe(b"not a ticket");
    let (_, invalid_bytes) = shake(&broker, &invalid, Loop::Blocking);

    let refused_again = |round: &str| {
        assert_eq!(
            broker.admit(listed.clone().into_bytes()).err(),
            Some(BrokerError::Refused),
            "{round}"
        );
        for which in [Loop::Asynchronous, Loop::Blocking] {
            let (admission, output) =
                shake(&broker, &handshake_then_observe(listed.as_bytes()), which);
            assert_eq!(admission, Admission::Refused(BrokerError::Refused));
            assert_eq!(
                output, invalid_bytes,
                "{round}: the same bytes as an invalid line"
            );
        }
    };
    let first = broker.admit(listed.clone().into_bytes()).unwrap();
    refused_again("while the first is connected");
    assert_eq!(broker.retained().unwrap().clients.len(), 1);
    first.close();
    refused_again("after it left");
    // An unlisted ticket is spent alike, and listing it afterwards does not revive it.
    broker.admit(unlisted.clone().into_bytes()).unwrap().close();
    grants.set(&unlisted, Some(full()));
    assert_eq!(
        broker.admit(unlisted.into_bytes()).err(),
        Some(BrokerError::Refused)
    );
    // A purge deletes perception state, not the spent tickets.
    world.operator.request_purge();
    broker.admit(Vec::new()).unwrap().close();
    assert!(!world.operator.purge_pending());
    assert_eq!(
        broker.admit(listed.into_bytes()).err(),
        Some(BrokerError::Refused)
    );
    assert!(broker.retained().unwrap().clients.is_empty());
}

#[test]
fn the_spent_set_is_bounded_and_a_full_set_admits_nobody_new_by_ticket() {
    let world = World::new();
    let grants = Arc::new(Grants::default());
    let broker = world.ticket_broker(grants.clone(), 8);
    let invalid = handshake_then_observe(b"not a ticket");
    let (_, invalid_bytes) = shake(&broker, &invalid, Loop::Asynchronous);
    let flood = u64::try_from(MAX_SPENT_TICKETS).unwrap();
    for n in 0..flood {
        broker
            .admit(ticket(1_000 + n).into_bytes())
            .unwrap_or_else(|e| panic!("ticket {n}: {e:?}"))
            .close();
    }
    let fresh = ticket(1_000 + flood);
    grants.set(&fresh, Some(full()));
    assert_eq!(
        broker.admit(fresh.clone().into_bytes()).err(),
        Some(BrokerError::Refused),
        "a full set admits nobody new by ticket"
    );
    for which in [Loop::Asynchronous, Loop::Blocking] {
        let (admission, output) = shake(&broker, &handshake_then_observe(fresh.as_bytes()), which);
        assert_eq!(admission, Admission::Refused(BrokerError::Refused));
        assert_eq!(output, invalid_bytes);
    }
    assert_eq!(
        broker.admit(ticket(1_000).into_bytes()).err(),
        Some(BrokerError::Refused),
        "a spent one is still refused"
    );
    // An empty handshake is still admitted, without a grant.
    let mut empty = broker.admit(Vec::new()).unwrap();
    assert_eq!(value_of(&see(&mut empty)), None);
    empty.close();
    assert_eq!(grants.asks(), 0);
    assert!(broker.retained().unwrap().clients.is_empty());
}

#[test]
fn a_ticket_never_appears_in_what_the_broker_writes_returns_or_formats() {
    let world = World::new();
    let grants = Arc::new(Grants::default());
    let broker = world.ticket_broker(grants.clone(), 8);
    let mut seen: Vec<String> = Vec::new();
    let tickets = [ticket(71), ticket(72)];
    for (presented, which) in tickets.iter().zip([Loop::Asynchronous, Loop::Blocking]) {
        grants.set(presented, Some(full()));
        let mut input = handshake_then_observe(presented.as_bytes());
        for line in [
            tool_line(2, "get_engine_metrics", &json!({})),
            tool_line(3, "execute_action", &json!({ "x": 1 })),
            "not json".to_owned(),
            "x".repeat(DEFAULT_MAX_MESSAGE_BYTES + 1),
        ] {
            input.extend_from_slice(line.as_bytes());
            input.push(b'\n');
        }
        let (admission, output) = shake(&broker, &input, which);
        assert_eq!(admission, Admission::Served);
        seen.push(String::from_utf8(output).unwrap());
        seen.push(format!("{admission:?}"));
        // Presented again, spent: the refusal, what `admit` returns, and their formats.
        let (refusal, output) = shake(
            &broker,
            &handshake_then_observe(presented.as_bytes()),
            which,
        );
        seen.push(String::from_utf8(output).unwrap());
        seen.push(format!("{refusal:?}"));
        let error = broker
            .admit(presented.clone().into_bytes())
            .err()
            .expect("spent");
        seen.push(format!("{error:?} {error}"));
        // A connection still open, by the same ticket's twin, while the broker is formatted.
        let mut open = broker.admit(format!("{presented}x").into_bytes()).unwrap();
        observe(&mut open);
        seen.push(format!("{:?}", broker.retained()));
        open.close();
    }
    for error in [
        BrokerError::Refused,
        BrokerError::Lease(LeaseRefusal::TooManyClients),
        BrokerError::Unavailable,
    ] {
        seen.push(format!("{error:?} {error} {:?}", Admission::Refused(error)));
    }
    let seen = seen.join("\n");
    assert!(
        seen.contains("Refused") && seen.contains("jsonrpc"),
        "{seen}"
    );
    for presented in &tickets {
        assert!(!seen.contains(presented.as_str()), "{presented} leaked");
    }
    assert!(!seen.contains("Qv7-Tkt_"), "a ticket's prefix leaked");
    // The store is given the hashes of the tickets presented, never a ticket.
    let hashes: BTreeSet<[u8; 32]> = tickets
        .iter()
        .flat_map(|t| [sha256(t), sha256(&format!("{t}x"))])
        .collect();
    let asked = grants.asked.lock().unwrap().clone();
    assert_ne!(asked.len(), 0);
    assert!(asked.iter().all(|hash| hashes.contains(hash)));
}

#[test]
fn a_handshake_beyond_the_bound_on_clients_is_refused_and_leaves_no_trace() {
    let world = World::new();
    let grants = Arc::new(Grants::default());
    let (first, second) = (ticket(1), ticket(2));
    grants.set(&first, Some(full()));
    grants.set(&second, Some(full()));
    let broker = world.ticket_broker(grants.clone(), 2);
    let a = broker.admit(first.into_bytes()).unwrap();
    let b = broker.connect(Some(full())).unwrap();
    let before = broker.retained().unwrap();
    let full_house = BrokerError::Lease(LeaseRefusal::TooManyClients);
    assert_eq!(
        broker.admit(second.clone().into_bytes()).err(),
        Some(full_house)
    );
    assert_eq!(broker.admit(Vec::new()).err(), Some(full_house));
    for which in [Loop::Asynchronous, Loop::Blocking] {
        let (admission, output) = shake(&broker, &handshake_then_observe(second.as_bytes()), which);
        assert_eq!(admission, Admission::Refused(full_house));
        assert_eq!(output, Vec::<u8>::new());
    }
    assert_eq!(broker.retained().unwrap(), before);
    assert_eq!(grants.asks(), 0);
    a.close();
    let c = broker
        .admit(second.into_bytes())
        .expect("the refusals spent nothing");
    assert_eq!(
        broker.retained().unwrap().clients,
        BTreeSet::from([b.client(), c.client()])
    );
}
