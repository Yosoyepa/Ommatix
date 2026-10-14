//! # ommatix-adapter-sim
//!
//! Deterministic, in-memory desktops that implement the SDD's driven ports
//! ([`FrameCapturer`], [`VisionDetector`], [`TextRecognizer`], [`OsActuator`]) over shared
//! state, so the engine can be run end to end without a real screen or models.
//!
//! - [`SimulatedDesktop`]: one form window with a text field labelled "Name", a
//!   **Cancel** button (clears the field) and a **Save** button (opens a confirmation
//!   dialog with an **OK** button). See [`layout`] and [`Theme`]. The detector and the
//!   recogniser report the form's ground truth; the test (or demo) plays the user with
//!   [`SimulatedDesktop::perform`].
//! - [`FixtureDesktop`]: a generic scene of controls and words that a test edits between
//!   captures ([`fixture`]).
//!
//! Both are simulations: their actuators are never live, record what they are handed and
//! never claim that input reached the OS. Captures are BGRA with an increasing
//! `frame_seq`, stamped by the injected [`Clock`].

pub mod fixture;
pub mod layout;
mod state;
mod theme;

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use ommatix_application::contract::ValidatedAction;
use ommatix_application::{
    CapturedFrame, Clock, DeliveryReceipt, FrameCapturer, ManualClock, OsActuator, PortError,
    TextRecognizer, VisionDetector,
};
use ommatix_domain::bbox::BBox;
use ommatix_domain::fusion::{Control, Word};
use ommatix_domain::incremental::BgraFrame;
use ommatix_domain::{Action, Frame};

pub use fixture::{FixtureDesktop, FixtureScene, confirmed};
pub use state::SimState;
pub use theme::{Palette, Theme};

const NAME: &str = "simulated-desktop";

/// The geometry epoch of every simulated capture: the simulated screen never changes size.
pub const SIM_GEOMETRY_EPOCH: u64 = 1;

/// Converts an RGBA prototype frame into a BGRA frame at `geometry_epoch`.
pub fn to_bgra(frame: &Frame, geometry_epoch: u64) -> BgraFrame {
    let mut px = frame.as_rgba().to_vec();
    for p in px.as_chunks_mut::<4>().0 {
        p.swap(0, 2);
    }
    BgraFrame::new(
        frame.width(),
        frame.height(),
        4 * frame.width() as usize,
        geometry_epoch,
        px,
    )
    .expect("a valid RGBA frame is a valid BGRA frame of the same size")
}

#[derive(Debug, Default)]
struct Shared {
    state: SimState,
    frame_seq: u64,
    handed: Vec<ValidatedAction>,
}

/// Owner of the shared simulator state. Hand out its port views to the engine.
#[derive(Clone)]
pub struct SimulatedDesktop {
    shared: Arc<Mutex<Shared>>,
    theme: Theme,
    clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for SimulatedDesktop {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SimulatedDesktop")
            .field("shared", &self.shared)
            .field("theme", &self.theme)
            .finish_non_exhaustive()
    }
}

impl Default for SimulatedDesktop {
    fn default() -> Self {
        Self::new()
    }
}

impl SimulatedDesktop {
    /// The light theme on a virtual clock that never advances.
    pub fn new() -> Self {
        Self::with_theme(Theme::default())
    }

    pub fn with_theme(theme: Theme) -> Self {
        Self {
            shared: Arc::default(),
            theme,
            clock: Arc::new(ManualClock::new()),
        }
    }

    /// Stamps captures with `clock`; share the engine's clock so `age_ms` is meaningful.
    #[must_use]
    pub fn with_clock(mut self, clock: Arc<dyn Clock>) -> Self {
        self.clock = clock;
        self
    }

    pub fn theme(&self) -> Theme {
        self.theme
    }

    /// Snapshot of the simulator state, for assertions.
    pub fn state(&self) -> SimState {
        self.lock().state.clone()
    }

    /// Plays the user: applies a prototype action (click at a pixel, typed text, a key)
    /// to the form. This is how a test changes the screen between observations; the
    /// engine's actuator view never does, because it is not live.
    pub fn perform(&self, action: &Action) {
        self.lock().state.apply(action);
    }

    /// Every action the actuator view was handed, in order.
    pub fn handed_actions(&self) -> Vec<ValidatedAction> {
        self.lock().handed.clone()
    }

    /// The form's controls as a perfect detector would report them.
    pub fn ground_truth_controls(&self) -> Vec<Control> {
        self.lock().state.controls()
    }

    /// The form's words as a perfect OCR would report them.
    pub fn ground_truth_words(&self) -> Vec<Word> {
        self.lock().state.words()
    }

    /// The current screen as a BGRA frame.
    pub fn render(&self) -> BgraFrame {
        to_bgra(
            &self.lock().state.render(&self.theme.palette()),
            SIM_GEOMETRY_EPOCH,
        )
    }

    pub fn capturer(&self) -> SimCapturer {
        SimCapturer(self.clone())
    }

    pub fn detector(&self) -> SimDetector {
        SimDetector(self.clone())
    }

    pub fn recognizer(&self) -> SimRecognizer {
        SimRecognizer(self.clone())
    }

    pub fn actuator(&self) -> SimActuator {
        SimActuator(self.clone())
    }

