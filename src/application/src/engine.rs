//! The thesis use case: one transactional observation (`observe_screen`), one validated
//! action (`execute_action`) and the engine counters (`get_engine_metrics`), over the SDD
//! ports. Joins Algorithm 1 (region plan), Algorithm 2 (fusion), Algorithm 3A (appearance
//! cache), Algorithm 3B (identity registry) and the snapshot store.
//!
//! - [`observe`]: capture → region plan → detect/recognise/read the accessibility tree only
//!   in the planned regions (concurrently, ADR-0019) → reconcile and fuse → resolve icons →
//!   track → stage and commit. A failed stage publishes nothing.
//! - [`icons`]: Algorithm 3A on icon-like controls of the planned regions.
//! - [`view`]: the agent's view of the published snapshot (query filter, value redaction,
//!   pagination with a cursor bound to the snapshot).
//! - [`act`]: the action gate of SDD §2.9 and the honest delivery report.
//! - [`journal`]: `request_id` deduplication without storing typed text.

mod accessibility;
mod act;
mod icons;
mod journal;
mod observe;
mod view;

#[cfg(test)]
mod tests;

use std::sync::Arc;
use std::time::Duration;

use ommatix_domain::fusion::FusionParams;
use ommatix_domain::identity::{AssociationPolicy, Changes, Registry};
use ommatix_domain::incremental::{PlanParams, connected_components};
use ommatix_domain::snapshot::SnapshotStore;

use crate::contract::{
    ActionRequest, ActionResult, ActionTimings, EngineMetrics, EngineMode, ObservePage,
    ObserveRequest, StageTimings, TimingsSource, ToolError,
};
use crate::ports::{
    AccessibilityProvider, Clock, FrameCapturer, OsActuator, PolicyStore, TextRecognizer,
    VisionDetector,
};

pub use icons::IconResolverConfig;
pub use journal::JOURNAL_CAPACITY;

use journal::Journal;
use observe::Published;

/// How long the engine looks for an action's visual effect after delivering it (SDD §2.9,
/// chapter 07 §7.4: the deadline of BDD-29). After delivery it captures, compares with
/// the frame captured right before delivery and, while no change shows, waits `poll` on
/// the injected [`Clock`] and captures again, until `timeout` has elapsed since delivery.
/// A change within the window is `effect_observed`; none is `indeterminate`. Only the
/// screen is read again: the input is never sent twice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VerifyParams {
    /// The window after delivery. Zero: a single check right after delivery.
    pub timeout: Duration,
    /// The wait between two verification captures (at least 1 ms is used).
    pub poll: Duration,
}

impl VerifyParams {
    /// `[engine] verify_timeout_ms` of the dossier template.
    pub const DEFAULT_TIMEOUT: Duration = Duration::from_millis(500);
    /// `[engine] verify_poll_ms` of the dossier template.
    pub const DEFAULT_POLL: Duration = Duration::from_millis(50);
}

impl Default for VerifyParams {
    fn default() -> Self {
        Self {
            timeout: Self::DEFAULT_TIMEOUT,
            poll: Self::DEFAULT_POLL,
        }
    }
}

/// Whether `since` lies more than `max` before `now`, both on the injected clock. The bound
/// is inclusive: an age of exactly `max` is still within it (ADR-0025).
fn aged_out(now: Duration, since: Duration, max: Duration) -> bool {
    now.saturating_sub(since) > max
}

/// Engine parameters; the defaults are the thesis's initial values
/// (`plantillas/configuracion_mvp.toml`).
#[derive(Debug, Clone, PartialEq)]
pub struct EngineConfig {
    /// Opaque session identifier chosen by the composition root (1..=64 chars).
    pub session_id: String,
    pub plan: PlanParams,
    pub fusion: FusionParams,
    pub association: AssociationPolicy,
    /// Algorithm 3A. `None` (the default): no icon is resolved and icon labels stay
    /// unknown.
    pub icons: Option<IconResolverConfig>,
    /// The settle window of effect verification.
    pub verify: VerifyParams,
    /// Whether detection, text recognition and the accessibility read of one observation
    /// run concurrently (SDD §2.7, ADR-0019). The evidence is the same either way; off, the
    /// stages run one after the other.
    pub concurrent_stages: bool,
    /// How old, on the injected [`Clock`], the capture of the published snapshot and the
    /// accessibility read that confirms a target may be when an action is authorised
    /// (ADR-0025). Checked at the gate and again before every real input.
    pub max_snapshot_age: Duration,
}

impl EngineConfig {
    /// `[engine] max_snapshot_age_ms` of the dossier template (ADR-0025). Wider than the
    /// first 250 ms: an agent acts seconds after it observes, and the freshness capture
    /// before each delivery still compares the screen with the snapshot.
    pub const DEFAULT_MAX_SNAPSHOT_AGE: Duration = Duration::from_millis(10_000);

    pub fn new(session_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            plan: PlanParams::default(),
            fusion: FusionParams::default(),
            association: AssociationPolicy::default(),
            icons: None,
            verify: VerifyParams::default(),
            concurrent_stages: true,
            max_snapshot_age: Self::DEFAULT_MAX_SNAPSHOT_AGE,
        }
    }
}

