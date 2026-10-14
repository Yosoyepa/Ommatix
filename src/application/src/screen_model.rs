use ommatix_domain::{Element, ElementId, Rect};

/// Read-only view of what is currently on screen. Lets use cases that need to resolve
/// targets (e.g. actuation) depend on an abstraction instead of the perception engine.
///
/// This is not a port to the outside world: the application implements it itself
/// ([`PerceptionEngine`](crate::PerceptionEngine)).
pub trait ScreenModel {
    /// Bounds of the last perceived frame, `None` before the first frame.
    fn screen_bounds(&self) -> Option<Rect>;

    fn element(&self, id: ElementId) -> Option<Element>;
}
