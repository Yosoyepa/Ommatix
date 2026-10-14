//! # ommatix-application
//!
//! The application hexagon. It defines the **ports** the outside world must implement
//! ([`FrameSource`], [`ElementDetector`], [`InputController`], [`Clock`]) and the **use
//! cases** that drive the domain:
//!
//! - [`PerceptionEngine`]: capture, diff, detect only what changed, keep the scene.
//! - [`Observer`]: after an action, perceive until the screen settles.
//! - [`ActuationService`]: validate an agent's action against the screen, then perform it.
//! - [`AgentSession`]: the three wired together for a driving adapter.
//!
//! It knows nothing about screens, files, MCP or JSON.

mod actuation;
mod clock;
pub mod contract;
mod engine;
mod error;
mod observe;
mod perception;
pub mod ports;
mod screen_model;
mod session;

pub use actuation::{
    ActionReceipt, ActuationError, ActuationPolicy, ActuationService, ClickTarget,
};
pub use clock::ManualClock;
pub use engine::{Engine, EngineConfig, Ports};
pub use error::{AppError, PortError, error_chain};
pub use observe::{MAX_SETTLE_TIMEOUT, Observation, ObserveError, Observer, SettlePolicy};
pub use perception::{PerceptionConfig, PerceptionEngine, PerceptionReport, Timings};
pub use ports::{
    CapturedFrame, Clock, DeliveryReceipt, ElementDetector, FrameCapturer, FrameSource,
    InputController, LocalPolicy, OsActuator, PolicyStore, TextRecognizer, VisionDetector,
};
pub use screen_model::ScreenModel;
pub use session::{ActionOutcome, AgentSession};
