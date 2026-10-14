//! Driven ports: what the application needs from the outside world.
//!
//! Driving adapters (e.g. the MCP server) do not need a port trait: they call the use
//! cases ([`PerceptionEngine`](crate::PerceptionEngine), [`Observer`](crate::Observer),
//! [`ActuationService`](crate::ActuationService)) directly.

use std::time::Duration;

use ommatix_domain::{Action, Detection, Frame, Rect};

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
/// yield the same detections. They may read pixels around `region` for context, but
/// only elements inside it are kept: the engine clips every bound to `region`.
pub trait ElementDetector: Send + Sync {
    fn name(&self) -> &'static str;

    fn detect(&self, frame: &Frame, region: Rect) -> Result<Vec<Detection>, PortError>;
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

/// Monotonic time and waiting. Injected so that stage timings and settle waits are
/// reproducible under test (see [`ManualClock`](crate::ManualClock)).
pub trait Clock: Send + Sync {
    /// Time elapsed since an arbitrary, fixed origin. Never goes backwards.
    fn now(&self) -> Duration;

    /// Waits for `duration` (a virtual clock simply advances).
    fn sleep(&self, duration: Duration);
}