    fn lock(&self) -> MutexGuard<'_, Shared> {
        self.shared.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

fn overlapping<T: Clone>(items: Vec<T>, bbox: impl Fn(&T) -> BBox, regions: &[BBox]) -> Vec<T> {
    items
        .into_iter()
        .filter(|item| regions.iter().any(|r| r.overlaps(&bbox(item))))
        .collect()
}

/// [`FrameCapturer`] view of a [`SimulatedDesktop`]: renders the current state.
#[derive(Debug, Clone)]
pub struct SimCapturer(SimulatedDesktop);

impl FrameCapturer for SimCapturer {
    fn name(&self) -> &'static str {
        NAME
    }

    fn capture(&mut self) -> Result<CapturedFrame, PortError> {
        let captured_at = self.0.clock.now();
        let palette = self.0.theme.palette();
        let mut shared = self.0.lock();
        shared.frame_seq += 1;
        Ok(CapturedFrame {
            frame: to_bgra(&shared.state.render(&palette), SIM_GEOMETRY_EPOCH),
            frame_seq: shared.frame_seq,
            captured_at,
        })
    }
}

/// [`VisionDetector`] view: the ground-truth controls whose box overlaps the regions,
/// as buttons and inputs with a confirmed affordance. No model is loaded.
#[derive(Debug, Clone)]
pub struct SimDetector(SimulatedDesktop);

impl VisionDetector for SimDetector {
    fn name(&self) -> &'static str {
        NAME
    }

    fn models_loaded(&self) -> bool {
        false
    }

    fn detect(&mut self, _: &BgraFrame, regions: &[BBox]) -> Result<Vec<Control>, PortError> {
        Ok(overlapping(
            self.0.ground_truth_controls(),
            |c| c.bbox,
            regions,
        ))
    }
}

/// [`TextRecognizer`] view: the ground-truth words whose box overlaps the regions.
#[derive(Debug, Clone)]
pub struct SimRecognizer(SimulatedDesktop);

impl TextRecognizer for SimRecognizer {
    fn name(&self) -> &'static str {
        NAME
    }

    fn recognize(&mut self, _: &BgraFrame, regions: &[BBox]) -> Result<Vec<Word>, PortError> {
        Ok(overlapping(
            self.0.ground_truth_words(),
            |w| w.bbox,
            regions,
        ))
    }
}

/// [`OsActuator`] view: not live. It records what it is handed, changes nothing and
/// reports that nothing was submitted to the OS.
#[derive(Debug, Clone)]
pub struct SimActuator(SimulatedDesktop);

impl OsActuator for SimActuator {
    fn name(&self) -> &'static str {
        NAME
    }

    fn is_live(&self) -> bool {
        false
    }

    fn deliver(&mut self, action: &ValidatedAction) -> Result<DeliveryReceipt, PortError> {
        self.0.lock().handed.push(action.clone());
        Ok(DeliveryReceipt {
            submitted: 0,
            inserted: 0,
        })
    }
}

/// A recorded session of the simulated desktop, as RGBA frames for replay (`ommatix demo`,
/// the capture adapter's `ScriptedSource`) and tests:
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
    use ommatix_domain::fusion::Role;
    use ommatix_domain::{Key, MouseButton};

    use super::*;

    #[test]
    fn the_views_share_one_desktop() {
        let desktop = SimulatedDesktop::with_theme(Theme::Dark);
        let mut capturer = desktop.capturer();
        let before = capturer.capture().unwrap();
        assert_eq!(before.frame_seq, 1);
        assert_eq!(before.frame.geometry_epoch(), SIM_GEOMETRY_EPOCH);
        desktop.perform(&Action::Click {
            at: layout::SAVE_BUTTON.center(),
            button: MouseButton::Left,
        });
        assert!(desktop.state().dialog_open);
        let open = capturer.capture().unwrap();
        assert_eq!(open.frame_seq, 2);
        assert_ne!(open.frame, before.frame);
        desktop.perform(&Action::PressKey { key: Key::Escape });
        assert_eq!(capturer.capture().unwrap().frame, before.frame);
    }

    #[test]
    fn frames_are_bgra() {
        let desktop = SimulatedDesktop::new();
        let frame = desktop.render();
        let save = layout::SAVE_BUTTON.center();
        let i = save.y as usize * frame.row_stride() + 4 * save.x as usize;
        let [r, g, b, a] = Theme::Light.palette().primary;
        assert_eq!(&frame.pixels()[i..i + 4], &[b, g, r, a]);
    }

    #[test]
    fn the_detector_reports_ground_truth_inside_the_regions_only() {
        let desktop = SimulatedDesktop::new();
        let frame = desktop.render();
        let mut detector = desktop.detector();
        let all = detector.detect(&frame, &[frame.bounds()]).unwrap();
        let roles: Vec<Role> = all.iter().map(|c| c.role).collect();
        assert_eq!(roles, vec![Role::Input, Role::Button, Role::Button]);
        assert!(!detector.models_loaded());

        let bottom_right = BBox::new(390.0, 290.0, 640.0, 400.0).unwrap();
        assert_eq!(detector.detect(&frame, &[bottom_right]).unwrap().len(), 2);
        let words = desktop
            .recognizer()
            .recognize(&frame, &[bottom_right])
            .unwrap();
        let texts: Vec<&str> = words.iter().map(|w| w.text.as_str()).collect();
        assert_eq!(texts, vec!["Cancel", "Save"]);
    }

    #[test]
    fn the_actuator_is_not_live_and_never_claims_delivery() {
        let desktop = SimulatedDesktop::new();
        let actuator = desktop.actuator();
        assert!(!actuator.is_live());
        assert!(desktop.handed_actions().is_empty());
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
