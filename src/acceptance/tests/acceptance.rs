//! Step definitions for `features/*.feature`.
//!
//! Every step talks to Ommatix exactly like an agent would: JSON-RPC messages through the
//! MCP adapter. The simulator state is only read to check ground truth on the "desktop".

// cucumber passes captured step parameters by value.
#![allow(clippy::needless_pass_by_value)]

use std::sync::{Arc, Mutex};

use cucumber::gherkin::Step;
use cucumber::{World, given, then, when};
use ommatix_adapter_capture::ScriptedSource;
use ommatix_adapter_input::{ActionLog, DryRunInput};
use ommatix_adapter_mcp::McpServer;
use ommatix_adapter_sim::{SimState, SimulatedDesktop, Theme, layout, scripted_session};
use ommatix_adapter_vision::FlatRegionDetector;
use ommatix_application::{
    ActuationPolicy, ActuationService, AgentSession, FrameSource, InputController, ManualClock,
    Observer, PerceptionConfig, PerceptionEngine, PortError, SettlePolicy,
};
use ommatix_domain::{Frame, Point, Rect};
use serde_json::{Value, json};

#[derive(Debug, Default, World)]
struct Agent {
    server: Option<McpServer>,
    sim: Option<SimulatedDesktop>,
    dry_run_log: Option<ActionLog>,
    next_id: u64,
    /// Full JSON-RPC response of the last request.
    last_response: Value,
    /// Perception report of the last `perceive` (or of the observation after an action).
    last_report: Option<Value>,
    /// Elements returned by the last spatial query.
    last_answer: Vec<Value>,
    remembered_id: Option<Value>,
    /// Frames the simulator's capture still delivers before it breaks (`None`: never).
    capture_budget: Arc<Mutex<Option<usize>>>,
}

/// The simulator's screen, with a capture that can be made to break.
struct Breakable {
    inner: Box<dyn FrameSource>,
    frames_left: Arc<Mutex<Option<usize>>>,
}

impl FrameSource for Breakable {
    fn name(&self) -> &'static str {
        self.inner.name()
    }

    fn next_frame(&mut self) -> Result<Option<Frame>, PortError> {
        let mut left = self.frames_left.lock().expect("budget lock");
        match left.as_mut() {
            Some(0) => Err(PortError::new(self.inner.name(), "capture lost")),
            Some(n) => {
                *n -= 1;
                self.inner.next_frame()
            }
            None => self.inner.next_frame(),
        }
    }
}

impl Agent {
    /// Wires Ommatix exactly like the binary does, but with a virtual clock so that
    /// waiting for the screen to settle is instant and reproducible.
    fn connect(&mut self, source: Box<dyn FrameSource>, input: Box<dyn InputController>) {
        let clock = Arc::new(ManualClock::new());
        let perception = PerceptionEngine::new(
            source,
            Box::new(FlatRegionDetector::default()),
            clock.clone(),
            PerceptionConfig::default(),
        )
        .expect("valid default config");
        let observer = Observer::new(clock, SettlePolicy::default()).expect("valid policy");
        let actuation = ActuationService::new(input, ActuationPolicy::default());
        self.server = Some(McpServer::new(AgentSession::new(
            perception, observer, actuation,
        )));
        self.last_report = None;
    }

    fn start_simulator(&mut self, theme: Theme) {
        let sim = SimulatedDesktop::with_theme(theme);
        let screen = Breakable {
            inner: Box::new(sim.screen()),
            frames_left: self.capture_budget.clone(),
        };
        self.connect(Box::new(screen), Box::new(sim.input()));
        self.sim = Some(sim);
    }

    fn send_raw(&mut self, line: &str) -> Value {
        let server = self.server.as_mut().expect("a desktop must be running");
        let response = server
            .handle_message(line.as_bytes())
            .unwrap_or(Value::Null);
        self.last_response = response.clone();
        response
    }

