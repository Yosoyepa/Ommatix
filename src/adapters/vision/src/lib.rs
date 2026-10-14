//! # ommatix-adapter-vision
//!
//! Driven adapter for the [`ElementDetector`](ommatix_application::ElementDetector) port.
//! The MVP ships one detector, [`ContrastBlobDetector`]: classic, deterministic,
//! CPU-only connected-component analysis. Learned detectors (OCR, widget classifiers)
//! are future adapters behind the same port.

mod contrast_blob;

pub use contrast_blob::{ContrastBlobConfig, ContrastBlobDetector};
