//! Shared test support: deterministic port doubles, a synthetic screen, MCP request helpers
//! and JSON Schema validation against the dossier's schema files.

#![allow(dead_code)] // each test crate uses a different subset

use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use ommatix_adapter_mcp::{Connection, MODERN_PROTOCOL_VERSION, McpConfig, McpServer};
use ommatix_application::contract::ValidatedAction;
use ommatix_application::{
    CapturedFrame, Clock, DeliveryReceipt, Engine, EngineConfig, FrameCapturer, LocalPolicy,
    ManualClock, OsActuator, PolicyStore, PortError, Ports, TextRecognizer, VisionDetector,
};
use ommatix_domain::bbox::BBox;
use ommatix_domain::fusion::{Actionability, Affordance, Control, Role, Word};
use ommatix_domain::incremental::BgraFrame;
use serde_json::{Value, json};

// ---- the synthetic screen ----

/// What the doubles "see": controls for the detector, words for the recognizer.
#[derive(Debug, Clone, Default)]
pub struct Scene {
    pub width: u32,
    pub height: u32,
    pub controls: Vec<Control>,
    pub words: Vec<Word>,
}

pub fn bx(x0: f64, y0: f64, x1: f64, y1: f64) -> BBox {
    BBox::new(x0, y0, x1, y1).expect("test boxes are valid")
}

pub fn control(bbox: BBox, role: Role, actionability: Actionability) -> Control {
    Control {
        bbox,
        role,
        affordance: Affordance {
            actionability,
            enabled: true,
            visible: true,
        },
    }
}

/// A small window: a confirmed "Save" button, a "Name" input with its external label, a
/// free "Welcome" text and an unknown, unlabelled control.
pub fn basic_scene() -> Scene {
    Scene {
        width: 320,
        height: 200,
        controls: vec![
            control(
                bx(10.0, 10.0, 90.0, 40.0),
                Role::Button,
                Actionability::Confirmed,
            ),
            control(
                bx(10.0, 70.0, 200.0, 100.0),
                Role::Input,
                Actionability::Confirmed,
            ),
            control(
                bx(250.0, 120.0, 300.0, 170.0),
                Role::Unknown,
                Actionability::Unknown,
            ),
        ],
        words: vec![
            Word::new("Save", bx(25.0, 15.0, 75.0, 35.0)),
            Word::new("Name", bx(10.0, 50.0, 60.0, 65.0)),
            Word::new("Welcome", bx(150.0, 10.0, 230.0, 30.0)),
        ],
    }
}

/// `n` free words in a grid, each `chars` characters long (text nodes only).
pub fn word_grid(n: usize, chars: usize) -> Scene {
    let cols = 10;
    let mut words = Vec::with_capacity(n);
    for i in 0..n {
        let (c, r) = ((i % cols) as f64, (i / cols) as f64);
        let letter = char::from(b'a' + (i % 26) as u8);
        let text: String = std::iter::repeat_n(letter, chars).collect();
        words.push(Word::new(
            text,
            bx(
                c * 100.0 + 5.0,
                r * 40.0 + 5.0,
                c * 100.0 + 95.0,
                r * 40.0 + 30.0,
            ),
        ));
    }
    let rows = n.div_ceil(cols) as u32;
    Scene {
        width: 1000,
        height: (rows * 40).max(40),
        controls: Vec::new(),
        words,
    }
}

fn intersects(a: &BBox, regions: &[BBox]) -> bool {
    regions.iter().any(|r| a.intersection_area(r) > 0.0)
}

struct Screen {
    frame: BgraFrame,
    seq: u64,
    clock: Arc<ManualClock>,
    cost: Duration,
}

impl FrameCapturer for Screen {
    fn name(&self) -> &'static str {
        "test-screen"
    }

    fn capture(&mut self) -> Result<CapturedFrame, PortError> {
        self.seq += 1;
        self.clock.advance(self.cost);
        Ok(CapturedFrame {
            frame: self.frame.clone(),
            frame_seq: self.seq,
            captured_at: self.clock.now(),
        })
    }
}

struct Detector(Arc<Scene>, Arc<ManualClock>, Duration);

