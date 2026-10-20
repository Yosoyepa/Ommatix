//! # ommatix-application
//!
//! The application hexagon. It defines the **driven ports** of the SDD (thesis chapter 02
//! §2.4) that the outside world implements ([`FrameCapturer`], [`VisionDetector`],
//! [`TextRecognizer`], [`AccessibilityProvider`], [`OsActuator`], [`Clock`],
//! [`PolicyStore`]), the Ommatix 1.0 tool contract as plain types ([`contract`]) and the one
//! use case that drives the domain:
//!
//! - [`Engine`]: `observe_screen` (capture, plan the dirty regions, detect, recognise and
//!   read the accessibility tree only there, fuse, label curated icons, track and commit a
//!   snapshot transactionally), `execute_action` (deduplicate, validate against the
//!   published snapshot and the local policy, dry run by default, deliver and verify
//!   honestly) and `get_engine_metrics`.
//!
//! It knows nothing about screens, files, MCP or JSON, and reads time only through the
//! [`Clock`] port.

mod clock;
pub mod contract;
mod engine;
mod error;
pub mod ports;
mod window_scope;

pub use clock::ManualClock;
pub use engine::{
    ActionFreshness, Engine, EngineConfig, FreshnessProbeCounts, FreshnessProbeGroup,
    IconResolverConfig, JOURNAL_CAPACITY, OccluderFallback, OccluderProbeCounts, Ports,
    VerifyParams, grey_grid,
};
pub use error::{PortError, error_chain};
pub use ports::{
    AccessibilityProvider, AccessibleElement, AccessiblePatterns, AccessibleRead, CapturedFrame,
    Clock, DeliveryReceipt, FrameCapturer, LocalPolicy, NativeIdentity, NoAccessibility,
    OsActuator, PolicyStore, TextRecognizer, TopLevelWindow, VisionDetector, WindowOpacity,
    WindowPin,
};
pub use window_scope::{window_scope_applies, window_scope_problem};
