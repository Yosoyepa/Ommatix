//! The Ommatix 1.0 tool contract as application types (thesis chapter 07): requests,
//! results and the error taxonomy of `observe_screen`, `execute_action` and
//! `get_engine_metrics`. No serialisation here: `adapter-mcp` maps these to the JSON
//! schemas in `docs/thesis/contratos/esquemas/`.

use std::fmt;
use std::time::Duration;

use ommatix_domain::fusion::{Actionability, Role, TextKind};
use ommatix_domain::identity::{ActionStatus, TargetId};

/// Contract version, independent of the MCP protocol version.
pub const SCHEMA_VERSION: &str = "1.0";

/// Limits of the contract (schemas and RNF-09).
pub mod limits {
    pub const MAX_QUERY_CHARS: usize = 256;
    pub const MIN_LIMIT: u16 = 1;
    pub const MAX_LIMIT: u16 = 200;
    pub const DEFAULT_LIMIT: u16 = 100;
    pub const MAX_CURSOR_CHARS: usize = 128;
    pub const MAX_ID_CHARS: usize = 64;
    pub const MAX_TYPE_TEXT_CHARS: usize = 2048;
    pub const MAX_SCROLL_STEPS: u8 = 5;
    pub const MAX_LABEL_CHARS: usize = 128;
    pub const MAX_TEXT_CHARS: usize = 512;
    pub const MAX_MESSAGE_CHARS: usize = 256;
}

// ---- errors ----

/// Ommatix error codes (chapter 07 §7.7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ErrorCode {
    StaleSnapshot,
    StaleTarget,
    TargetNotFound,
    AmbiguousTarget,
    NotActionable,
    PolicyDenied,
    PermissionDenied,
    CaptureUnavailable,
    InferenceFailed,
    UnsupportedGeometry,
    IndeterminateEffect,
    RequestConflict,
    Cancelled,
    LimitExceeded,
    SessionMismatch,
    EpochMismatch,
}

impl ErrorCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::StaleSnapshot => "STALE_SNAPSHOT",
            Self::StaleTarget => "STALE_TARGET",
            Self::TargetNotFound => "TARGET_NOT_FOUND",
            Self::AmbiguousTarget => "AMBIGUOUS_TARGET",
            Self::NotActionable => "NOT_ACTIONABLE",
            Self::PolicyDenied => "POLICY_DENIED",
            Self::PermissionDenied => "PERMISSION_DENIED",
            Self::CaptureUnavailable => "CAPTURE_UNAVAILABLE",
            Self::InferenceFailed => "INFERENCE_FAILED",
            Self::UnsupportedGeometry => "UNSUPPORTED_GEOMETRY",
            Self::IndeterminateEffect => "INDETERMINATE_EFFECT",
            Self::RequestConflict => "REQUEST_CONFLICT",
            Self::Cancelled => "CANCELLED",
            Self::LimitExceeded => "LIMIT_EXCEEDED",
            Self::SessionMismatch => "SESSION_MISMATCH",
            Self::EpochMismatch => "EPOCH_MISMATCH",
        }
    }
}

/// What the agent should do next (chapter 07 §7.7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NextStep {
    ObserveAgain,
    OperatorReview,
    ReduceRequest,
    ReconcileEffect,
    Stop,
}

impl NextStep {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ObserveAgain => "observe_again",
            Self::OperatorReview => "operator_review",
            Self::ReduceRequest => "reduce_request",
            Self::ReconcileEffect => "reconcile_effect",
            Self::Stop => "stop",
        }
    }
}

/// An expected domain error, returned as a tool result with `isError: true`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolError {
    pub code: ErrorCode,
    /// English, at most [`limits::MAX_MESSAGE_CHARS`] characters, never screen content.
    pub message: String,
    pub retryable: bool,
    pub next_step: NextStep,
}

impl ErrorCode {
    /// Whether repeating the same call may succeed (chapter 07 §7.7). Never a licence to
    /// resend an action blindly: actions are deduplicated by `request_id`.
    pub const fn retryable(self) -> bool {
        matches!(self, Self::CaptureUnavailable | Self::InferenceFailed)
    }

    /// What the agent should do after this error.
    pub const fn next_step(self) -> NextStep {
        match self {
            Self::StaleSnapshot
            | Self::StaleTarget
            | Self::TargetNotFound
            | Self::SessionMismatch
            | Self::EpochMismatch
            | Self::AmbiguousTarget
            | Self::NotActionable
            | Self::CaptureUnavailable
            | Self::InferenceFailed => NextStep::ObserveAgain,
            Self::PolicyDenied | Self::PermissionDenied | Self::RequestConflict => {
                NextStep::OperatorReview
            }
            Self::UnsupportedGeometry | Self::Cancelled => NextStep::Stop,
            Self::IndeterminateEffect => NextStep::ReconcileEffect,
            Self::LimitExceeded => NextStep::ReduceRequest,
        }
    }
}