impl VisionDetector for Detector {
    fn name(&self) -> &'static str {
        "test-detector"
    }

    fn models_loaded(&self) -> bool {
        false
    }

    fn detect(&mut self, _: &BgraFrame, regions: &[BBox]) -> Result<Vec<Control>, PortError> {
        self.1.advance(self.2);
        Ok(self
            .0
            .controls
            .iter()
            .filter(|c| intersects(&c.bbox, regions))
            .cloned()
            .collect())
    }
}

struct Recognizer(Arc<Scene>, Arc<ManualClock>, Duration);

impl TextRecognizer for Recognizer {
    fn name(&self) -> &'static str {
        "test-ocr"
    }

    fn recognize(&mut self, _: &BgraFrame, regions: &[BBox]) -> Result<Vec<Word>, PortError> {
        self.1.advance(self.2);
        Ok(self
            .0
            .words
            .iter()
            .filter(|w| intersects(&w.bbox, regions))
            .cloned()
            .collect())
    }
}

/// Never live; counts what it would have been asked to deliver.
#[derive(Default)]
struct NullActuator {
    delivered: Arc<Mutex<u32>>,
}

impl OsActuator for NullActuator {
    fn name(&self) -> &'static str {
        "test-actuator"
    }

    fn is_live(&self) -> bool {
        false
    }

    fn deliver(&mut self, _: &ValidatedAction) -> Result<DeliveryReceipt, PortError> {
        *self
            .delivered
            .lock()
            .unwrap_or_else(PoisonError::into_inner) += 1;
        Err(PortError::new("test-actuator", "not live"))
    }
}

struct FixedPolicy(LocalPolicy);

impl PolicyStore for FixedPolicy {
    fn current(&self) -> LocalPolicy {
        self.0.clone()
    }
}

pub const SESSION_ID: &str = "test-session";

/// How long each double takes on the engine's virtual clock. The stages then run one after
/// the other, so that their durations are exact.
#[derive(Debug, Clone, Copy, Default)]
pub struct Costs {
    pub capture: Duration,
    pub detect: Duration,
    pub recognize: Duration,
}

pub fn engine_with(scene: Scene, policy: LocalPolicy) -> Engine {
    build_engine(scene, Arc::new(FixedPolicy(policy)), None)
}

/// Like [`engine_with`], with the operator's policy store given: the tests that need the
/// operator's purge file (SEC-07) supply their own.
pub fn engine_with_store(scene: Scene, policy: Arc<dyn PolicyStore>) -> Engine {
    build_engine(scene, policy, None)
}

/// Like [`engine_with`], with doubles that take `costs` and stages that run in sequence.
pub fn engine_with_costs(scene: Scene, policy: LocalPolicy, costs: Costs) -> Engine {
    build_engine(scene, Arc::new(FixedPolicy(policy)), Some(costs))
}

fn build_engine(scene: Scene, policy: Arc<dyn PolicyStore>, costs: Option<Costs>) -> Engine {
    let clock = Arc::new(ManualClock::new());
    let costs_in_use = costs.unwrap_or_default();
    let pixels = vec![0x80_u8; scene.width as usize * scene.height as usize * 4];
    let frame = BgraFrame::new(
        scene.width,
        scene.height,
        scene.width as usize * 4,
        1,
        pixels,
    )
    .expect("valid synthetic frame");
    let scene = Arc::new(scene);
    let ports = Ports {
        capturer: Box::new(Screen {
            frame,
            seq: 0,
            clock: clock.clone(),
            cost: costs_in_use.capture,
        }),
        detector: Box::new(Detector(scene.clone(), clock.clone(), costs_in_use.detect)),
        recognizer: Box::new(Recognizer(scene, clock.clone(), costs_in_use.recognize)),
        accessibility: Box::new(ommatix_application::NoAccessibility),
        actuator: Box::new(NullActuator::default()),
        policy,
        clock,
    };
    let mut config = EngineConfig::new(SESSION_ID);
    config.concurrent_stages = costs.is_none();
    Engine::new(config, ports)
}

pub fn server_with(scene: Scene, config: McpConfig) -> McpServer {
    McpServer::new(engine_with(scene, LocalPolicy::default()), config)
}

pub fn server() -> McpServer {
    server_with(basic_scene(), McpConfig::default())
}

pub fn legacy_server() -> McpServer {
    server_with(basic_scene(), McpConfig::with_legacy())
}

// ---- messages ----

pub fn modern_meta() -> Value {
    json!({
        "io.modelcontextprotocol/protocolVersion": MODERN_PROTOCOL_VERSION,
        "io.modelcontextprotocol/clientCapabilities": {},
        "io.modelcontextprotocol/clientInfo": { "name": "ommatix-tests", "version": "1.0" }
    })
}

