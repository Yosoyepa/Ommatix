//! # ommatix-domain
//!
//! The innermost hexagon: pure, deterministic types and algorithms for incremental
//! visual perception. This crate performs **no I/O**, spawns no threads, reads no clock,
//! has no serialisation format and depends on no other Ommatix crate.
//!
//! ## Thesis core
//!
//! - [`incremental`]: Algorithm 1. BGRA frames, dirty tiles against the last committed
//!   base, contextual closure and the region plan (or the explicit full-refresh fallback).
//! - [`fusion`]: Algorithm 2. Conservative spatial fusion of controls and OCR words
//!   (IoC ownership, reading order, external labels, text nodes, affordances).
//! - [`candidates`]: how Algorithm 2 enumerates its candidate controls: the reference scan,
//!   or the experimental R*-tree (ADR-0027, off by default).
//! - [`appearance`]: Algorithm 3A. Masked gradient descriptors and the open-set icon cache.
//! - [`identity`]: Algorithm 3B. The persistent identity registry and the action check.
//! - [`snapshot`]: the transactional staging/commit store.
//! - [`bbox`]: the geometry those algorithms share.
//!
//! ## Support types for the driven adapters
//!
//! What the adapters still need from the bootstrap prototype, until S3 replaces them: an
//! RGBA [`Frame`] with integer [`Rect`]/[`Point`] geometry (the simulated desktop's
//! renderer, scripted captures and the flat-region detector), the detector's raw
//! [`Detection`]s, the simulated user's [`Action`]s, and the [`DisplayMapping`] from frame
//! pixels to OS input coordinates (screen capture and native input).

pub mod appearance;
pub mod bbox;
pub mod candidates;
pub mod fusion;
pub mod identity;
pub mod incremental;
pub mod snapshot;

mod action;
mod display;
mod element;
mod error;
mod frame;
mod geometry;

pub use action::{Action, Key, MouseButton};
pub use display::DisplayMapping;
pub use element::{Detection, ElementKind};
pub use error::DomainError;
pub use frame::Frame;
pub use geometry::{Point, Rect};
