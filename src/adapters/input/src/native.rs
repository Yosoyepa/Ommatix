use enigo::{Button, Coordinate, Direction, Enigo, Keyboard, Mouse, Settings};
use ommatix_application::{InputController, PortError};
use ommatix_domain::{Action, DisplayMapping, Key, MouseButton};

const ADAPTER: &str = "native-input";

/// Sends real mouse and keyboard events to the operating system.
///
/// Click coordinates arrive in frame pixels and are converted with the
/// [`DisplayMapping`] of the captured monitor, so clicks land on the right spot on HiDPI
/// screens and on secondary monitors.
pub struct NativeInput {
    enigo: Enigo,
    mapping: DisplayMapping,
}

impl std::fmt::Debug for NativeInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativeInput")
            .field("mapping", &self.mapping)
            .finish_non_exhaustive()
    }
}

impl NativeInput {
    /// `mapping` must describe the monitor the frames come from
    /// (`ScreenSource::display_mapping` in the capture adapter).
    pub fn new(mapping: DisplayMapping) -> Result<Self, PortError> {
        let enigo = Enigo::new(&Settings::default()).map_err(|e| {
            PortError::new(ADAPTER, "cannot connect to the input system").with_source(e)
        })?;
        Ok(Self { enigo, mapping })
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
                let (x, y) = self.mapping.to_os(*at);
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