    fn request(&mut self, method: &str, params: &Value) -> Value {
        self.next_id += 1;
        let msg =
            json!({ "jsonrpc": "2.0", "id": self.next_id, "method": method, "params": params });
        self.send_raw(&msg.to_string())
    }

    /// Calls a tool; returns `structuredContent` when it succeeded.
    fn call(&mut self, tool: &str, args: &Value) -> Option<Value> {
        let response = self.request("tools/call", &json!({ "name": tool, "arguments": args }));
        let result = &response["result"];
        if result["isError"] == true {
            return None;
        }
        let structured = result["structuredContent"].clone();
        match tool {
            "perceive" => self.last_report = Some(structured.clone()),
            "click" | "type_text" | "press_key" => {
                self.last_report = Some(structured["observation"].clone());
            }
            _ => {}
        }
        Some(structured)
    }

    fn must_call(&mut self, tool: &str, args: &Value) -> Value {
        self.call(tool, args)
            .unwrap_or_else(|| panic!("{tool} failed: {}", self.last_response))
    }

    fn report(&self) -> &Value {
        self.last_report.as_ref().expect("nothing perceived yet")
    }

    fn elements_at(&mut self, p: Point) -> Vec<Value> {
        let answer = self.must_call("element_at", &json!({ "x": p.x, "y": p.y }));
        answer["elements"].as_array().cloned().unwrap_or_default()
    }

    /// The element whose bounds exactly match `rect`, as the agent would find it.
    fn find(&mut self, rect: Rect) -> Option<Value> {
        let want = rect_json(rect);
        self.elements_at(rect.center())
            .into_iter()
            .find(|e| e["bounds"] == want)
    }

    fn sim_state(&self) -> SimState {
        self.sim.as_ref().expect("simulator running").state()
    }
}

fn rect_json(r: Rect) -> Value {
    json!({ "x": r.x, "y": r.y, "w": r.w, "h": r.h })
}

fn button(name: &str) -> Rect {
    match name {
        "Save" => layout::SAVE_BUTTON,
        "Cancel" => layout::CANCEL_BUTTON,
        "OK" => layout::OK_BUTTON,
        other => panic!("unknown button {other}"),
    }
}

/// Named parts of the desktop that are not buttons.
fn part(name: &str) -> Rect {
    match name {
        "text field" => layout::TEXT_FIELD,
        "window" => layout::CLIENT_AREA,
        "title bar" => layout::TITLE_BAR,
        "dialog" => layout::DIALOG,
        other => panic!("unknown part {other}"),
    }
}

fn theme(name: &str) -> Theme {
    match name {
        "light" => Theme::Light,
        "dark" => Theme::Dark,
        "contrasting wallpaper" => Theme::ContrastingWallpaper,
        other => panic!("unknown theme {other}"),
    }
}

fn count(v: &Value) -> usize {
    v.as_array().map_or(0, Vec::len)
}

// ---------------------------------------------------------------- Given

#[given("the simulated desktop is running")]
fn simulated_desktop(w: &mut Agent) {
    w.start_simulator(Theme::Light);
}

#[given(regex = r"^the simulated desktop is running with the (.+) theme$")]
fn themed_desktop(w: &mut Agent, name: String) {
    w.start_simulator(theme(&name));
}

#[given("a fresh simulated desktop that has not been perceived")]
fn fresh_desktop(w: &mut Agent) {
    w.start_simulator(Theme::Light);
}

#[given("the recorded demo session with dry-run input")]
fn demo_session(w: &mut Agent) {
    let log = ActionLog::default();
    w.connect(
        Box::new(ScriptedSource::new(scripted_session(Theme::Light))),
        Box::new(DryRunInput::new(log.clone())),
    );
    w.sim = None;
    w.dry_run_log = Some(log);
}

#[given(regex = r"^the screen capture will fail after (\d+) more frames?$")]
fn capture_breaks(w: &mut Agent, frames: usize) {
    *w.capture_budget.lock().expect("budget lock") = Some(frames);
}

#[given("the recorded session has been played to the end")]
fn play_to_the_end(w: &mut Agent) {
    while w.call("perceive", &json!({})).is_some() {}
}

