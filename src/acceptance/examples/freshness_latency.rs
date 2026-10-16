//! ADR-0032 synthetic action-freshness measurement. No OS capturer or live actuator.
//! cargo run --release -p ommatix-acceptance --example freshness_latency -- screen
//! cargo run --release -p ommatix-acceptance --example freshness_latency -- target_window
//! The wrapper reaches the freshness gate but delegates to the simulator's zero-event
//! receipt, so every attempt is indeterminate with no delivered input. This measures
//! freshness on an unchanged 640x400 simulated desktop, not live Windows performance.

#![allow(clippy::print_stdout)] // This measurement's JSON report is its explicit output.

use ommatix_adapter_sim::SimulatedDesktop;
use ommatix_application::contract::{
    ActionKind, ActionRequest, ExpectedEffect, ObserveRequest, ValidatedAction,
};
use ommatix_application::{
    Clock, DeliveryReceipt, Engine, EngineConfig, LocalPolicy, NoAccessibility, OsActuator,
    PolicyStore, PortError, Ports,
};
use serde_json::json;
use std::sync::Arc;
use std::time::{Duration, Instant};

struct Monotonic(Instant);
impl Clock for Monotonic {
    fn now(&self) -> Duration {
        self.0.elapsed()
    }
    fn sleep(&self, _: Duration) {}
}
struct Policy;
impl PolicyStore for Policy {
    fn current(&self) -> LocalPolicy {
        LocalPolicy {
            actuation_enabled: true,
            allowed_applications: vec!["synthetic-benchmark".into()],
            ..LocalPolicy::default()
        }
    }
}
struct SyntheticActuator(SimulatedDesktop);
impl OsActuator for SyntheticActuator {
    fn name(&self) -> &'static str {
        "synthetic-freshness-probe"
    }
    fn is_live(&self) -> bool {
        true
    }
    fn foreground_application(&self) -> Option<String> {
        Some("synthetic-benchmark".into())
    }
    fn foreground_window(&self) -> Option<ommatix_application::TopLevelWindow> {
        self.0.actuator().foreground_window()
    }
    fn top_level_windows(&self) -> Option<Vec<ommatix_application::TopLevelWindow>> {
        self.0.actuator().top_level_windows()
    }
    fn window_at(&self, point: (f64, f64)) -> Option<ommatix_application::TopLevelWindow> {
        self.0.actuator().window_at(point)
    }
    fn deliver(&mut self, action: &ValidatedAction) -> Result<DeliveryReceipt, PortError> {
        self.0.actuator().deliver(action)
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if cfg!(debug_assertions) {
        return Err("run this measurement with --release".into());
    }
    let scope = std::env::args().nth(1).unwrap_or_else(|| "screen".into());
    let clock = Arc::new(Monotonic(Instant::now()));
    let desktop = SimulatedDesktop::new().with_clock(clock.clone());
    let mut config = EngineConfig::new("synthetic-freshness");
    config.action_freshness = match scope.as_str() {
        "screen" => ommatix_application::ActionFreshness::Screen,
        "target_window" => ommatix_application::ActionFreshness::TargetWindow,
        _ => return Err("scope must be screen or target_window".into()),
    };
    config.concurrent_stages = false;
    let mut engine = Engine::new(
        config,
        Ports {
            capturer: Box::new(desktop.capturer()),
            detector: Box::new(desktop.detector()),
            recognizer: Box::new(desktop.recognizer()),
            accessibility: Box::new(NoAccessibility),
            actuator: Box::new(SyntheticActuator(desktop)),
            policy: Arc::new(Policy),
            clock,
        },
    );
    let mut samples = Vec::new();
    for n in 0..220 {
        let page = engine.observe_screen(&ObserveRequest::default())?;
        let target = page
            .elements
            .iter()
            .find(|e| e.label == "Save")
            .ok_or("missing synthetic button")?;
        let request = ActionRequest {
            request_id: format!("sample-{n}"),
            session_id: page.session_id,
            snapshot_id: page.snapshot_id,
            geometry_epoch: page.geometry_epoch,
            target_id: target.target_id.to_string(),
            expected_revision: target.revision,
            action: ActionKind::Click,
            dry_run: false,
            expected_effect: ExpectedEffect::None,
        };
        let result = engine.execute_action(&request);
        if result.input_delivered()
            || result.status != ommatix_domain::identity::ActionStatus::Indeterminate
        {
            return Err(
                "the synthetic zero-event actuator must report indeterminate and no delivery"
                    .into(),
            );
        }
        if n >= 20 {
            samples.push(engine.last_action_timings().ok_or("missing timings")?);
        }
    }
    let percentile = |stage: fn(&ommatix_application::contract::ActionTimings) -> Duration| {
        let mut values: Vec<u128> = samples.iter().map(|s| stage(s).as_micros()).collect();
        values.sort_unstable();
        json!({"p50":values[99],"p95":values[189]})
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "scope":scope,"desktop":"SimulatedDesktop 640x400 unchanged","warmup":20,"samples":200,
            "machine":std::env::var("COMPUTERNAME").unwrap_or_default(),
            "cpu":std::env::var("PROCESSOR_IDENTIFIER").unwrap_or_default(),
            "os_input_delivered":false,"timings_us":{
                "gate":percentile(|t|t.gate),"freshness":percentile(|t|t.freshness),
                "delivery":percentile(|t|t.delivery),"focus":percentile(|t|t.focus),
                "verification":percentile(|t|t.verification),"total":percentile(|t|t.total)
            }
        }))?
    );
    Ok(())
}
