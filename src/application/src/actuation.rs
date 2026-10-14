use ommatix_domain::{Action, ElementId, Key, MouseButton, Point, Rect};
use thiserror::Error;

use crate::{InputController, PortError, ScreenModel};

/// Guard rails applied before any input reaches an [`InputController`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActuationPolicy {
    /// Longest text accepted by a single `type_text`.
    pub max_text_chars: usize,
}

impl Default for ActuationPolicy {
    fn default() -> Self {
        Self {
            max_text_chars: 1_000,
        }
    }
}

/// Where to click: a raw coordinate, or the centre of a perceived element.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClickTarget {
    Point(Point),
    Element(ElementId),
}

/// What was actually sent to the input controller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionReceipt {
    pub action: Action,
    /// The element the click was resolved from, if any.
    pub element: Option<ElementId>,
    /// `false` when the controller is a dry-run or simulator.
    pub live: bool,
}

#[derive(Debug, Error)]
pub enum ActuationError {
    #[error("nothing has been perceived yet; call perceive first")]
    NoFrameYet,
    #[error("point ({}, {}) is outside the screen {}x{}", point.x, point.y, screen.w, screen.h)]
    OutOfBounds { point: Point, screen: Rect },
    #[error("unknown element; it may have disappeared, call perceive again")]
    UnknownElement(ElementId),
    #[error("text must not be empty")]
    EmptyText,
    #[error("text has {len} characters, the limit is {max}")]
    TextTooLong { len: usize, max: usize },
    #[error(
        "text contains a control character at position {position}; \
         use press_key for Enter, Tab, Backspace and other keys"
    )]
    ControlCharacter { position: usize },
    #[error("input controller failed")]
    Input(#[source] PortError),
}

/// The actuation use case: validate an agent's request against what is on screen, then
/// hand it to the injected [`InputController`].
pub struct ActuationService {
    input: Box<dyn InputController>,
    policy: ActuationPolicy,
}

impl std::fmt::Debug for ActuationService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ActuationService")
            .field("input", &self.input.name())
            .field("live", &self.input.is_live())
            .field("policy", &self.policy)
            .finish()
    }
}

impl ActuationService {
    pub fn new(input: Box<dyn InputController>, policy: ActuationPolicy) -> Self {
        Self { input, policy }
    }

    pub fn input_name(&self) -> &'static str {
        self.input.name()
    }

    pub fn is_live(&self) -> bool {
        self.input.is_live()
    }

    pub fn click(
        &mut self,
        screen: &dyn ScreenModel,
        target: ClickTarget,
        button: MouseButton,
    ) -> Result<ActionReceipt, ActuationError> {
        let bounds = screen.screen_bounds().ok_or(ActuationError::NoFrameYet)?;
        let (at, element) = match target {
            ClickTarget::Point(p) => (p, None),
            ClickTarget::Element(id) => {
                let e = screen
                    .element(id)
                    .ok_or(ActuationError::UnknownElement(id))?;
                (e.bounds.center(), Some(id))
            }
        };
        if !bounds.contains(at) {
            return Err(ActuationError::OutOfBounds {
                point: at,
                screen: bounds,
            });
        }
        self.perform(Action::Click { at, button }, element)
    }

    /// Types printable text. Control characters (newline, tab, escape...) are rejected:
    /// their effect depends on the focused control, so agents send them as keys.
    pub fn type_text(&mut self, text: &str) -> Result<ActionReceipt, ActuationError> {
        let len = text.chars().count();
        if len == 0 {
            return Err(ActuationError::EmptyText);
        }
        if len > self.policy.max_text_chars {
            return Err(ActuationError::TextTooLong {
                len,
                max: self.policy.max_text_chars,
            });
        }
        if let Some(position) = text.chars().position(char::is_control) {
            return Err(ActuationError::ControlCharacter { position });
        }
        self.perform(
            Action::TypeText {
                text: text.to_owned(),
            },
            None,
        )
    }

    pub fn press_key(&mut self, key: Key) -> Result<ActionReceipt, ActuationError> {
        self.perform(Action::PressKey { key }, None)
    }

    fn perform(
        &mut self,
        action: Action,
        element: Option<ElementId>,
    ) -> Result<ActionReceipt, ActuationError> {
        self.input.perform(&action).map_err(ActuationError::Input)?;
        Ok(ActionReceipt {
            action,
            element,
            live: self.input.is_live(),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use ommatix_domain::{Element, ElementKind};

    use super::*;

    #[derive(Default, Clone)]
    struct Recorder(Arc<Mutex<Vec<Action>>>);

    impl InputController for Recorder {
        fn name(&self) -> &'static str {
            "recorder"
        }
        fn is_live(&self) -> bool {
            false
        }
        fn perform(&mut self, action: &Action) -> Result<(), PortError> {
            self.0.lock().unwrap().push(action.clone());
            Ok(())
        }
    }

    struct FakeScreen(Option<Rect>, Vec<Element>);

    impl ScreenModel for FakeScreen {
        fn screen_bounds(&self) -> Option<Rect> {
            self.0
        }
        fn element(&self, id: ElementId) -> Option<Element> {
            self.1.iter().find(|e| e.id == id).copied()
        }
    }

    fn button() -> Element {
        Element {
            id: ElementId(7),
            bounds: Rect::new(100, 100, 40, 20),
            kind: ElementKind::Blob,
            first_seen: 1,
            last_changed: 1,
        }
    }

    fn service() -> (ActuationService, Recorder) {
        let rec = Recorder::default();
        let svc = ActuationService::new(Box::new(rec.clone()), ActuationPolicy::default());
        (svc, rec)
    }

    #[test]
    fn click_on_element_targets_its_center() {
        let (mut svc, rec) = service();
        let screen = FakeScreen(Some(Rect::new(0, 0, 640, 400)), vec![button()]);
        let r = svc
            .click(
                &screen,
                ClickTarget::Element(ElementId(7)),
                MouseButton::Left,
            )
            .unwrap();
        assert_eq!(r.element, Some(ElementId(7)));
        assert!(!r.live);
        assert_eq!(
            rec.0.lock().unwrap().as_slice(),
            [Action::Click {
                at: Point::new(120, 110),
                button: MouseButton::Left
            }]
        );
    }

    #[test]
    fn invalid_requests_never_reach_the_controller() {
        let (mut svc, rec) = service();
        let screen = FakeScreen(Some(Rect::new(0, 0, 640, 400)), vec![]);
        assert!(matches!(
            svc.click(
                &FakeScreen(None, vec![]),
                ClickTarget::Point(Point::new(1, 1)),
                MouseButton::Left
            ),
            Err(ActuationError::NoFrameYet)
        ));
        assert!(matches!(
            svc.click(
                &screen,
                ClickTarget::Point(Point::new(640, 10)),
                MouseButton::Left
            ),
            Err(ActuationError::OutOfBounds { .. })
        ));
        assert!(matches!(
            svc.click(
                &screen,
                ClickTarget::Element(ElementId(99)),
                MouseButton::Left
            ),
            Err(ActuationError::UnknownElement(_))
        ));
        assert!(matches!(svc.type_text(""), Err(ActuationError::EmptyText)));
        assert!(matches!(
            svc.type_text(&"x".repeat(1_001)),
            Err(ActuationError::TextTooLong { len: 1_001, .. })
        ));
        assert!(matches!(
            svc.type_text("ok\nrm -rf"),
            Err(ActuationError::ControlCharacter { position: 2 })
        ));
        assert_eq!(rec.0.lock().unwrap().len(), 0);
    }
}
