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

#[cfg(feature = "images")]
mod image_sequence;
#[cfg(feature = "screen")]
mod screen;
mod scripted;
mod stamp;

#[cfg(feature = "images")]
pub use image_sequence::ImageSequenceSource;
#[cfg(feature = "screen")]
pub use screen::ScreenSource;
pub use scripted::ScriptedSource;
