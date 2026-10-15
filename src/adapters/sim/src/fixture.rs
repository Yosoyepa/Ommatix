//! A generic, deterministic desktop for acceptance tests: the test places controls and
//! words, and every capture renders them.
//!
//! Each item gets a stable ID when it is added (never reused) and is painted as a filled
//! rectangle whose colour derives from that ID, controls first, then words, on a black
//! background. So a capture differs from the previous one exactly where an item was
//! added, moved, replaced or removed, and the engine's dirty tiles land there. Changes
//! that do not touch pixels (a new role or affordance on the same box) are invisible, as
//! they would be on a real screen; [`FixtureDesktop::replace_control`] gives the item a
//! new ID (and colour) to make the change visible.
//!
//! The four port views share the scene: [`FixtureDesktop::capturer`] renders it,
//! [`FixtureDesktop::detector`] and [`FixtureDesktop::recognizer`] report the ground
//! truth that intersects the requested regions, and [`FixtureDesktop::actuator`] records
//! what it is handed. Nothing is ever delivered to the OS.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use ommatix_application::contract::ValidatedAction;
use ommatix_application::{
    CapturedFrame, Clock, DeliveryReceipt, FrameCapturer, ManualClock, OsActuator, PortError,
    TextRecognizer, VisionDetector,
};
use ommatix_domain::Scaling;
use ommatix_domain::bbox::BBox;
use ommatix_domain::fusion::{Actionability, Affordance, Control, Role, Word};
use ommatix_domain::incremental::BgraFrame;

const NAME: &str = "fixture-desktop";

/// A control with a confirmed affordance, enabled and visible.
pub fn confirmed(role: Role, bbox: BBox) -> Control {
    Control {
        bbox,
        role,
        affordance: Affordance {
            actionability: Actionability::Confirmed,
            enabled: true,
            visible: true,
        },
    }
}

/// Colour of item `id`; `seed` tells controls from words. Never black (the background).
fn colour(id: u32, seed: u32) -> [u8; 3] {
    let h = (id ^ seed).wrapping_add(1).wrapping_mul(0x9e37_79b1);
    let [b, g, r, _] = h.to_le_bytes();
    [b | 1, g, r]
}

const CONTROL_SEED: u32 = 0;
const WORD_SEED: u32 = 0x5bd1_e995;

/// Pixel span `[x0, x1) × [y0, y1)` covered by `bbox`, clipped to the frame.
fn pixel_span(bbox: &BBox, width: u32, height: u32) -> (usize, usize, usize, usize) {
    let clamp = |v: f64, max: u32| v.clamp(0.0, f64::from(max)) as usize;
    (
        clamp(bbox.x0().floor(), width),
        clamp(bbox.y0().floor(), height),
        clamp(bbox.x1().ceil(), width),
        clamp(bbox.y1().ceil(), height),
    )
}

/// What a fixture desktop shows: items keyed by their stable IDs.
#[derive(Debug, Clone, PartialEq)]
pub struct FixtureScene {
    width: u32,
    height: u32,
    geometry_epoch: u64,
    next_id: u32,
    controls: BTreeMap<u32, Control>,
    words: BTreeMap<u32, Word>,
}

