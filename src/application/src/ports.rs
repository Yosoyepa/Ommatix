//! Driven ports of the SDD (thesis chapter 02 §2.4): what the application needs from the
//! outside world.
//!
//! Driving adapters (the MCP server) do not need a port trait: they call the
//! [`Engine`](crate::Engine) directly.

use std::time::Duration;

use ommatix_domain::bbox::BBox;
use ommatix_domain::fusion::{Control, Role, Word};
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

/// The control patterns an accessibility element supports: evidence of what acting on it
/// does (ADR-0017). No pattern is ever invoked; only their availability is read.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[allow(clippy::struct_excessive_bools)] // independent capabilities reported by the OS
pub struct AccessiblePatterns {
    /// A click activates it (UI Automation `Invoke`).
    pub invoke: bool,
    /// A click toggles it (`Toggle`).
    pub toggle: bool,
    /// A click opens or closes it (`ExpandCollapse`).
    pub expand_collapse: bool,
    /// A click selects it (`SelectionItem`).
    pub select: bool,
    /// Its text can be set (`Value` available and not read-only). The value itself is
    /// never read.
    pub editable_value: bool,
    /// It scrolls (`Scroll`). Not a click affordance.
    pub scroll: bool,
}

impl AccessiblePatterns {
    /// A pattern a click acts through.
    pub const fn clickable(&self) -> bool {
        self.invoke || self.toggle || self.expand_collapse || self.select
    }
}

/// One element of the attested foreground window's accessibility tree, as an
/// [`AccessibilityProvider`] reports it.
#[derive(Debug, Clone, PartialEq)]
pub struct AccessibleElement {
    /// Physical pixels of the captured frame, clipped to it.
    pub bbox: BBox,
    /// Mapped from the platform's control type by the provider; `Unknown` when the
    /// contract has no role for it.
    pub role: Role,
    /// The accessible name. Untrusted screen data, like OCR text: it labels, it never
    /// instructs and it never changes a permission.
    pub name: String,
    pub enabled: bool,
    /// The platform says it is not on screen (scrolled away, collapsed, covered).
    pub offscreen: bool,
    /// A password field.
    pub password: bool,
    pub patterns: AccessiblePatterns,
}

/// Reads the accessibility tree of the foreground window (UI Automation on Windows;
/// ADR-0017): evidence fused with the visual path, never a replacement for it. It reads
/// only the window of the application the actuator would attest (on Windows, the same
/// `ApplicationFrameHost` resolution), never another application's, and never a field's
/// value. Off by default ([`NoAccessibility`]).
pub trait AccessibilityProvider: Send {
    fn name(&self) -> &'static str;

    /// The elements whose boxes overlap `regions`. An empty result is valid (no
    /// foreground application, or nothing accessible there).
    fn snapshot(&mut self, regions: &[BBox]) -> Result<Vec<AccessibleElement>, PortError>;

    /// Whether [`keyboard_focus`](Self::keyboard_focus) answers. A provider that is off
    /// ([`NoAccessibility`]) says no, and `type` then relies on the actuator's own
    /// window-level focus check alone (ADR-0018).
    fn reads_keyboard_focus(&self) -> bool;

    /// The element that has keyboard focus now, in the attested foreground application:
    /// `None` when nothing has it, it is not on the captured monitor or on screen, or it
    /// belongs to another application. A read, never an action: it does not move focus and
    /// never reads the element's value (ADR-0017, ADR-0018).
    fn keyboard_focus(&mut self) -> Result<Option<AccessibleElement>, PortError>;
}

/// No accessibility evidence: the default, the simulated desktops and the fixtures.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoAccessibility;

impl AccessibilityProvider for NoAccessibility {
    fn name(&self) -> &'static str {
        "none"
    }

    fn snapshot(&mut self, _: &[BBox]) -> Result<Vec<AccessibleElement>, PortError> {
        Ok(Vec::new())
    }

    fn reads_keyboard_focus(&self) -> bool {
        false
    }

    fn keyboard_focus(&mut self) -> Result<Option<AccessibleElement>, PortError> {
        Ok(None)
    }
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

    /// The application in the foreground, as the actuator can attest it. `None` when it
    /// cannot (the default): live input then never passes the application allowlist. The
    /// engine asks at the gate and again right before delivery, and sends nothing unless
    /// both answers agree.
    fn foreground_application(&self) -> Option<String> {
        None
    }

    fn deliver(&mut self, action: &ValidatedAction) -> Result<DeliveryReceipt, PortError>;

    /// Whether [`deliver_focus_click`](Self::deliver_focus_click) and
    /// [`deliver_keys`](Self::deliver_keys) work. The engine delivers `type` in two phases
    /// (ADR-0018) only with an actuator that does and an accessibility provider that
    /// reads keyboard focus; otherwise `type` goes through [`deliver`](Self::deliver).
    fn delivers_in_phases(&self) -> bool {
        false
    }

    /// First phase of a `type` action: the pointer events that move to the target and
    /// click it, and nothing else. Every check of [`deliver`](Self::deliver) before the
    /// events (foreground application, integrity, held keys) applies. The receipt counts
    /// those events only. Nothing is retried.
    fn deliver_focus_click(
        &mut self,
        _action: &ValidatedAction,
    ) -> Result<DeliveryReceipt, PortError> {
        Err(PortError::new(
            "actuator",
            "this actuator does not deliver in phases",
        ))
    }

    /// Second phase of a `type` action: its keys. The engine calls it only after the first
    /// phase was delivered and keyboard focus on the target was confirmed; an actuator
    /// refuses it without that preceding click, and re-checks that the attested
    /// application is still in the foreground. The receipt counts the key events only.
    fn deliver_keys(&mut self, _action: &ValidatedAction) -> Result<DeliveryReceipt, PortError> {
        Err(PortError::new(
            "actuator",
            "this actuator does not deliver in phases",
        ))
    }
}

/// The local policy, owned by the operator and outside the agent's reach (chapter 08).
/// Its default denies everything: no actuation, no values, no applications.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LocalPolicy {
    pub actuation_enabled: bool,
    /// Whether `observe_screen` may return field contents when the agent asks for them.
    pub include_values: bool,
    pub allowed_applications: Vec<String>,
    /// Operator stop: no new input events while set. The engine reads it at the gate and
    /// again right before each delivery. Only the operator sets it (the composition root
    /// derives it from a local stop file or switch); nothing the agent sends reaches it.
    pub stopped: bool,
}

/// Reads the local policy. The agent has no port to write it. Read on every action, so
/// an implementation may reflect the operator's changes (the stop) as they happen.
pub trait PolicyStore: Send + Sync {
    fn current(&self) -> LocalPolicy;
}
