use enigo::{Button, Coordinate, Direction, Enigo, Keyboard, Mouse, Settings};
use ommatix_application::{InputController, PortError};
use ommatix_domain::{Action, Key, MouseButton};

const ADAPTER: &str = "native-input";

/// Sends real mouse and keyboard events to the operating system.
pub struct NativeInput {
    enigo: Enigo,
}

impl std::fmt::Debug for NativeInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativeInput").finish_non_exhaustive()
    }
}

impl NativeInput {
    pub fn new() -> Result<Self, PortError> {
        let enigo = Enigo::new(&Settings::default()).map_err(|e| {
            PortError::new(ADAPTER, "cannot connect to the input system").with_source(e)
        })?;
        Ok(Self { enigo })
    }
}

impl InputController for NativeInput {
    fn name(&self) -> &'static str {
        ADAPTER
    }

    fn is_live(&self) -> bool {
        true
    }

    fn perform(&mut self, action: &Action) -> Result<(), PortError> {
        let fail = |e: enigo::InputError| PortError::new(ADAPTER, "input failed").with_source(e);
        match action {
            Action::Click { at, button } => {
                let x =
                    i32::try_from(at.x).map_err(|_| PortError::new(ADAPTER, "x out of range"))?;
                let y =
                    i32::try_from(at.y).map_err(|_| PortError::new(ADAPTER, "y out of range"))?;
                self.enigo.move_mouse(x, y, Coordinate::Abs).map_err(fail)?;
                self.enigo
                    .button(to_button(*button), Direction::Click)
                    .map_err(fail)?;
            }
            Action::TypeText { text } => self.enigo.text(text).map_err(fail)?,
            Action::PressKey { key } => self
                .enigo
                .key(to_key(*key), Direction::Click)
                .map_err(fail)?,
        }
        Ok(())
    }
}

fn to_button(b: MouseButton) -> Button {
    match b {
        MouseButton::Left => Button::Left,
        MouseButton::Right => Button::Right,
        MouseButton::Middle => Button::Middle,
    }
}

fn to_key(k: Key) -> enigo::Key {
    match k {
        Key::Enter => enigo::Key::Return,
        Key::Tab => enigo::Key::Tab,
        Key::Escape => enigo::Key::Escape,
        Key::Backspace => enigo::Key::Backspace,
        Key::Delete => enigo::Key::Delete,
        Key::Space => enigo::Key::Space,
        Key::ArrowUp => enigo::Key::UpArrow,
        Key::ArrowDown => enigo::Key::DownArrow,
        Key::ArrowLeft => enigo::Key::LeftArrow,
        Key::ArrowRight => enigo::Key::RightArrow,
        Key::Home => enigo::Key::Home,
        Key::End => enigo::Key::End,
        Key::PageUp => enigo::Key::PageUp,
        Key::PageDown => enigo::Key::PageDown,
    }
}