impl ToolError {
    /// An error with the `retryable` flag and `next_step` of its code. The message is cut
    /// to [`limits::MAX_MESSAGE_CHARS`] characters; it must never carry screen content.
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        let mut message = message.into();
        truncate_chars(&mut message, limits::MAX_MESSAGE_CHARS);
        Self {
            code,
            message,
            retryable: code.retryable(),
            next_step: code.next_step(),
        }
    }
}

/// Cuts `text` to at most `max` characters, on a character boundary.
pub(crate) fn truncate_chars(text: &mut String, max: usize) {
    if let Some((cut, _)) = text.char_indices().nth(max) {
        text.truncate(cut);
    }
}

impl fmt::Display for ToolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code.as_str(), self.message)
    }
}

impl std::error::Error for ToolError {}

// ---- observe_screen ----

/// `observe_screen` arguments after schema validation, with the runtime defaults applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObserveRequest {
    pub query: Option<String>,
    /// 1..=200; [`limits::DEFAULT_LIMIT`] when omitted.
    pub limit: u16,
    pub cursor: Option<String>,
    /// `false` when omitted; the local policy may still redact values.
    pub include_values: bool,
}

impl Default for ObserveRequest {
    fn default() -> Self {
        Self {
            query: None,
            limit: limits::DEFAULT_LIMIT,
            cursor: None,
            include_values: false,
        }
    }
}

/// Where an element's label came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LabelSource {
    Ocr,
    CuratedIcon,
    Model,
    Accessibility,
    Unknown,
}

/// Coarse screen zone. `Unknown` unless there is evidence for one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Zone {
    TopBar,
    LeftSidebar,
    CenterMain,
    BottomBar,
    Unknown,
}

/// One element as the agent sees it: no coordinates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElementView {
    pub target_id: TargetId,
    pub revision: u64,
    pub role: Role,
    /// Empty when unknown; at most [`limits::MAX_LABEL_CHARS`] characters.
    pub label: String,
    pub label_source: LabelSource,
    pub actionability: Actionability,
    pub enabled: Option<bool>,
    pub visible: Option<bool>,
    /// Field content and its kind, only when values were requested and the policy allows
    /// them; at most [`limits::MAX_TEXT_CHARS`] characters.
    pub text: Option<(String, TextKind)>,
    pub zone: Zone,
}

/// Per-stage durations of one observation. Run-dependent: MCP `_meta` only.
///
/// Each stage reports its own duration. Detection, text recognition and the accessibility
/// read run concurrently (SDD §2.7, ADR-0019), so those three overlap and do not add up to
/// `total`, which is the wall-clock time of the whole observation measured directly
/// (chapter 09 §9.6: overlapping stages are never summed).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StageTimings {
    pub capture: Duration,
    pub plan: Duration,
    pub detect: Duration,
    pub recognize: Duration,
    /// Reading the accessibility tree (zero when it is off or nothing was planned).
    pub accessibility: Duration,
    pub fuse: Duration,
    pub track: Duration,
    pub total: Duration,
}

/// Per-stage durations of one `execute_action`. Run-dependent: MCP `_meta` only.
/// Stages an action did not reach are zero (a dry run has no delivery).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ActionTimings {
    /// The checks before anything is captured or delivered: arguments, snapshot, registry,
    /// policy, allowlist.
    pub gate: Duration,
    /// The capture that confirms the screen is still the committed base, and the last look
    /// at the operator's stop and the foreground application.
    pub freshness: Duration,
    /// The actuator's calls (for two-phase `type`: the click and the keys).
    pub delivery: Duration,
    /// Waiting for UI Automation to report keyboard focus on the target (two-phase `type`).
    pub focus: Duration,
    /// Looking for the action's visual effect after delivery.
    pub verification: Duration,
    pub total: Duration,
}

/// One page of a full snapshot (`mode: "full"`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservePage {
    pub session_id: String,
    /// `s<n>`.
    pub snapshot_id: String,
    pub geometry_epoch: u64,
    pub frame_seq: u64,
    pub age_ms: u64,
    pub degraded: bool,
    /// Sorted by target ID; at most `limit` items.
    pub elements: Vec<ElementView>,
    /// Elements in the filtered view, across all pages.
    pub total_elements: usize,
    /// Present exactly when the page is truncated; bound to this snapshot.
    pub next_cursor: Option<String>,
    pub timings: StageTimings,
}

impl ObservePage {
    pub const fn truncated(&self) -> bool {
        self.next_cursor.is_some()
    }
}