#[given(expr = "the agent remembers the id of the {string} button")]
fn remember_id(w: &mut Agent, name: String) {
    let element = w.find(button(&name)).expect("button visible");
    w.remembered_id = Some(element["id"].clone());
}

#[given(regex = r"^the agent remembers the id of the (window|text field|title bar)$")]
fn remember_part_id(w: &mut Agent, name: String) {
    let element = w.find(part(&name)).expect("part visible");
    w.remembered_id = Some(element["id"].clone());
}

// ---------------------------------------------------------------- When (agent)

#[given("the agent perceives the screen")]
#[when("the agent perceives the screen")]
fn perceive(w: &mut Agent) {
    w.must_call("perceive", &json!({}));
}

#[given("the agent clicks the text field")]
#[when("the agent clicks the text field")]
fn click_field(w: &mut Agent) {
    let field = w.find(layout::TEXT_FIELD).expect("text field visible");
    w.must_call("click", &json!({ "element_id": field["id"] }));
}

#[given(expr = "the agent clicks the {string} button")]
#[when(expr = "the agent clicks the {string} button")]
fn click_button(w: &mut Agent, name: String) {
    let element = w
        .find(button(&name))
        .unwrap_or_else(|| panic!("the agent cannot see the {name} button"));
    w.must_call("click", &json!({ "element_id": element["id"] }));
}

#[when(expr = "the agent clicks at {int}, {int}")]
fn click_at(w: &mut Agent, x: u32, y: u32) {
    w.call("click", &json!({ "x": x, "y": y }));
}

#[when(expr = "the agent clicks element {string}")]
fn click_element(w: &mut Agent, id: String) {
    w.call("click", &json!({ "element_id": id }));
}

#[when(expr = "the agent types {string}")]
fn type_text(w: &mut Agent, text: String) {
    w.must_call("type_text", &json!({ "text": text }));
}

#[when(regex = r"^the agent types text containing U\+([0-9A-F]{4})$")]
fn type_special(w: &mut Agent, code: String) {
    let c = u32::from_str_radix(&code, 16)
        .ok()
        .and_then(char::from_u32)
        .expect("a Unicode scalar value");
    w.call("type_text", &json!({ "text": format!("a{c}b") }));
}

#[when(expr = "the agent presses {word}")]
fn press(w: &mut Agent, key: String) {
    w.must_call("press_key", &json!({ "key": key }));
}

#[when(expr = "the agent asks what is under the {string} button")]
fn ask_button(w: &mut Agent, name: String) {
    w.last_answer = w.elements_at(button(&name).center());
}

#[when("the agent asks what is under the first typed character")]
fn ask_first_char(w: &mut Agent) {
    let p = Point::new(layout::TEXT_ORIGIN_X + 1, layout::TEXT_ORIGIN_Y + 1);
    w.last_answer = w.elements_at(p);
}

#[when(regex = r"^the agent calls (\S+) with (.+)$")]
fn call_with(w: &mut Agent, tool: String, args: String) {
    let tool = tool.trim_matches('"').to_owned();
    let args: Value = serde_json::from_str(&args).expect("valid JSON arguments");
    w.call(&tool, &args);
}

// ---------------------------------------------------------------- When (protocol)

#[when(expr = "the client initializes with protocol version {string}")]
fn initialize(w: &mut Agent, version: String) {
    w.request("initialize", &json!({ "protocolVersion": version }));
}

#[when("the client lists the tools")]
fn list_tools(w: &mut Agent) {
    w.request("tools/list", &json!({}));
}

#[when(regex = r"^the client sends (.+)$")]
fn send_raw(w: &mut Agent, line: String) {
    w.send_raw(&line);
}

// ---------------------------------------------------------------- Then (perception)

#[then("every grid cell is re-analysed")]
fn all_dirty(w: &mut Agent) {
    let r = w.report();
    assert_eq!(r["dirty_cells"], r["total_cells"]);
}

