//! # ommatix-adapter-vision
//!
//! Driven adapter for the [`ElementDetector`](ommatix_application::ElementDetector) port.
//! The MVP ships one detector, [`FlatRegionDetector`]: classic, deterministic, CPU-only
//! region growing over the flat colours GUIs are drawn with (bootstrap ADR B0010).
//! Learned detectors (OCR, widget classifiers) are future adapters behind the same port.

mod border;
mod flat_region;
mod forest;
mod grouping;
mod segment;

pub use flat_region::{FlatRegionConfig, FlatRegionDetector};