impl FixtureScene {
    /// An empty `width` × `height` screen at geometry epoch 1. Sizes are clamped to 1..=4096.
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width: width.clamp(1, 4096),
            height: height.clamp(1, 4096),
            geometry_epoch: 1,
            next_id: 0,
            controls: BTreeMap::new(),
            words: BTreeMap::new(),
        }
    }

    pub const fn width(&self) -> u32 {
        self.width
    }

    pub const fn height(&self) -> u32 {
        self.height
    }

    pub const fn geometry_epoch(&self) -> u64 {
        self.geometry_epoch
    }

    fn take_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        id
    }

    /// Adds a control and returns its ID.
    pub fn add_control(&mut self, control: Control) -> u32 {
        let id = self.take_id();
        self.controls.insert(id, control);
        id
    }

    /// Adds a word and returns its ID.
    pub fn add_word(&mut self, text: impl Into<String>, bbox: BBox) -> u32 {
        let id = self.take_id();
        self.words.insert(id, Word::new(text, bbox));
        id
    }

    /// Moves an item (control or word) to `bbox`, keeping its ID and colour. Returns
    /// whether the ID exists.
    pub fn move_item(&mut self, id: u32, bbox: BBox) -> bool {
        if let Some(c) = self.controls.get_mut(&id) {
            c.bbox = bbox;
            true
        } else if let Some(w) = self.words.get_mut(&id) {
            w.bbox = bbox;
            true
        } else {
            false
        }
    }

    /// Replaces a control with a new one under a new ID (and colour), which it returns.
    pub fn replace_control(&mut self, id: u32, control: Control) -> Option<u32> {
        self.controls.remove(&id)?;
        Some(self.add_control(control))
    }

    /// Replaces a word with a new one under a new ID (and colour), which it returns.
    pub fn replace_word(&mut self, id: u32, text: impl Into<String>, bbox: BBox) -> Option<u32> {
        self.words.remove(&id)?;
        Some(self.add_word(text, bbox))
    }

    /// Removes an item. Returns whether the ID existed.
    pub fn remove(&mut self, id: u32) -> bool {
        self.controls.remove(&id).is_some() || self.words.remove(&id).is_some()
    }

    /// Changes the screen size and starts a new geometry epoch.
    pub fn resize(&mut self, width: u32, height: u32) {
        self.width = width.clamp(1, 4096);
        self.height = height.clamp(1, 4096);
        self.geometry_epoch += 1;
    }

    /// Starts a new geometry epoch without changing the size (a DPI change, say).
    pub fn bump_geometry_epoch(&mut self) {
        self.geometry_epoch += 1;
    }

    /// Controls by ID.
    pub fn controls(&self) -> impl Iterator<Item = (u32, &Control)> {
        self.controls.iter().map(|(id, c)| (*id, c))
    }

    /// Words by ID.
    pub fn words(&self) -> impl Iterator<Item = (u32, &Word)> {
        self.words.iter().map(|(id, w)| (*id, w))
    }

    /// Paints the scene. Pure: the same scene always gives the same pixels.
    pub fn render(&self) -> BgraFrame {
        let (w, h) = (self.width, self.height);
        let stride = 4 * w as usize;
        let mut px = vec![0u8; stride * h as usize];
        for x in px.iter_mut().skip(3).step_by(4) {
            *x = 255;
        }
        let mut paint = |bbox: &BBox, bgr: [u8; 3]| {
            let (x0, y0, x1, y1) = pixel_span(bbox, w, h);
            for y in y0..y1 {
                for x in x0..x1 {
                    let i = y * stride + 4 * x;
                    px[i..i + 3].copy_from_slice(&bgr);
                }
            }
        };
        for (id, c) in &self.controls {
            paint(&c.bbox, colour(*id, CONTROL_SEED));
        }
        for (id, word) in &self.words {
            paint(&word.bbox, colour(*id, WORD_SEED));
        }
        BgraFrame::new(w, h, stride, self.geometry_epoch, px)
            .expect("a clamped fixture size is a valid frame")
    }

    /// Controls whose box overlaps any region, by ID.
    pub fn controls_in(&self, regions: &[BBox]) -> Vec<Control> {
        self.controls
            .values()
            .filter(|c| regions.iter().any(|r| r.overlaps(&c.bbox)))
            .cloned()
            .collect()
    }

    /// Words whose box overlaps any region, by ID.
    pub fn words_in(&self, regions: &[BBox]) -> Vec<Word> {
        self.words
            .values()
            .filter(|w| regions.iter().any(|r| r.overlaps(&w.bbox)))
            .cloned()
            .collect()
    }
}

#[derive(Debug)]
struct FixtureState {
    scene: FixtureScene,
    /// The monitor's DPI (96 = 100 %): the profile's scaling unless a test changes it.
    dpi: u32,
    frame_seq: u64,
    fail_capture: bool,
    fail_inference: bool,
    regions_requested: Vec<Vec<BBox>>,
    handed: Vec<ValidatedAction>,
}

/// Owner of a shared [`FixtureScene`]; hands out the four port views.
#[derive(Clone)]
pub struct FixtureDesktop {
    state: Arc<Mutex<FixtureState>>,
    clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for FixtureDesktop {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FixtureDesktop")
            .field("state", &self.state)
            .finish_non_exhaustive()
    }
}

impl FixtureDesktop {
    /// `clock` stamps each capture; share it with the engine so `age_ms` is meaningful.
    pub fn new(scene: FixtureScene, clock: Arc<dyn Clock>) -> Self {
        Self {
            state: Arc::new(Mutex::new(FixtureState {
                scene,
                dpi: 96,
                frame_seq: 0,
                fail_capture: false,
                fail_inference: false,
                regions_requested: Vec::new(),
                handed: Vec::new(),
            })),
            clock,
        }
    }

