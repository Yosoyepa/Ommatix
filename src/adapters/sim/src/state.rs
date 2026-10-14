use ommatix_domain::{Action, Frame, Key, MouseButton, Rect};

use crate::Palette;
use crate::layout;

/// Everything the simulated form remembers.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SimState {
    pub text: String,
    pub field_focused: bool,
    pub dialog_open: bool,
    /// How many times Save was confirmed (OK pressed).
    pub saves: u32,
}

impl SimState {
    pub(crate) fn apply(&mut self, action: &Action) {
        match action {
            Action::Click {
                at,
                button: MouseButton::Left,
            } => {
                if self.dialog_open {
                    // Modal: only OK reacts, everything else is blocked.
                    if layout::OK_BUTTON.contains(*at) {
                        self.confirm();
                    }
                    return;
                }
                self.field_focused = layout::TEXT_FIELD.contains(*at);
                if layout::CANCEL_BUTTON.contains(*at) {
                    self.text.clear();
                } else if layout::SAVE_BUTTON.contains(*at) {
                    self.dialog_open = true;
                }
            }
            Action::Click { .. } => {}
            Action::TypeText { text } => {
                if self.field_focused && !self.dialog_open {
                    let room = layout::MAX_CHARS.saturating_sub(self.text.chars().count());
                    self.text
                        .extend(text.chars().filter(|c| !c.is_control()).take(room));
                }
            }
            Action::PressKey { key } => match key {
                Key::Escape if self.dialog_open => self.dialog_open = false,
                Key::Enter if self.dialog_open => self.confirm(),
                Key::Enter if self.field_focused => self.dialog_open = true,
                Key::Backspace if self.field_focused && !self.dialog_open => {
                    self.text.pop();
                }
                _ => {}
            },
        }
    }

    fn confirm(&mut self) {
        self.dialog_open = false;
        self.saves += 1;
    }

    /// Draws the desktop. Pure: the same state and palette always give the same pixels.
    pub fn render(&self, palette: &Palette) -> Frame {
        let s = layout::SCREEN;
        let mut f = Frame::solid(s.w, s.h, palette.desktop).expect("layout screen is non-empty");
        f.fill_rect(layout::WINDOW, palette.window);
        f.fill_rect(layout::TITLE_BAR, palette.title);

        let field = layout::TEXT_FIELD;
        let border = if self.field_focused {
            palette.focus
        } else {
            palette.border
        };
        f.fill_rect(field, border);
        f.fill_rect(
            Rect::new(field.x + 1, field.y + 1, field.w - 2, field.h - 2),
            palette.window,
        );
        for i in 0..self.text.chars().count() as u32 {
            f.fill_rect(
                Rect::new(
                    layout::TEXT_ORIGIN_X + i * layout::GLYPH_ADVANCE,
                    layout::TEXT_ORIGIN_Y,
                    layout::GLYPH_W,
                    layout::GLYPH_H,
                ),
                palette.ink,
            );
        }

        f.fill_rect(layout::CANCEL_BUTTON, palette.secondary);
        f.fill_rect(layout::SAVE_BUTTON, palette.primary);

        if self.dialog_open {
            let d = layout::DIALOG;
            f.fill_rect(d, palette.title);
            f.fill_rect(
                Rect::new(d.x + 2, d.y + 2, d.w - 4, d.h - 4),
                palette.window,
            );
            f.fill_rect(layout::OK_BUTTON, palette.primary);
        }
        f
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Theme;

    fn click(r: Rect) -> Action {
        Action::Click {
            at: r.center(),
            button: MouseButton::Left,
        }
    }

    #[test]
    fn form_workflow() {
        let mut s = SimState::default();
        s.apply(&Action::TypeText {
            text: "ignored".into(),
        });
        assert_eq!(s.text, "", "typing needs focus");

        s.apply(&click(layout::TEXT_FIELD));
        s.apply(&Action::TypeText {
            text: "hello".into(),
        });
        s.apply(&Action::PressKey {
            key: Key::Backspace,
        });
        assert_eq!(s.text, "hell");

        s.apply(&click(layout::SAVE_BUTTON));
        assert!(s.dialog_open);
        s.apply(&click(layout::CANCEL_BUTTON));
        assert_eq!(s.text, "hell", "dialog is modal");
        s.apply(&click(layout::OK_BUTTON));
        assert!(!s.dialog_open);
        assert_eq!(s.saves, 1);

        s.apply(&click(layout::CANCEL_BUTTON));
        assert_eq!(s.text, "");
    }

    #[test]
    fn text_is_capped_and_render_is_deterministic() {
        let mut s = SimState::default();
        s.apply(&click(layout::TEXT_FIELD));
        s.apply(&Action::TypeText {
            text: "x".repeat(100),
        });
        assert_eq!(s.text.len(), layout::MAX_CHARS);
        for theme in Theme::ALL {
            let palette = theme.palette();
            assert_eq!(s.render(&palette), s.render(&palette));
            assert_eq!(s.render(&palette).pixel(5, 5), palette.desktop);
        }
    }
}
