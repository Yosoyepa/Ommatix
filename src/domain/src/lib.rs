//! # ommatix-domain
//!
//! The innermost hexagon: pure, deterministic types and algorithms for incremental
//! visual perception. This crate performs **no I/O**, spawns no threads and depends on
//! no other Ommatix crate.
//!
//! Pipeline (one tick):
//! 1. A [`Frame`] is split by a [`GridSpec`] into independent cells ("ommatidia").
//! 2. Each cell is fingerprinted into a [`GridSignature`]; comparing two signatures yields
//!    the dirty cells.
//! 3. Dirty cells are merged into rectangular regions ([`merge_dirty_cells`]).
//! 4. Detections inside those regions update the [`Scene`], whose elements are served
//!    through a [`SpatialIndex`].

mod element;
mod error;
mod frame;
mod geometry;
mod grid;
mod region;
mod scene;
mod spatial_index;

pub use element::{Detection, Element, ElementId, ElementKind};
pub use error::DomainError;
pub use frame::Frame;
pub use geometry::{Point, Rect};
pub use grid::{CellCoord, GridSignature, GridSpec};
pub use region::merge_dirty_cells;
pub use scene::{Scene, SceneDelta};
pub use spatial_index::SpatialIndex;