// ---- execute_action ----

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ScrollDirection {
    Up,
    Down,
}

/// The MVP actions. None of them carries coordinates.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ActionKind {
    Click,
    /// 1..=2048 characters.
    Type {
        text: String,
    },
    /// 1..=5 steps.
    Scroll {
        direction: ScrollDirection,
        steps: u8,
    },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ExpectedEffect {
    #[default]
    None,
    FocusChanged,
    ValueUpdated,
    ScrollMoved,
    VisualChange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ObservedEffect {
    None,
    FocusChanged,
    ValueUpdated,
    ScrollMoved,
    VisualChange,
    Unknown,
}

/// `execute_action` arguments after schema validation, with `dry_run` defaulting to true.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ActionRequest {
    pub request_id: String,
    pub session_id: String,
    pub snapshot_id: String,
    pub geometry_epoch: u64,
    pub target_id: String,
    pub expected_revision: u64,
    pub action: ActionKind,
    pub dry_run: bool,
    pub expected_effect: ExpectedEffect,
}

/// The outcome of `execute_action`. `verified` derives from `status`; so does
/// `input_delivered`, except for `indeterminate`, which carries its own (chapter 07 §7.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionResult {
    pub request_id: String,
    pub status: ActionStatus,
    pub target_id: String,
    pub snapshot_id: String,
    /// Present when rejected; may be present when indeterminate.
    pub error: Option<ToolError>,
    pub observed_effect: Option<ObservedEffect>,
    /// Whether the actuator reported every input event as inserted by the OS. Implied by
    /// `input_delivered` and `effect_observed`, never true for `simulated` or `rejected`;
    /// for `indeterminate` it tells a delivered action whose effect is unverified (true)
    /// from one whose delivery itself is uncertain (false). It never claims an effect.
    pub delivered: bool,
}

impl ActionResult {
    pub const fn input_delivered(&self) -> bool {
        self.delivered
    }

    pub const fn verified(&self) -> bool {
        self.status.verified()
    }
}

/// An action that passed every check. Only the engine's gate builds one, so an actuator
/// can never be handed an unvalidated action. The point is internal: it never goes on the
/// wire.
#[derive(Debug, Clone, PartialEq)]
pub struct ValidatedAction {
    pub(crate) target_id: TargetId,
    pub(crate) kind: ActionKind,
    /// Physical pixel inside the target's visible box.
    pub(crate) point: (f64, f64),
}

impl ValidatedAction {
    pub const fn target_id(&self) -> TargetId {
        self.target_id
    }

    pub const fn kind(&self) -> &ActionKind {
        &self.kind
    }

    pub const fn point(&self) -> (f64, f64) {
        self.point
    }
}

// ---- get_engine_metrics ----

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineMode {
    Simulation,
    Native,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimingsSource {
    NotMeasured,
    RuntimeMonotonic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EngineMetrics {
    pub mode: EngineMode,
    pub observations: u64,
    pub rejected_actions: u64,
    pub delivered_actions: u64,
    pub pending_regions: u64,
    pub models_loaded: bool,
    pub timings_source: TimingsSource,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_error_takes_retryable_and_next_step_from_its_code() {
        use ErrorCode::*;
        let table = [
            (StaleSnapshot, false, NextStep::ObserveAgain),
            (StaleTarget, false, NextStep::ObserveAgain),
            (TargetNotFound, false, NextStep::ObserveAgain),
            (SessionMismatch, false, NextStep::ObserveAgain),
            (EpochMismatch, false, NextStep::ObserveAgain),
            (AmbiguousTarget, false, NextStep::ObserveAgain),
            (NotActionable, false, NextStep::ObserveAgain),
            (PolicyDenied, false, NextStep::OperatorReview),
            (PermissionDenied, false, NextStep::OperatorReview),
            (CaptureUnavailable, true, NextStep::ObserveAgain),
            (InferenceFailed, true, NextStep::ObserveAgain),
            (UnsupportedGeometry, false, NextStep::Stop),
            (IndeterminateEffect, false, NextStep::ReconcileEffect),
            (RequestConflict, false, NextStep::OperatorReview),
            (Cancelled, false, NextStep::Stop),
            (LimitExceeded, false, NextStep::ReduceRequest),
        ];
        for (code, retryable, next_step) in table {
            let e = ToolError::new(code, "m");
            assert_eq!(
                (e.retryable, e.next_step),
                (retryable, next_step),
                "{code:?}"
            );
        }
    }

    #[test]
    fn tool_error_message_is_cut_on_a_char_boundary() {
        let e = ToolError::new(ErrorCode::Cancelled, "é".repeat(300));
        assert_eq!(e.message.chars().count(), limits::MAX_MESSAGE_CHARS);
    }
}
