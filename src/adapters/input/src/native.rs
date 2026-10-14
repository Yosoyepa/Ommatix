use enigo::{Axis, Button, Coordinate, Direction, Enigo, Keyboard, Mouse, Settings};
use ommatix_application::contract::{ActionKind, ScrollDirection, ValidatedAction};
use ommatix_application::{DeliveryReceipt, OsActuator, PortError};
use ommatix_domain::DisplayMapping;

use crate::aim::frame_pixel;

const ADAPTER: &str = "native-input";

type MappingSource = Box<dyn Fn() -> Result<DisplayMapping, PortError> + Send + Sync>;

/// One OS call and the number of input events it injects.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Step {
    MoveTo(i32, i32),
    Click,
    Text(String),
    Scroll(i32),
}

impl Step {
    /// Events the step injects: a move is one, a click a press and a release, each
    /// character a press and a release, each scroll notch one.
    fn events(&self) -> u32 {
        match self {
            Self::MoveTo(..) => 1,
            Self::Click => 2,
            Self::Text(t) => u32::try_from(t.chars().count())
                .unwrap_or(u32::MAX)
                .saturating_mul(2),
            Self::Scroll(n) => n.unsigned_abs(),
        }
    }
}

/// The OS calls for `kind` at OS coordinates `at`: move there, then click, type (after a
/// click that gives the validated field focus) or scroll.
fn steps(kind: &ActionKind, at: (i32, i32)) -> Vec<Step> {
    let mut steps = vec![Step::MoveTo(at.0, at.1)];
    match kind {
        ActionKind::Click => steps.push(Step::Click),
        ActionKind::Type { text } => {
            steps.push(Step::Click);
            steps.push(Step::Text(text.clone()));
        }
        ActionKind::Scroll {
            direction,
            steps: n,
        } => {
            let n = i32::from(*n);
            steps.push(Step::Scroll(match direction {
                ScrollDirection::Down => n,
                ScrollDirection::Up => -n,
            }));
        }
    }
    steps
}

/// Sends real mouse and keyboard events to the operating system through `enigo`.
///
/// Action points arrive in frame pixels and are converted with the [`DisplayMapping`] of
/// the captured monitor, so input lands on the right spot on HiDPI screens and on
/// secondary monitors. The receipt counts the events submitted and those the OS took;
/// a failing call stops the action there and the receipt says so. Nothing is retried.
pub struct NativeInput {
    enigo: Enigo,
    mapping: MappingSource,
}

impl std::fmt::Debug for NativeInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativeInput").finish_non_exhaustive()
    }
}

impl NativeInput {
    /// `mapping` must describe the monitor the frames come from.
    pub fn new(mapping: DisplayMapping) -> Result<Self, PortError> {
        Self::with_mapping_source(move || Ok(mapping))
    }

    /// Reads the mapping before every action (`ScreenSource::mapping_source` in the
    /// capture adapter), so a mapping invalidated by a screen change fails closed.
    pub fn with_mapping_source(
        mapping: impl Fn() -> Result<DisplayMapping, PortError> + Send + Sync + 'static,
    ) -> Result<Self, PortError> {
        let enigo = Enigo::new(&Settings::default()).map_err(|e| {
            PortError::new(ADAPTER, "cannot connect to the input system").with_source(e)
        })?;
        Ok(Self {
            enigo,
            mapping: Box::new(mapping),
        })
    }

    fn run(&mut self, step: &Step) -> Result<(), enigo::InputError> {
        match step {
            Step::MoveTo(x, y) => self.enigo.move_mouse(*x, *y, Coordinate::Abs),
            Step::Click => self.enigo.button(Button::Left, Direction::Click),
            Step::Text(text) => self.enigo.text(text),
            Step::Scroll(n) => self.enigo.scroll(*n, Axis::Vertical),
        }
    }
}

impl OsActuator for NativeInput {
    fn name(&self) -> &'static str {
        ADAPTER
    }

    fn is_live(&self) -> bool {
        true
    }

    fn deliver(&mut self, action: &ValidatedAction) -> Result<DeliveryReceipt, PortError> {
        let mapping = (self.mapping)()?;
        let pixel = frame_pixel(action.point())
            .ok_or_else(|| PortError::new(ADAPTER, "the action point is not a frame pixel"))?;
        let plan = steps(action.kind(), mapping.to_os(pixel));
        let submitted = plan.iter().map(Step::events).fold(0, u32::saturating_add);
        let mut inserted = 0u32;
        for step in &plan {
            if self.run(step).is_err() {
                break;
            }
            inserted = inserted.saturating_add(step.events());
        }
        Ok(DeliveryReceipt {
            submitted,
            inserted,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_action_moves_first_and_counts_its_events() {
        let click = steps(&ActionKind::Click, (5, 6));
        assert_eq!(click, vec![Step::MoveTo(5, 6), Step::Click]);
        let typed = steps(
            &ActionKind::Type {
                text: "héllo".into(),
            },
            (0, 0),
        );
        assert_eq!(typed.iter().map(Step::events).sum::<u32>(), 1 + 2 + 10);
        let up = steps(
            &ActionKind::Scroll {
                direction: ScrollDirection::Up,
                steps: 3,
            },
            (0, 0),
        );
        assert_eq!(up[1], Step::Scroll(-3));
        assert_eq!(up[1].events(), 3);
    }

    #[test]
    fn the_mapping_converts_frame_pixels_to_os_coordinates() {
        let retina = DisplayMapping::new((0, 0), (2880, 1800), (1440, 900)).unwrap();
        let pixel = frame_pixel((1080.0, 632.0)).unwrap();
        assert_eq!(
            steps(&ActionKind::Click, retina.to_os(pixel))[0],
            Step::MoveTo(540, 316)
        );
    }
}
