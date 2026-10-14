//! # ommatix-adapter-vision
//!
//! Driven adapters for the [`VisionDetector`](ommatix_application::VisionDetector) and
//! [`TextRecognizer`](ommatix_application::TextRecognizer) ports:
//!
//! - [`FlatRegionDetector`]: classic, deterministic, CPU-only region growing over the flat
//!   colours GUIs are drawn with (bootstrap ADR B0010). It finds boxes but has no evidence
//!   of roles or affordances, so it reports `Role::Unknown` and `Actionability::Unknown`:
//!   the thesis forbids inventing roles. It loads no model.
//! - `OnnxVisionDetector` (feature `onnx`): a generic YOLO-style detector on ONNX Runtime,
//!   fail-closed: it loads only weights whose SHA-256 matches a manifest the operator pinned
//!   (ADR-0009), and takes roles and affordances from that manifest, never from class
//!   names. The model is chosen in the S0 spike; no weights ship with this crate.
//! - [`NullTextRecognizer`] ("none"): reads no text.
//! - `WinRtTextRecognizer` (feature `winrt-ocr`, Windows only): the OCR engine built into
//!   Windows, for exactly one configured language.
//!
//! The model-independent pieces ([`model`]: manifest, integrity, letterbox, decoding, NMS;
//! [`crop`]; [`ocr`]) are always built, so they are tested without any model or OS API.

mod border;
pub mod crop;
mod flat_region;
mod forest;
mod grouping;
pub mod model;
pub mod ocr;
#[cfg(feature = "onnx")]
mod onnx;
mod segment;
mod text;
#[cfg(all(windows, feature = "winrt-ocr"))]
mod winrt_ocr;

pub use flat_region::{FlatRegionConfig, FlatRegionDetector};
#[cfg(feature = "onnx")]
pub use onnx::{MAX_MODEL_BYTES, MAX_RUNTIME_BYTES, OnnxDetectorSource, OnnxVisionDetector};
pub use text::NullTextRecognizer;
#[cfg(all(windows, feature = "winrt-ocr"))]
pub use winrt_ocr::{DEFAULT_LANGUAGE, WinRtTextRecognizer, installed_languages};