    /// A desktop on a virtual clock that never advances.
    pub fn with_manual_clock(scene: FixtureScene) -> Self {
        Self::new(scene, Arc::new(ManualClock::new()))
    }

    fn lock(&self) -> MutexGuard<'_, FixtureState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// A copy of the current scene.
    pub fn scene(&self) -> FixtureScene {
        self.lock().scene.clone()
    }

    /// Changes the scene between captures; returns what `change` returns.
    pub fn update<T>(&self, change: impl FnOnce(&mut FixtureScene) -> T) -> T {
        change(&mut self.lock().scene)
    }

    /// Changes the monitor's DPI as Windows does: the next captures report it and the geometry
    /// epoch moves on (RNF-07, ADR-0031). The fixture starts at 96 DPI (100 %).
    pub fn set_dpi(&self, dpi: u32) {
        let mut state = self.lock();
        state.dpi = dpi;
        state.scene.bump_geometry_epoch();
    }

    /// Makes the next captures fail (`CAPTURE_UNAVAILABLE`) until switched off.
    pub fn set_capture_failure(&self, fail: bool) {
        self.lock().fail_capture = fail;
    }

    /// Makes detection fail (`INFERENCE_FAILED`) until switched off.
    pub fn set_inference_failure(&self, fail: bool) {
        self.lock().fail_inference = fail;
    }

    /// Every region list the detector was asked about, in order.
    pub fn regions_requested(&self) -> Vec<Vec<BBox>> {
        self.lock().regions_requested.clone()
    }

    /// Every action the actuator view was handed, in order.
    pub fn handed_actions(&self) -> Vec<ValidatedAction> {
        self.lock().handed.clone()
    }

    pub fn capturer(&self) -> FixtureCapturer {
        FixtureCapturer(self.clone())
    }

    pub fn detector(&self) -> FixtureDetector {
        FixtureDetector(self.clone())
    }

    pub fn recognizer(&self) -> FixtureRecognizer {
        FixtureRecognizer(self.clone())
    }

    pub fn actuator(&self) -> FixtureActuator {
        FixtureActuator(self.clone())
    }
}

/// [`FrameCapturer`] view of a [`FixtureDesktop`].
#[derive(Debug, Clone)]
pub struct FixtureCapturer(FixtureDesktop);

impl FrameCapturer for FixtureCapturer {
    fn name(&self) -> &'static str {
        NAME
    }

    fn capture(&mut self) -> Result<CapturedFrame, PortError> {
        let captured_at = self.0.clock.now();
        let mut state = self.0.lock();
        if state.fail_capture {
            return Err(PortError::new(NAME, "capture switched off by the test"));
        }
        state.frame_seq += 1;
        Ok(CapturedFrame {
            frame: state.scene.render(),
            frame_seq: state.frame_seq,
            captured_at,
            scaling: Scaling::Dpi(state.dpi),
        })
    }
}

/// [`VisionDetector`] view: the ground-truth controls that overlap the regions.
#[derive(Debug, Clone)]
pub struct FixtureDetector(FixtureDesktop);

impl VisionDetector for FixtureDetector {
    fn name(&self) -> &'static str {
        NAME
    }

    fn models_loaded(&self) -> bool {
        false
    }

    fn detect(&mut self, _: &BgraFrame, regions: &[BBox]) -> Result<Vec<Control>, PortError> {
        let mut state = self.0.lock();
        state.regions_requested.push(regions.to_vec());
        if state.fail_inference {
            return Err(PortError::new(NAME, "inference switched off by the test"));
        }
        Ok(state.scene.controls_in(regions))
    }
}

/// [`TextRecognizer`] view: the ground-truth words that overlap the regions.
#[derive(Debug, Clone)]
pub struct FixtureRecognizer(FixtureDesktop);

impl TextRecognizer for FixtureRecognizer {
    fn name(&self) -> &'static str {
        NAME
    }

    fn recognize(&mut self, _: &BgraFrame, regions: &[BBox]) -> Result<Vec<Word>, PortError> {
        let state = self.0.lock();
        if state.fail_inference {
            return Err(PortError::new(NAME, "inference switched off by the test"));
        }
        Ok(state.scene.words_in(regions))
    }
}

/// [`OsActuator`] view: not live; records what it is handed and submits nothing.
#[derive(Debug, Clone)]
pub struct FixtureActuator(FixtureDesktop);