/// Sends one raw message; parses the response line, if any.
pub fn send_raw(conn: &mut Connection, raw: &str) -> Option<Value> {
    conn.handle_message(raw.as_bytes()).map(|line| {
        assert!(!line.contains('\n'), "one message per line");
        serde_json::from_str(&line).expect("responses are JSON")
    })
}

pub fn send(conn: &mut Connection, message: &Value) -> Option<Value> {
    send_raw(conn, &message.to_string())
}

/// A request with the given params (no metadata added).
#[allow(clippy::needless_pass_by_value)] // call sites build the params inline
pub fn request(conn: &mut Connection, id: u64, method: &str, params: Value) -> Value {
    send(
        conn,
        &json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }),
    )
    .expect("requests get a response")
}

/// A modern request: `params._meta` carries the per-request metadata.
pub fn modern(conn: &mut Connection, id: u64, method: &str, mut params: Value) -> Value {
    params["_meta"] = modern_meta();
    request(conn, id, method, params)
}

/// A modern `tools/call`; returns the whole response.
#[allow(clippy::needless_pass_by_value)] // call sites build the arguments inline
pub fn call(conn: &mut Connection, id: u64, tool: &str, arguments: Value) -> Value {
    modern(
        conn,
        id,
        "tools/call",
        json!({ "name": tool, "arguments": arguments }),
    )
}

/// A modern `tools/call` that must produce a tool result; checks the result against the
/// contract and returns it.
pub fn call_ok(conn: &mut Connection, id: u64, tool: &str, arguments: Value) -> Value {
    let response = call(conn, id, tool, arguments);
    let result = response
        .get("result")
        .unwrap_or_else(|| panic!("expected a tool result, got {response}"))
        .clone();
    check_tool_result(&result, tool);
    result
}

pub fn error_code(response: &Value) -> i64 {
    response["error"]["code"]
        .as_i64()
        .unwrap_or_else(|| panic!("expected a JSON-RPC error, got {response}"))
}

// ---- contract checks ----

pub fn dossier(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../docs/thesis")
        .join(path)
}

pub fn read_json(path: &str) -> Value {
    let text = std::fs::read_to_string(dossier(path)).expect("dossier file exists");
    serde_json::from_str(&text).expect("dossier file is JSON")
}

pub fn schema(name: &str) -> Value {
    read_json(&format!("contratos/esquemas/{name}.schema.json"))
}

/// Validation errors of `instance` against the dossier schema `name`.
pub fn violations(name: &str, instance: &Value) -> Vec<String> {
    let validator = jsonschema::validator_for(&schema(name)).expect("schemas compile");
    validator
        .iter_errors(instance)
        .map(|e| format!("{} at {}", e, e.instance_path()))
        .collect()
}

pub fn assert_valid(name: &str, instance: &Value) {
    let errors = violations(name, instance);
    assert!(
        errors.is_empty(),
        "{name}: {errors:?}\ninstance: {instance}"
    );
}

/// The checks every tool result must pass: `structuredContent` validates against the
/// tool's output schema (or `tool_error` for an `observe_screen` error), the single text
/// block is the same JSON, and nothing run-dependent is in `structuredContent`.
pub fn check_tool_result(result: &Value, tool: &str) {
    let structured = &result["structuredContent"];
    let is_error = result["isError"]
        .as_bool()
        .unwrap_or_else(|| panic!("isError is explicit: {result}"));
    let schema_name = match (tool, is_error) {
        ("observe_screen", true) => "tool_error.output".to_owned(),
        _ => format!("{tool}.output"),
    };
    assert_valid(&schema_name, structured);
    let content = result["content"].as_array().expect("content array");
    assert_eq!(content.len(), 1, "one text block: {result}");
    assert_eq!(content[0]["type"], "text");
    let text: Value =
        serde_json::from_str(content[0]["text"].as_str().expect("text")).expect("text is JSON");
    assert_eq!(&text, structured, "text block mirrors structuredContent");
    // Run-dependent values live in `_meta`, never in the Ommatix object.
    let serialized = structured.to_string();
    for run_dependent in ["\"timings\"", "_us\""] {
        assert!(!serialized.contains(run_dependent), "{serialized}");
    }
}
