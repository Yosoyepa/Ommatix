//! The thesis use case: one transactional observation (`observe_screen`), one validated
//! action (`execute_action`) and the engine counters (`get_engine_metrics`), over the SDD
//! ports. Joins Algorithm 1 (region plan), Algorithm 2 (fusion), Algorithm 3B (identity
//! registry) and the snapshot store.
//!
//! SKELETON: the bodies are filled in by the S2b application work.

use std::sync::Arc;

use ommatix_domain::fusion::FusionParams;
use ommatix_domain::identity::AssociationPolicy;
use ommatix_domain::incremental::PlanParams;

use crate::contract::{
    ActionRequest, ActionResult, EngineMetrics, ObservePage, ObserveRequest, StageTimings,
    ToolError,
};
use crate::ports::{Clock, FrameCapturer, OsActuator, PolicyStore, TextRecognizer, VisionDetector};

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

/// Single writer of the published state (SDD §2.7).
#[allow(missing_debug_implementations)] // trait objects
pub struct Engine {
    config: EngineConfig,
    ports: Ports,
}

impl Engine {
    pub fn new(config: EngineConfig, ports: Ports) -> Self {
        Self { config, ports }
    }

    pub fn session_id(&self) -> &str {
        &self.config.session_id
    }

    /// Captures, plans, detects and recognises only the planned regions, fuses, tracks and
    /// commits a new snapshot; or, with a cursor, pages the snapshot the cursor names.
    pub fn observe_screen(&mut self, request: &ObserveRequest) -> Result<ObservePage, ToolError> {
        let _ = (request, &self.ports);
        Err(skeleton())
    }

    /// Validates and (dry-run by default) performs one action on a target by ID.
    pub fn execute_action(&mut self, request: &ActionRequest) -> ActionResult {
        ActionResult {
            request_id: request.request_id.clone(),
            status: ommatix_domain::identity::ActionStatus::Rejected,
            target_id: request.target_id.clone(),
            snapshot_id: request.snapshot_id.clone(),
            error: Some(skeleton()),
            observed_effect: None,
        }
    }

    pub fn engine_metrics(&self) -> EngineMetrics {
        EngineMetrics {
            mode: crate::contract::EngineMode::Simulation,
            observations: 0,
            rejected_actions: 0,
            delivered_actions: 0,
            pending_regions: 0,
            models_loaded: false,
            timings_source: crate::contract::TimingsSource::NotMeasured,
        }
    }

    /// Timings of the last committed observation, for MCP `_meta`.
    pub fn last_timings(&self) -> Option<StageTimings> {
        None
    }
}

fn skeleton() -> ToolError {
    ToolError {
        code: crate::contract::ErrorCode::Cancelled,
        message: "not implemented yet".to_owned(),
        retryable: false,
        next_step: crate::contract::NextStep::Stop,
    }
}
