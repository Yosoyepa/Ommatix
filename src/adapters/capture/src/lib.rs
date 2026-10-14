//! # ommatix-adapter-capture
//!
//! Driven adapters for the [`FrameCapturer`](ommatix_application::FrameCapturer) port.
//! Every capturer returns BGRA frames with an increasing `frame_seq`, a capture instant
//! from the injected clock and a geometry epoch that starts at 1 and grows whenever the
//! frame size changes.
//!
//! | Adapter | Feature | Use |
//! |---|---|---|
//! | [`ScriptedSource`] | always | tests, demos, benchmarks |
//! | `ImageSequenceSource` | `images` (default) | replay recorded sessions |
//! | `ScreenSource` | `screen` | live capture of the primary monitor, with its HiDPI mapping |
//! | `DxgiCapturer` | `dxgi` (Windows) | DXGI Desktop Duplication of one output, in physical pixels |
//!
//! The DXGI capturer is the only code of this crate that uses `unsafe`, confined to its
//! `#[cfg(windows)]` module (ADR-0016).

#[cfg(feature = "dxgi")]
mod dxgi;
#[cfg(feature = "images")]
mod image_sequence;
#[cfg(feature = "screen")]
mod screen;
mod scripted;
mod stamp;

#[cfg(all(windows, feature = "dxgi"))]
pub use dxgi::DxgiCapturer;
#[cfg(feature = "dxgi")]
pub use dxgi::{DxgiDiagnostics, DxgiOptions};
#[cfg(feature = "images")]
pub use image_sequence::ImageSequenceSource;
#[cfg(feature = "screen")]
pub use screen::ScreenSource;
pub use scripted::ScriptedSource;
