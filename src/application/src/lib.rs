//! # ommatix-application
//!
//! The application hexagon. It defines the **ports** the outside world must implement
//! ([`FrameSource`], [`ElementDetector`]) and the **use cases** that drive the domain
//! ([`PerceptionEngine`]). It knows nothing about screens, files, MCP or JSON.

mod error;
mod perception;
pub mod ports;

pub use error::{AppError, PortError};
pub use perception::{PerceptionConfig, PerceptionEngine, PerceptionReport, Timings};
pub use ports::{ElementDetector, FrameSource};
