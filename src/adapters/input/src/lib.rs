//! # ommatix-adapter-input
//!
//! Driven adapters for the [`OsActuator`](ommatix_application::OsActuator) port. The
//! engine hands an actuator only actions that passed its gate, and only live ones when the
//! operator's policy allows them and the actuator is live.
//!
//! | Adapter | Feature | Live | Use |
//! |---|---|---|---|
//! | [`DryRunInput`] | always | no | default; records what it is handed, submits nothing |
//! | `NativeInput` | `native` | yes | real mouse/keyboard via `enigo`: move to the point, then click, type or scroll |

mod dry_run;
#[cfg(feature = "native")]
mod native;

pub use dry_run::{ActionLog, DryRunInput, RecordedAction};
#[cfg(feature = "native")]
pub use native::NativeInput;
