//! Driven ports: what the application needs from the outside world.
//!
//! Driving adapters (e.g. the MCP server) do not need a port trait: they call
//! [`PerceptionEngine`](crate::PerceptionEngine) directly.

use ommatix_domain::{Action, Detection, Element, ElementId, Frame, Rect};

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

/// Performs input on the desktop (mouse, keyboard).
///
/// Implementations that do not touch the real OS (dry-run, simulators) must report
/// `is_live() == false` so callers and agents can tell the difference.
pub trait InputController: Send {
    fn name(&self) -> &'static str;

    fn is_live(&self) -> bool;

    fn perform(&mut self, action: &Action) -> Result<(), PortError>;
}

/// Read-only view of what is currently on screen. Lets use cases that need to resolve
/// targets (e.g. actuation) depend on an abstraction instead of the perception engine.
pub trait ScreenModel {
    /// Bounds of the last perceived frame, `None` before the first frame.
    fn screen_bounds(&self) -> Option<Rect>;

    fn element(&self, id: ElementId) -> Option<Element>;
}
