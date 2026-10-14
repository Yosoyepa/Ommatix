//! Driven ports of the SDD (thesis chapter 02 §2.4): what the application needs from the
//! outside world.
//!
//! Driving adapters (the MCP server) do not need a port trait: they call the
//! [`Engine`](crate::Engine) directly.

use std::time::Duration;

use ommatix_domain::bbox::BBox;
use ommatix_domain::fusion::{Control, Word};
use ommatix_domain::incremental::BgraFrame;

use crate::PortError;
use crate::contract::ValidatedAction;

/// Monotonic time and waiting. Injected so that stage timings and settle waits are
/// reproducible under test (see [`ManualClock`](crate::ManualClock)).
pub trait Clock: Send + Sync {
    /// Time elapsed since an arbitrary, fixed origin. Never goes backwards.
    fn now(&self) -> Duration;

    /// Waits for `duration` (a virtual clock simply advances).
    fn sleep(&self, duration: Duration);
}

/// One capture with the sequence number and monotonic instant the capturer assigned.
#[derive(Debug, Clone)]
pub struct CapturedFrame {
    pub frame: BgraFrame,
    /// Grows by one per capture; never reset within a session.
    pub frame_seq: u64,
    /// Monotonic capture instant, on the same origin as [`Clock::now`].
    pub captured_at: Duration,
}

/// Captures the selected monitor (DXGI in S3; synthetic fixtures until then).
pub trait FrameCapturer: Send {
    fn name(&self) -> &'static str;

    fn capture(&mut self) -> Result<CapturedFrame, PortError>;
}

/// Detects controls inside `regions` of `frame`. Deterministic: same pixels and regions,
/// same controls. A detector that cannot tell roles apart reports `Role::Unknown`, and one
/// without affordance evidence reports `Actionability::Unknown`.
pub trait VisionDetector: Send {
    fn name(&self) -> &'static str;

    /// Whether real model weights are loaded (`get_engine_metrics.models_loaded`).
    fn models_loaded(&self) -> bool;

    fn detect(&mut self, frame: &BgraFrame, regions: &[BBox]) -> Result<Vec<Control>, PortError>;
}

/// Recognises words inside `regions` of `frame` (OCR). Deterministic like the detector.
pub trait TextRecognizer: Send {
    fn name(&self) -> &'static str;

    fn recognize(&mut self, frame: &BgraFrame, regions: &[BBox]) -> Result<Vec<Word>, PortError>;
}

/// What the actuator reports after injecting the events of one action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeliveryReceipt {
    /// Events handed to the OS.
    pub submitted: u32,
    /// Events the OS reported as inserted.
    pub inserted: u32,
}

/// Injects validated input into the OS (`SendInput` in S3). Implementations that do not
/// touch the real OS report `is_live() == false` and are never handed a live action.
pub trait OsActuator: Send {
    fn name(&self) -> &'static str;

    fn is_live(&self) -> bool;

    fn deliver(&mut self, action: &ValidatedAction) -> Result<DeliveryReceipt, PortError>;
}

/// The local policy, owned by the operator and outside the agent's reach (chapter 08).
/// Its default denies everything: no actuation, no values, no applications.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LocalPolicy {
    pub actuation_enabled: bool,
    /// Whether `observe_screen` may return field contents when the agent asks for them.
    pub include_values: bool,
    pub allowed_applications: Vec<String>,
    /// Operator stop: no new input events while set.
    pub stopped: bool,
}

/// Reads the local policy. The agent has no port to write it.
pub trait PolicyStore: Send + Sync {
    fn current(&self) -> LocalPolicy;
}
