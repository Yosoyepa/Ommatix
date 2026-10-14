use serde::{Deserialize, Serialize};

use crate::Point;

/// Mouse button used by a click.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MouseButton {
    #[default]
    Left,
    Right,
    Middle,
}

/// Non-character keys an agent may press. Characters are sent with [`Action::TypeText`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
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

/// A single input the agent asks Ommatix to perform on the desktop.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Action {
    Click { at: Point, button: MouseButton },
    TypeText { text: String },
    PressKey { key: Key },
}
