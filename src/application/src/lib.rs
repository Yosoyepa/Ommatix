//! # ommatix-application
//!
//! The application hexagon. It defines the **driven ports** of the SDD (thesis chapter 02
//! §2.4) that the outside world implements ([`FrameCapturer`], [`VisionDetector`],
//! [`TextRecognizer`], [`OsActuator`], [`Clock`], [`PolicyStore`]), the Ommatix 1.0 tool
//! contract as plain types ([`contract`]) and the one use case that drives the domain:
//!
//! - [`Engine`]: `observe_screen` (capture, plan the dirty regions, detect and recognise
//!   only those, fuse, label curated icons, track and commit a snapshot
//!   transactionally), `execute_action`
//!   (deduplicate, validate against the published snapshot and the local policy, dry run
//!   by default, deliver and verify honestly) and `get_engine_metrics`.
//!
//! It knows nothing about screens, files, MCP or JSON, and reads time only through the
//! [`Clock`] port.

mod clock;
pub mod contract;
mod engine;
mod error;
pub mod ports;

pub use clock::ManualClock;
pub use engine::{Engine, EngineConfig, IconResolverConfig, JOURNAL_CAPACITY, Ports};
pub use error::{PortError, error_chain};
pub use ports::{
    CapturedFrame, Clock, DeliveryReceipt, FrameCapturer, LocalPolicy, OsActuator, PolicyStore,
    TextRecognizer, VisionDetector,
};
