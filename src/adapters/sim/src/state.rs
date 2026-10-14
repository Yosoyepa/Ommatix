use ommatix_domain::bbox::BBox;
use ommatix_domain::fusion::{Actionability, Affordance, Control, Role, Word};
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
    /// Applies one user input to the form.
    pub fn apply(&mut self, action: &Action) {
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

    /// The controls on screen, as a perfect detector would report them: the field and the
    /// two buttons (disabled while the modal dialog is open), and the dialog's OK button.
    pub fn controls(&self) -> Vec<Control> {
        let control = |rect: Rect, role, enabled| Control {
            bbox: to_bbox(rect),
            role,
            affordance: Affordance {
                actionability: Actionability::Confirmed,
                enabled,
                visible: true,
            },
        };
        let form_enabled = !self.dialog_open;
        let mut controls = vec![
            control(layout::TEXT_FIELD, Role::Input, form_enabled),
            control(layout::CANCEL_BUTTON, Role::Button, form_enabled),
            control(layout::SAVE_BUTTON, Role::Button, form_enabled),
        ];
        if self.dialog_open {
            controls.push(control(layout::OK_BUTTON, Role::Button, true));
        }
        controls
    }

    /// The words on screen, as a perfect OCR would report them: the field's label, the
    /// button captions, the typed text (one word per run of non-blank characters) and,
    /// while it is open, the dialog's prompt and OK caption.
    pub fn words(&self) -> Vec<Word> {
        let word = |text: &str, rect: Rect| Word::new(text, to_bbox(rect));
        let mut words = vec![
            word("Name", layout::FIELD_LABEL),
            word("Cancel", layout::CANCEL_CAPTION),
            word("Save", layout::SAVE_CAPTION),
        ];
        let chars: Vec<char> = self.text.chars().collect();
        let mut start = None;
        for i in 0..=chars.len() {
            let blank = chars.get(i).is_none_or(|c| c.is_whitespace());
            match (start, blank) {
                (None, false) => start = Some(i),
                (Some(a), true) => {
                    let text: String = chars[a..i].iter().collect();
                    words.push(word(&text, layout::glyph_run(a as u32, i as u32)));
                    start = None;
                }
                _ => {}
            }
        }
        if self.dialog_open {
            words.push(word("Confirm", layout::DIALOG_PROMPT));
            words.push(word("OK", layout::OK_CAPTION));
        }
        words
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

fn to_bbox(r: Rect) -> BBox {
    BBox::new(
        f64::from(r.x),
        f64::from(r.y),
        f64::from(r.right()),
        f64::from(r.bottom()),
    )
    .expect("layout rectangles are non-empty")
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

#[cfg(test)]
mod truth_tests {
    use super::*;

    #[test]
    fn typed_text_is_one_word_per_run() {
        let s = SimState {
            text: "ab  c".into(),
            ..SimState::default()
        };
        let words = s.words();
        let typed: Vec<(&str, f64, f64)> = words[3..]
            .iter()
            .map(|w| (w.text.as_str(), w.bbox.x0(), w.bbox.x1()))
            .collect();
        let x = f64::from(layout::TEXT_ORIGIN_X);
        assert_eq!(typed, vec![("ab", x, x + 15.0), ("c", x + 36.0, x + 42.0)]);
    }

    #[test]
    fn the_modal_dialog_disables_the_form() {
        let open = SimState {
            dialog_open: true,
            ..SimState::default()
        };
        let controls = open.controls();
        assert_eq!(controls.len(), 4);
        assert!(controls[..3].iter().all(|c| !c.affordance.enabled));
        assert!(controls[3].affordance.enabled);
        assert!(
            SimState::default()
                .controls()
                .iter()
                .all(|c| c.affordance.enabled)
        );
    }
}
