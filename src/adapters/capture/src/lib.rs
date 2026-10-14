//! # ommatix-adapter-capture
//!
//! Driven adapters for the [`FrameSource`](ommatix_application::FrameSource) port.
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

#[cfg(feature = "images")]
pub use image_sequence::ImageSequenceSource;
#[cfg(feature = "screen")]
pub use screen::ScreenSource;
pub use scripted::ScriptedSource;