#[then("no grid cell is re-analysed")]
fn none_dirty(w: &mut Agent) {
    assert_eq!(w.report()["dirty_cells"], 0);
}

#[then(expr = "fewer than {int} grid cells are re-analysed")]
fn few_dirty(w: &mut Agent, max: u64) {
    let dirty = w.report()["dirty_cells"].as_u64().unwrap();
    assert!(dirty < max, "{dirty} cells re-analysed");
}

#[then(regex = r"^(\d+) elements? (?:is|are) added$")]
fn added(w: &mut Agent, n: usize) {
    assert_eq!(count(&w.report()["added"]), n, "{}", w.report());
}

#[then("the scene is unchanged")]
fn unchanged(w: &mut Agent) {
    let r = w.report();
    assert_eq!(
        count(&r["added"]) + count(&r["updated"]) + count(&r["removed"]),
        0
    );
}

#[then(expr = "the answer is the {string} button with its exact bounds")]
fn answer_is_button(w: &mut Agent, name: String) {
    let first = w.last_answer.first().expect("something under the point");
    assert_eq!(first["bounds"], rect_json(button(&name)));
    assert_eq!(
        first["center"],
        json!({ "x": button(&name).center().x, "y": button(&name).center().y })
    );
}

#[then("the first answer is smaller than the text field")]
fn smallest_first(w: &mut Agent) {
    let area = |e: &Value| e["bounds"]["w"].as_u64().unwrap() * e["bounds"]["h"].as_u64().unwrap();
    let field = rect_json(layout::TEXT_FIELD);
    assert!(w.last_answer.len() >= 2, "{:?}", w.last_answer);
    assert!(area(&w.last_answer[0]) < layout::TEXT_FIELD.area());
    assert!(w.last_answer.iter().any(|e| e["bounds"] == field));
}

#[then(expr = "the {string} button still has the remembered id")]
fn same_id(w: &mut Agent, name: String) {
    let element = w.find(button(&name)).expect("button visible");
    assert_eq!(Some(element["id"].clone()), w.remembered_id);
}

#[then(regex = r"^the (window|text field|title bar) still has the remembered id$")]
fn part_same_id(w: &mut Agent, name: String) {
    let element = w.find(part(&name)).expect("part visible");
    assert_eq!(Some(element["id"].clone()), w.remembered_id);
}

#[then(regex = r"^the (window|text field|title bar) was not re-analysed$")]
fn part_untouched(w: &mut Agent, name: String) {
    let id = w.find(part(&name)).expect("part visible")["id"].clone();
    let r = w.report();
    let touched = ["added", "updated"]
        .iter()
        .flat_map(|k| r[*k].as_array().cloned().unwrap_or_default())
        .any(|e| e["id"] == id);
    assert!(!touched, "{name} was re-analysed: {r}");
}

#[then(expr = "the {string} button is reported at its exact bounds")]
fn reported(w: &mut Agent, name: String) {
    assert!(w.find(button(&name)).is_some(), "{name} not perceived");
}

#[then(regex = r"^the (window|text field|title bar|dialog) is reported at its exact bounds$")]
fn part_reported(w: &mut Agent, name: String) {
    assert!(w.find(part(&name)).is_some(), "{name} not perceived");
}

#[then(expr = "the typed text {string} is reported as one element")]
fn text_is_one_element(w: &mut Agent, text: String) {
    let want = rect_json(layout::text_bounds(text.chars().count() as u32));
    let p = Point::new(layout::TEXT_ORIGIN_X + 1, layout::TEXT_ORIGIN_Y + 1);
    let first = w.elements_at(p).into_iter().next().expect("text perceived");
    assert_eq!(first["bounds"], want);
}

#[then(expr = "at most {int}% of the screen is re-analysed")]
fn analysed_share(w: &mut Agent, percent: u64) {
    let analysed = w.report()["analysed_px"].as_u64().expect("analysed_px");
    let screen = layout::SCREEN.area();
    assert!(
        analysed * 100 <= screen * percent,
        "{analysed} of {screen} px re-analysed"
    );
}

