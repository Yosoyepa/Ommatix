//! # ommatix-adapter-input
//!
//! Driven adapters for the [`InputController`](ommatix_application::InputController) port.
//!
//! | Adapter | Feature | Live | Use |
//! |---|---|---|---|
//! | [`DryRunInput`] | always | no | default for `serve`; records and logs actions |
//! | `NativeInput` | `native` | yes | real mouse/keyboard via `enigo` (bootstrap ADR B0008) |

mod dry_run;
#[cfg(feature = "native")]
mod native;

pub use dry_run::{ActionLog, DryRunInput};
#[cfg(feature = "native")]
pub use native::NativeInput;
