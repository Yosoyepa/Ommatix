//! The thesis use case: one transactional observation (`observe_screen`), one validated
//! action (`execute_action`) and the engine counters (`get_engine_metrics`), over the SDD
//! ports. Joins Algorithm 1 (region plan), Algorithm 2 (fusion), Algorithm 3B (identity
//! registry) and the snapshot store.
//!
//! - [`observe`]: capture → region plan → detect/recognise only the planned regions → fuse
//!   → track → stage and commit. A failed stage publishes nothing.
//! - [`view`]: the agent's view of the published snapshot (query filter, value redaction,
//!   pagination with a cursor bound to the snapshot).
//! - [`act`]: the action gate of SDD §2.9 and the honest delivery report.
//! - [`journal`]: `request_id` deduplication without storing typed text.

mod act;
mod journal;
mod observe;
mod view;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use ommatix_domain::fusion::FusionParams;
use ommatix_domain::identity::AssociationPolicy;
use ommatix_domain::incremental::{PlanParams, connected_components};
use ommatix_domain::snapshot::SnapshotStore;

use crate::contract::{
    ActionRequest, ActionResult, EngineMetrics, EngineMode, ObservePage, ObserveRequest,
    StageTimings, TimingsSource, ToolError,
};
use crate::ports::{Clock, FrameCapturer, OsActuator, PolicyStore, TextRecognizer, VisionDetector};

pub use journal::JOURNAL_CAPACITY;

use journal::Journal;
use observe::Published;

/// Engine parameters; the defaults are the thesis's initial values
/// (`plantillas/configuracion_mvp.toml`).
#[derive(Debug, Clone, PartialEq)]
pub struct EngineConfig {
    /// Opaque session identifier chosen by the composition root (1..=64 chars).
    pub session_id: String,
    pub plan: PlanParams,
    pub fusion: FusionParams,
    pub association: AssociationPolicy,
}

impl EngineConfig {
    pub fn new(session_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            plan: PlanParams::default(),
            fusion: FusionParams::default(),
            association: AssociationPolicy::default(),
        }
    }
}

/// The ports the engine drives.
#[allow(missing_debug_implementations)] // trait objects
pub struct Ports {
    pub capturer: Box<dyn FrameCapturer>,
    pub detector: Box<dyn VisionDetector>,
    pub recognizer: Box<dyn TextRecognizer>,
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
}

impl Engine {
    pub fn new(config: EngineConfig, ports: Ports) -> Self {
        Self {
            config,
            ports,
            store: SnapshotStore::new(),
            journal: Journal::new(JOURNAL_CAPACITY),
            counters: Counters::default(),
        }
    }

    pub fn session_id(&self) -> &str {
        &self.config.session_id
    }

    /// Captures, plans, detects and recognises only the planned regions, fuses, tracks and
    /// commits a new snapshot; or, with a cursor, pages the snapshot the cursor names.
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
}