/// The ports the engine drives.
#[allow(missing_debug_implementations)] // trait objects
pub struct Ports {
    pub capturer: Box<dyn FrameCapturer>,
    pub detector: Box<dyn VisionDetector>,
    pub recognizer: Box<dyn TextRecognizer>,
    /// Accessibility evidence (ADR-0017); [`NoAccessibility`](crate::NoAccessibility) when
    /// off.
    pub accessibility: Box<dyn AccessibilityProvider>,
    pub actuator: Box<dyn OsActuator>,
    pub policy: Arc<dyn PolicyStore>,
    pub clock: Arc<dyn Clock>,
}

/// Counters reported by `get_engine_metrics`.
#[derive(Debug, Clone, Copy, Default)]
struct Counters {
    rejected_actions: u64,
    delivered_actions: u64,
}

/// Single writer of the published state (SDD §2.7).
#[allow(missing_debug_implementations)] // trait objects
pub struct Engine {
    config: EngineConfig,
    ports: Ports,
    store: SnapshotStore<Published>,
    journal: Journal,
    counters: Counters,
    /// Input was handed to the actuator since the last commit (whatever its outcome): the
    /// published snapshot may no longer show the screen and cannot be acted on again.
    snapshot_spent: bool,
    /// Stage durations of the last action the gate ran (`None` for a replay or a request
    /// refused before the gate).
    action_timings: Option<ActionTimings>,
    /// The identity counters of the snapshot the last purge deleted, without its tracks: the
    /// next observation builds on them, so no snapshot or target ID is issued twice.
    purged_identity: Option<Registry>,
}

impl Engine {
    pub fn new(config: EngineConfig, ports: Ports) -> Self {
        Self {
            config,
            ports,
            store: SnapshotStore::new(),
            journal: Journal::new(JOURNAL_CAPACITY),
            counters: Counters::default(),
            snapshot_spent: false,
            action_timings: None,
            purged_identity: None,
        }
    }

    /// Deletes the engine's state and cache (chapter 08 §8.4): the published snapshot with its
    /// tracks, evidence, icon labels and timings, the committed base frame (overwritten when
    /// nothing else holds it), the pending regions and the request journal. Nothing is
    /// published until the next observation, and actions are refused until then.
    ///
    /// Kept: the configuration and the operator's files, which are not the engine's state, and
    /// the identity counters (session, geometry epoch, snapshot and target counters), so that
    /// no snapshot or target identifier is ever issued again. Those are numbers, not screen
    /// content; the action counters of `get_engine_metrics` are kept too.
    pub fn purge(&mut self) {
        if let Some(published) = self.store.purge() {
            self.purged_identity = Some(published.registry.purged());
        }
        self.journal = Journal::new(JOURNAL_CAPACITY);
        self.action_timings = None;
    }

    pub fn session_id(&self) -> &str {
        &self.config.session_id
    }

    /// Captures, plans, detects, recognises and reads accessibility evidence only in the
    /// planned regions, fuses, tracks and commits a new snapshot; or, with a cursor, pages the
    /// snapshot the cursor names.
    pub fn observe_screen(&mut self, request: &ObserveRequest) -> Result<ObservePage, ToolError> {
        view::check_request(request)?;
        if let Some(cursor) = &request.cursor {
            return self.next_page(request, cursor);
        }
        self.observe()?;
        self.first_page(request)
    }

    /// Validates and (dry-run by default) performs one action on a target by ID.
    pub fn execute_action(&mut self, request: &ActionRequest) -> ActionResult {
        self.act(request)
    }

    pub fn engine_metrics(&self) -> EngineMetrics {
        let observed = self.store.revision() > 0;
        EngineMetrics {
            mode: if self.ports.actuator.is_live() {
                EngineMode::Native
            } else {
                EngineMode::Simulation
            },
            observations: self.store.revision(),
            rejected_actions: self.counters.rejected_actions,
            delivered_actions: self.counters.delivered_actions,
            pending_regions: connected_components(self.store.pending_dirty()).len() as u64,
            models_loaded: self.ports.detector.models_loaded(),
            timings_source: if observed {
                TimingsSource::RuntimeMonotonic
            } else {
                TimingsSource::NotMeasured
            },
        }
    }

    /// Timings of the last committed observation, for MCP `_meta`.
    pub fn last_timings(&self) -> Option<StageTimings> {
        self.store.published().map(|p| p.timings)
    }

    /// Whether the accessibility evidence of the published snapshot is complete: `false`
    /// while a bounded read left regions the next observation will read again. `None`
    /// before the first observation. For MCP `_meta`.
    pub fn accessibility_complete(&self) -> Option<bool> {
        self.store
            .published()
            .map(|p| p.accessibility_gaps.is_empty())
    }

    /// Why the tracks of the published snapshot retired in the observation that published it
    /// (ADR-0026): codes only, never a label, a discriminant or a box. `None` before the
    /// first observation.
    pub fn last_identity_changes(&self) -> Option<&Changes> {
        self.store.published().map(|p| &p.changes)
    }

    /// Stage durations of the last `execute_action` the gate ran, for MCP `_meta`. `None`
    /// when that call was a replay of an earlier `request_id` or was refused before the
    /// gate (arguments, `request_id` conflict).
    pub const fn last_action_timings(&self) -> Option<ActionTimings> {
        self.action_timings
    }
}
