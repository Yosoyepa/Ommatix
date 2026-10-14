//! # ommatix-adapter-input
//!
//! Driven adapters for the [`OsActuator`](ommatix_application::OsActuator) port. The
//! engine hands an actuator only actions that passed its gate, and only live ones when the
//! operator's policy allows them and the actuator is live.
//!
//! | Adapter | Feature | Live | Use |
//! |---|---|---|---|
//! | [`DryRunInput`] | always | no | default; records what it is handed, submits nothing |
//! | `NativeInput` | `native` | yes | real mouse/keyboard via `enigo`: move to the point, then click, type or scroll; it cannot attest the foreground application, so the engine never lets it act live |
//! | `SendInputActuator` | `sendinput` (Windows) | yes | Win32 `SendInput` in one batch per action, after checking the foreground process, its integrity level and the modifier keys |
//!
//! `SendInputActuator` is the only code of this crate that uses `unsafe`, confined to its
//! `#[cfg(windows)]` module (ADR-0016).

#[cfg(any(feature = "native", all(windows, feature = "sendinput")))]
mod aim;
mod dry_run;
#[cfg(feature = "native")]
mod native;
#[cfg(feature = "sendinput")]
mod sendinput;

pub use dry_run::{ActionLog, DryRunInput, RecordedAction};
#[cfg(feature = "native")]
pub use native::NativeInput;
#[cfg(all(windows, feature = "sendinput"))]
pub use sendinput::SendInputActuator;
