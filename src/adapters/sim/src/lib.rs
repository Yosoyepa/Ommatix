//! # ommatix-adapter-sim
//!
//! A deterministic, in-memory "desktop" with one form window. It implements **both**
//! driven ports over shared state, so an agent's clicks and keystrokes really change the
//! next frame. This closes the perceive → act → perceive loop without a real screen,
//! which is what the Gherkin acceptance suite and `ommatix serve --source sim` use.
//!
//! The form: a text field, a **Cancel** button (clears the field) and a **Save** button
//! (opens a confirmation dialog with an **OK** button). See [`layout`] and [`Theme`].

pub mod layout;
mod state;
mod theme;

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use ommatix_application::{FrameSource, InputController, PortError};
use ommatix_domain::{Action, Frame};

pub use state::SimState;
pub use theme::{Palette, Theme};

/// Owner of the shared simulator state. Hand out its two port views to the engine.
#[derive(Debug, Clone, Default)]
pub struct SimulatedDesktop {
    state: Arc<Mutex<SimState>>,
    theme: Theme,
}

impl SimulatedDesktop {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_theme(theme: Theme) -> Self {
        Self {
            theme,
            ..Self::default()
        }
    }

    pub fn theme(&self) -> Theme {
        self.theme
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
        let palette = self.0.theme.palette();
        Ok(Some(self.0.lock().render(&palette)))
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

/// A recorded session of the simulated desktop, for replay (`ommatix demo`) and tests:
///
/// 1. The form appears.
/// 2. Nothing changes (a static screen must cost ~nothing).
/// 3. "hello" is typed into the text field.
/// 4. The confirmation dialog opens.
pub fn scripted_session(theme: Theme) -> Vec<Frame> {
    let palette = theme.palette();
    let form = SimState::default();
    let typed = SimState {
        text: "hello".into(),
        ..SimState::default()
    };
    let dialog = SimState {
        dialog_open: true,
        ..typed.clone()
    };
    vec![
        form.render(&palette),
        form.render(&palette),
        typed.render(&palette),
        dialog.render(&palette),
    ]
}

#[cfg(test)]
mod tests {
    use ommatix_domain::{Key, MouseButton};

    use super::*;

    #[test]
    fn both_views_share_one_desktop() {
        let desktop = SimulatedDesktop::with_theme(Theme::Dark);
        let (mut screen, mut input) = (desktop.screen(), desktop.input());
        let before = screen.next_frame().unwrap().unwrap();
        input
            .perform(&Action::Click {
                at: layout::SAVE_BUTTON.center(),
                button: MouseButton::Left,
            })
            .unwrap();
        assert!(desktop.state().dialog_open);
        assert_ne!(screen.next_frame().unwrap().unwrap(), before);
        input
            .perform(&Action::PressKey { key: Key::Escape })
            .unwrap();
        assert_eq!(screen.next_frame().unwrap().unwrap(), before);
    }

    #[test]
    fn the_scripted_session_shows_each_step() {
        let frames = scripted_session(Theme::Light);
        assert_eq!(frames.len(), 4);
        assert_eq!(frames[0], frames[1]);
        assert_ne!(frames[1], frames[2]);
        let ok = layout::OK_BUTTON.center();
        assert_eq!(frames[3].pixel(ok.x, ok.y), Theme::Light.palette().primary);
    }
}
