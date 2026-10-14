use crate::Point;

/// Mouse button used by a click.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum MouseButton {
    #[default]
    Left,
    Right,
    Middle,
}

/// Non-character keys an agent may press. Characters are sent with [`Action::TypeText`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Key {
    Enter,
    Tab,
    Escape,
    Backspace,
    Delete,
    Space,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Home,
    End,
    PageUp,
    PageDown,
}

/// A single input the agent asks Ommatix to perform on the desktop. Coordinates are in
/// frame pixels; mapping them to the OS input space is the input adapter's job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Click { at: Point, button: MouseButton },
    TypeText { text: String },
    PressKey { key: Key },
}
