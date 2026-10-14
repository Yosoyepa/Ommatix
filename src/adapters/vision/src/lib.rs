//! # ommatix-adapter-vision
//!
//! Driven adapters for the [`VisionDetector`](ommatix_application::VisionDetector) and
//! [`TextRecognizer`](ommatix_application::TextRecognizer) ports, until the models chosen
//! in the S0 spike arrive in S3:
//!
//! - [`FlatRegionDetector`]: classic, deterministic, CPU-only region growing over the flat
//!   colours GUIs are drawn with (bootstrap ADR B0010). It finds boxes but has no evidence
//!   of roles or affordances, so it reports `Role::Unknown` and `Actionability::Unknown`:
//!   the thesis forbids inventing roles. It loads no model.
//! - [`NullTextRecognizer`] ("none"): reads no text.

mod border;
mod flat_region;
mod forest;
mod grouping;
mod segment;
mod text;

pub use flat_region::{FlatRegionConfig, FlatRegionDetector};
pub use text::NullTextRecognizer;
