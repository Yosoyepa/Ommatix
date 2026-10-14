//! # ommatix-adapter-sim
//!
//! A deterministic, in-memory "desktop" with one form window. It implements **both**
//! driven ports over shared state, so an agent's clicks and keystrokes really change the
//! next frame. This closes the perceive → act → perceive loop without a real screen,
//! which is what the Gherkin acceptance suite and `ommatix serve --source sim` use.
//!
//! The form: a text field, a **Cancel** button (clears the field) and a **Save** button
//! (opens a confirmation dialog with an **OK** button). See [`layout`].

pub mod layout;
mod state;

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use ommatix_application::{FrameSource, InputController, PortError};
use ommatix_domain::{Action, Frame};

pub use state::SimState;

/// Owner of the shared simulator state. Hand out its two port views to the engine.
#[derive(Debug, Clone, Default)]
pub struct SimulatedDesktop {
    state: Arc<Mutex<SimState>>,
}

impl SimulatedDesktop {
    pub fn new() -> Self {
        Self::default()
    }

    /// The [`FrameSource`] view: renders the current state on every call.
    pub fn screen(&self) -> SimScreen {
        SimScreen(self.clone())
    }

    /// The [`InputController`] view: applies actions to the state.
    pub fn input(&self) -> SimInput {
        SimInput(self.clone())
    }

    /// Snapshot of the simulator state, for assertions.
    pub fn state(&self) -> SimState {
        self.lock().clone()
    }

    fn lock(&self) -> MutexGuard<'_, SimState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// [`FrameSource`] view of a [`SimulatedDesktop`]. Never exhausts.
#[derive(Debug, Clone)]
pub struct SimScreen(SimulatedDesktop);

impl FrameSource for SimScreen {
    fn name(&self) -> &'static str {
        "simulated-desktop"
    }

    fn next_frame(&mut self) -> Result<Option<Frame>, PortError> {
        Ok(Some(self.0.lock().render()))
    }
}

/// [`InputController`] view of a [`SimulatedDesktop`]. Not live: nothing leaves the process.
#[derive(Debug, Clone)]
pub struct SimInput(SimulatedDesktop);

impl InputController for SimInput {
    fn name(&self) -> &'static str {
        "simulated-desktop"
    }

    fn is_live(&self) -> bool {
        false
    }

    fn perform(&mut self, action: &Action) -> Result<(), PortError> {
        self.0.lock().apply(action);
        Ok(())
    }
}
