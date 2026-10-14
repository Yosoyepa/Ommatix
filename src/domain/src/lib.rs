//! # ommatix-domain
//!
//! The innermost hexagon: pure, deterministic types and algorithms for incremental
//! visual perception. This crate performs **no I/O**, spawns no threads, reads no clock,
//! has no serialisation format and depends on no other Ommatix crate.
//!
//! Pipeline (one tick):
//! 1. A [`ChangeTracker`] splits each [`Frame`] into independent cells ("ommatidia") with a
//!    [`GridSpec`], fingerprints them and reports the dirty cells merged into regions.
//! 2. The [`Scene`] grows those regions to whole elements ([`Scene::expand_regions`]).
//! 3. Detections found inside the regions update the [`Scene`], whose elements are served
//!    through a [`SpatialIndex`].

mod action;
mod change;
mod display;
mod element;
mod error;
mod frame;
mod geometry;
mod grid;
mod region;
mod scene;
mod spatial_index;
mod union_find;

pub use action::{Action, Key, MouseButton};
pub use change::{ChangeTracker, FrameChange, PendingChange};
pub use display::DisplayMapping;
pub use element::{Detection, Element, ElementId, ElementKind};
pub use error::DomainError;
pub use frame::Frame;
pub use geometry::{Point, Rect};
pub use grid::{CellCoord, GridSignature, GridSpec};
pub use region::{merge_dirty_cells, merge_overlapping};
pub use scene::{Scene, SceneDelta};
pub use spatial_index::SpatialIndex;