impl OsActuator for FixtureActuator {
    fn name(&self) -> &'static str {
        NAME
    }

    fn is_live(&self) -> bool {
        false
    }

    fn deliver(&mut self, action: &ValidatedAction) -> Result<DeliveryReceipt, PortError> {
        self.0.lock().handed.push(action.clone());
        // A simulation hands nothing to the OS and never claims it did.
        Ok(DeliveryReceipt {
            submitted: 0,
            inserted: 0,
        })
    }
}

#[cfg(test)]
mod tests {
    use ommatix_domain::incremental::{DEFAULT_TILE_SIZE, TileCoord, dirty_tiles};

    use super::*;

    fn bx(x0: f64, y0: f64, x1: f64, y1: f64) -> BBox {
        BBox::new(x0, y0, x1, y1).unwrap()
    }

    fn dirty(a: &BgraFrame, b: &BgraFrame) -> Vec<TileCoord> {
        dirty_tiles(Some(a), b, DEFAULT_TILE_SIZE)
            .unwrap()
            .tiles
            .into_iter()
            .collect()
    }

    #[test]
    fn changes_dirty_exactly_the_tiles_they_touch() {
        let mut scene = FixtureScene::new(256, 128);
        let save = scene.add_control(confirmed(Role::Button, bx(10.0, 10.0, 50.0, 30.0)));
        let caption = scene.add_word("Save", bx(15.0, 15.0, 45.0, 25.0));
        scene.add_control(confirmed(Role::Input, bx(140.0, 70.0, 250.0, 100.0)));
        let before = scene.render();
        assert_eq!(before, scene.render(), "deterministic");

        let mut moved = scene.clone();
        assert!(moved.move_item(save, bx(12.0, 10.0, 52.0, 30.0)));
        assert_eq!(dirty(&before, &moved.render()), vec![TileCoord::new(0, 0)]);

        let mut removed = scene.clone();
        assert!(removed.remove(caption));
        assert_eq!(
            dirty(&before, &removed.render()),
            vec![TileCoord::new(0, 0)]
        );

        let mut replaced = scene.clone();
        let new_id = replaced
            .replace_control(save, confirmed(Role::Input, bx(10.0, 10.0, 50.0, 30.0)))
            .unwrap();
        assert_ne!(new_id, save, "IDs are never reused");
        assert_eq!(
            dirty(&before, &replaced.render()),
            vec![TileCoord::new(0, 0)]
        );
    }

    /// RNF-07, ADR-0031: the fixture is a 100 % monitor, so it is in the profile.
    #[test]
    fn the_fixture_reports_100_percent_scaling() {
        let desktop = FixtureDesktop::with_manual_clock(FixtureScene::new(20, 10));
        let frame = desktop.capturer().capture().unwrap();
        assert_eq!(frame.scaling, Scaling::Dpi(96));
    }

    #[test]
    fn port_views_share_the_scene_and_report_ground_truth_in_the_regions() {
        let mut scene = FixtureScene::new(200, 100);
        scene.add_control(confirmed(Role::Button, bx(10.0, 10.0, 50.0, 30.0)));
        scene.add_control(confirmed(Role::Button, bx(150.0, 60.0, 190.0, 90.0)));
        scene.add_word("OK", bx(160.0, 70.0, 180.0, 80.0));
        let desktop = FixtureDesktop::with_manual_clock(scene);
        let (mut cap, mut det, mut rec, act) = (
            desktop.capturer(),
            desktop.detector(),
            desktop.recognizer(),
            desktop.actuator(),
        );
        let first = cap.capture().unwrap();
        assert_eq!((first.frame_seq, first.frame.geometry_epoch()), (1, 1));
        let region = [bx(100.0, 50.0, 200.0, 100.0)];
        let controls = det.detect(&first.frame, &region).unwrap();
        assert_eq!(controls.len(), 1);
        assert_eq!(controls[0].bbox, bx(150.0, 60.0, 190.0, 90.0));
        assert_eq!(rec.recognize(&first.frame, &region).unwrap().len(), 1);
        assert_eq!(desktop.regions_requested(), vec![region.to_vec()]);
        assert!(!det.models_loaded() && !act.is_live());

        desktop.update(|s| s.resize(300, 100));
        let second = cap.capture().unwrap();
        assert_eq!((second.frame_seq, second.frame.geometry_epoch()), (2, 2));
        assert_eq!(second.frame.width(), 300);

        desktop.set_inference_failure(true);
        assert!(det.detect(&second.frame, &region).is_err());
        assert!(rec.recognize(&second.frame, &region).is_err());
        desktop.set_capture_failure(true);
        assert!(cap.capture().is_err());
    }
}
