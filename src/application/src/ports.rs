//! Driven ports: what the application needs from the outside world.
//!
//! Driving adapters (e.g. the MCP server) do not need a port trait: they call
//! [`PerceptionEngine`](crate::PerceptionEngine) directly.

use ommatix_domain::{Detection, Frame, Rect};

use crate::PortError;

/// Produces frames: a real screen, a recorded image sequence or a synthetic script.
pub trait FrameSource: Send {
    /// Human-readable adapter name, used in diagnostics.
    fn name(&self) -> &'static str;

    /// Returns the next frame, or `None` when the source is exhausted.
    fn next_frame(&mut self) -> Result<Option<Frame>, PortError>;
}

/// Finds visual elements inside a region of a frame.
///
/// Implementations must be **deterministic**: the same pixels and region must always
/// yield the same detections, and every returned bound must lie inside `region`.
pub trait ElementDetector: Send + Sync {
    fn name(&self) -> &'static str;

    fn detect(&self, frame: &Frame, region: Rect) -> Vec<Detection>;
}
