//! The broker (ADR-0039, ADR-0040): several MCP connections on one engine under one input lease
//! and each client's grant. Every behaviour that is on the wire is driven through real MCP
//! messages on the broker's connections, on the simulated desktop, with a manual clock and fixed
//! seeds. Nothing injects real input and nothing captures a screen.

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use common::{call, call_ok, error_code, modern_meta};
use ommatix_adapter_mcp::{
    Broker, BrokerConnection, BrokerError, Connection, DEFAULT_MAX_MESSAGE_BYTES, McpConfig,
    McpServer,
};
use ommatix_adapter_sim::fixture::{FixtureActuator, FixtureCapturer};
use ommatix_adapter_sim::{FixtureDesktop, FixtureScene, confirmed};
use ommatix_application::contract::ValidatedAction;
use ommatix_application::lease::{Acquired, ClientId, LeaseChange, LeaseRefusal};
use ommatix_application::{
    CapturedFrame, Clock, DeliveryReceipt, Engine, EngineConfig, FrameCapturer, Grant, LeaseTable,
    LocalPolicy, ManualClock, NoAccessibility, OsActuator, PolicyStore, PortError, Ports,
    TopLevelWindow, WindowOpacity,
};
use ommatix_domain::bbox::BBox;
use ommatix_domain::fusion::Role;
use serde_json::{Value, json};

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
        let clock = Arc::new(ManualClock::new());
        Self {
            desktop: FixtureDesktop::new(scene(), clock.clone()),
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
fn after_one_clients_delivery_another_clients_action_on_that_snapshot_is_stale() {
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

// ---- 2. a non-holder is served the published page ----

#[test]
fn a_non_holders_observation_is_the_published_page_and_moves_nothing() {
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
    assert_eq!(keys(&served), keys(&holder), "no new _meta key");
    assert_eq!(rig.world.captures(), captures, "nothing is captured");
    assert_eq!(metrics(&mut b)["observations"], observations);

    let page = holder["structuredContent"].clone();
    assert!(delivered(&act(&mut a, click(&page, "a1", true))));
    assert_eq!(rig.world.events(), 1);
}

// ---- 3. no lease, no input, no screen text ----

#[test]
fn a_live_action_without_the_lease_delivers_nothing_and_quotes_no_screen_text() {
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
fn when_the_term_ends_and_another_client_acquires_the_former_holder_is_refused() {
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
fn the_holder_disconnecting_ends_the_lease() {
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
fn the_operators_stop_ends_the_lease_and_refuses_every_delivery_whoever_holds_it() {
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
fn an_operator_purge_honoured_at_the_next_request_ends_the_lease() {
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
fn a_client_without_a_grant_observes_without_values_and_never_delivers() {
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
fn a_grant_that_leaves_the_application_out_is_refused_at_the_gate() {
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
fn two_clients_with_different_read_values_get_their_own_view_of_one_snapshot() {
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
fn a_non_holders_dry_run_is_judged_by_the_gate_on_the_current_page_only() {
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
fn two_clients_with_one_request_id_get_their_own_results_and_each_retry_replays_its_own() {
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

// ---- 9. the bound, and leaving frees everything ----

#[test]
fn a_ninth_client_is_refused_and_a_client_that_left_leaves_nothing_behind() {
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
fn only_the_last_client_to_leave_purges_the_engine() {
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
fn one_client_whose_grant_equals_the_policy_gets_the_stdio_servers_responses_byte_for_byte() {
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
fn oversized_and_malformed_messages_keep_the_stdio_servers_bounds_and_codes() {
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
fn a_call_that_panics_under_the_lock_leaves_the_broker_closed_to_every_request() {
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
    Stopped,
    Resumed,
    Purged,
    TimeMoved,
    LeaseSpent,
    Metrics,
}

const EVERY_OUTCOME: [Seen; 29] = [
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
}

/// Every action response must be free of screen text.
fn assert_no_screen_text(response: &Value) {
    let text = response.to_string();
    for word in SCREEN_TEXT {
        assert!(!text.contains(word), "{word} in {text}");
    }
}

#[test]
#[allow(clippy::too_many_lines)] // one generated run: each kind of step beside its model
fn generated_interleavings_deliver_once_per_snapshot_only_from_the_holder_under_its_grant() {
    const STEPS: usize = 5_000;
    const SLOTS: usize = 5;
    const MAX_CLIENTS: usize = 4;
    let world = World::new();
    let broker = world.broker(McpConfig::default(), MAX_CLIENTS);
    let mut model = Model::new(MAX_CLIENTS);
    let mut rng = Rng(0x0B40_4E55);
    let mut slots: Vec<Option<Slot>> = (0..SLOTS).map(|_| None).collect();
    let mut next_request = 0_u64;
    // The witness: every delivery, in the order the broker's lock was taken (one thread).
    let mut witness: Vec<(usize, ClientId, String)> = Vec::new();
    let mut per_snapshot: BTreeMap<String, usize> = BTreeMap::new();

    for step in 0..STEPS {
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
                match (expected.result, broker.connect(kind.grant())) {
                    (Ok(id), Ok(conn)) => {
                        assert_eq!(conn.client(), id);
                        model.connected.insert(id);
                        slots[index] = Some(Slot {
                            conn,
                            kind,
                            page: None,
                            last: None,
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
            (10..=19, true) => {
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
            (20..=25, true) => {
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
                model.saw(if actual.is_ok() {
                    Seen::Released
                } else {
                    Seen::ReleaseRefused
                });
            }
            (26..=35, true) => {
                let millis = 100 + rng.below(1_400);
                world.advance(Duration::from_millis(u64::try_from(millis).unwrap()));
                model.saw(Seen::TimeMoved);
            }
            (36..=52, true) => {
                let slot = slots[index].as_mut().unwrap();
                let id = slot.conn.client();
                let values = rng.chance(50);
                let publishes = model.tool(id, now, true);
                let result = observe_with(&mut slot.conn, json!({ "include_values": values }));
                let page = result["structuredContent"].clone();
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
                let response = act(&mut slot.conn, arguments.clone());
                assert_no_screen_text(&response);
                slot.last = Some(arguments.clone());
                let reaches_engine = !live || holds;
                let journal = model.journals.entry(id).or_default();
                let replay = journal.get(&request_id).cloned().filter(|_| reaches_engine);
                let outcome = if let Some(stored) = replay {
                    assert_eq!(
                        response, stored,
                        "step {step}: a retry replays its own result"
                    );
                    Seen::Replayed
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
                if delivered(&response) && outcome != Seen::Replayed {
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
                let _ = metrics(&mut slot.conn);
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
fn concurrent_connections_never_deliver_twice_on_one_snapshot_nor_outside_their_grant() {
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

    let logs: Vec<(Kind, Vec<Value>)> = std::thread::scope(|scope| {
        let handles: Vec<_> = connections
            .into_iter()
            .zip(kinds)
            .enumerate()
            .map(|(thread, (mut conn, kind))| {
                let clock = clock.clone();
                scope.spawn(move || {
                    let mut rng = Rng(0xC0FF_EE00 + u64::try_from(thread).unwrap());
                    let mut log = Vec::new();
                    let mut page: Option<Value> = None;
                    for step in 0..STEPS {
                        match rng.below(10) {
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
                            _ => {
                                let Some(page) = &page else { continue };
                                let live = rng.chance(80);
                                let id = format!("t{thread}-{step}");
                                let response = act(&mut conn, click(page, &id, live));
                                assert_no_screen_text(&response);
                                log.push(response);
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
        for response in log.iter().filter(|r| delivered(r)) {
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
        .filter(|r| refused_without_lease(r))
        .count();
    assert!(refusals > 0, "the lease refused something");
    let retained = broker.retained().unwrap();
    assert!(retained.clients.is_empty());
    assert!(
        !retained.grant_installed,
        "every grant was installed and removed inside one critical section"
    );
}