#[then(expr = "the {string} button is no longer reported")]
fn not_reported(w: &mut Agent, name: String) {
    assert!(w.find(button(&name)).is_none(), "{name} still perceived");
}

// ---------------------------------------------------------------- Then (desktop)

#[then("the dialog is open on the desktop")]
fn dialog_open(w: &mut Agent) {
    assert!(w.sim_state().dialog_open);
}

#[then("the dialog is closed on the desktop")]
fn dialog_closed(w: &mut Agent) {
    assert!(!w.sim_state().dialog_open);
}

#[then("nothing was saved")]
fn nothing_saved(w: &mut Agent) {
    assert_eq!(w.sim_state().saves, 0);
}

#[then(expr = "the text field contains {string}")]
fn field_contains(w: &mut Agent, text: String) {
    assert_eq!(w.sim_state().text, text);
}

#[then("the desktop received no input")]
fn no_input(w: &mut Agent) {
    assert_eq!(w.sim_state(), SimState::default());
}

#[then(expr = "the call fails with {string}")]
fn call_fails(w: &mut Agent, fragment: String) {
    let result = &w.last_response["result"];
    assert_eq!(result["isError"], true, "{}", w.last_response);
    let text = result["content"][0]["text"].as_str().unwrap_or_default();
    assert!(text.contains(&fragment), "{text:?} lacks {fragment:?}");
}

#[then("the action is reported as performed with an observation error")]
fn performed_but_unobserved(w: &mut Agent) {
    let result = &w.last_response["result"];
    assert_eq!(result["isError"], false, "{}", w.last_response);
    let structured = &result["structuredContent"];
    assert!(structured["performed"].is_object(), "{structured}");
    assert!(structured["observe_error"].is_string(), "{structured}");
}

#[then("there is no response")]
fn no_response(w: &mut Agent) {
    assert!(w.last_response.is_null(), "{}", w.last_response);
}

#[then(expr = "the response is a batch of {int} response(s)")]
fn batch_size(w: &mut Agent, n: usize) {
    assert_eq!(count(&w.last_response), n, "{}", w.last_response);
}

#[then("the action is reported as not live")]
fn not_live(w: &mut Agent) {
    let structured = &w.last_response["result"]["structuredContent"];
    assert_eq!(
        structured["performed"]["live"], false,
        "{}",
        w.last_response
    );
}

#[then(expr = "the dry-run log contains {int} action(s)")]
fn dry_run_log(w: &mut Agent, n: usize) {
    let log = w.dry_run_log.as_ref().expect("dry-run session");
    assert_eq!(log.snapshot().len(), n);
}

// ---------------------------------------------------------------- Then (protocol)

#[then(expr = "the server answers with protocol version {string}")]
fn protocol_version(w: &mut Agent, version: String) {
    assert_eq!(w.last_response["result"]["protocolVersion"], version);
}

#[then(expr = "the server identifies itself as {string}")]
fn server_name(w: &mut Agent, name: String) {
    assert_eq!(w.last_response["result"]["serverInfo"]["name"], name);
}

#[then("the tools are:")]
fn tools_are(w: &mut Agent, step: &Step) {
    let expected: Vec<String> = step
        .table
        .as_ref()
        .expect("a table of tool names")
        .rows
        .iter()
        .map(|row| row[0].clone())
        .collect();
    let actual: Vec<String> = w.last_response["result"]["tools"]
        .as_array()
        .expect("tools array")
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(actual, expected);
}

#[then(regex = r"^the response is a JSON-RPC error (-?\d+)$")]
fn rpc_error(w: &mut Agent, code: i64) {
    assert_eq!(
        w.last_response["error"]["code"], code,
        "{}",
        w.last_response
    );
}

fn main() {
    let features = concat!(env!("CARGO_MANIFEST_DIR"), "/features");
    futures::executor::block_on(Agent::cucumber().fail_on_skipped().run_and_exit(features));
}
