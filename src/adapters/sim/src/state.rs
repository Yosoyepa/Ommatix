use ommatix_domain::{Action, Frame, Key, MouseButton, Rect};

use crate::layout::{self, colors};

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

    pub(crate) fn render(&self) -> Frame {
        let s = layout::SCREEN;
        let mut f = Frame::solid(s.w, s.h, colors::DESKTOP).expect("layout screen is non-empty");
        f.fill_rect(layout::WINDOW, colors::WINDOW);
        f.fill_rect(layout::TITLE_BAR, colors::TITLE);

        let field = layout::TEXT_FIELD;
        let border = if self.field_focused {
            colors::FOCUS
        } else {
            colors::BORDER
        };
        f.fill_rect(field, border);
        f.fill_rect(
            Rect::new(field.x + 1, field.y + 1, field.w - 2, field.h - 2),
            colors::WINDOW,
        );
        for i in 0..self.text.chars().count() as u32 {
            f.fill_rect(
                Rect::new(
                    layout::TEXT_ORIGIN_X + i * layout::GLYPH_ADVANCE,
                    layout::TEXT_ORIGIN_Y,
                    layout::GLYPH_W,
                    layout::GLYPH_H,
                ),
                colors::INK,
            );
        }

        f.fill_rect(layout::CANCEL_BUTTON, colors::SECONDARY);
        f.fill_rect(layout::SAVE_BUTTON, colors::PRIMARY);

        if self.dialog_open {
            let d = layout::DIALOG;
            f.fill_rect(d, colors::TITLE);
            f.fill_rect(
                Rect::new(d.x + 2, d.y + 2, d.w - 4, d.h - 4),
                colors::WINDOW,
            );
            f.fill_rect(layout::OK_BUTTON, colors::PRIMARY);
        }
        f
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(s.render(), s.render());
        assert_eq!(s.render().pixel(5, 5), colors::DESKTOP);
    }
}
