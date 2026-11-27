//! The support-aware closure in the engine (ADR-0045): what the merge keeps (UT-84) and the
//! differential test of a regional observation against a full read of the same frames
//! (UT-85), what `--plan-check` says of the words that differ (O3d), and the crop-edge
//! defence (UT-89, O3e).

use super::*;

use crate::contract::ElementView;
use crate::engine::crop_edge::{CropEdgeKinds, CropEdgeOutcome};
use crate::engine::observe::EvidenceSet;
use crate::engine::plan_check::{Unexplained, WordDetail, WordPlaces};
use crate::ports::FrameCapturer;
use ommatix_domain::fusion::AccessibleControl;
use ommatix_domain::incremental::{PlanChoice, PlanTrace};

fn unknown(bbox: BBox) -> Control {
    Control {
        bbox,
        role: Role::Unknown,
        affordance: Affordance {
            actionability: Actionability::Unknown,
            enabled: true,
            visible: true,
        },
    }
}

fn evidence(controls: &[BBox], surfaces: &[BBox]) -> EvidenceSet {
    EvidenceSet {
        controls: controls.iter().copied().map(unknown).collect(),
        surfaces: surfaces.to_vec(),
        ..EvidenceSet::default()
    }
}

/// The overlap rule the merge had before ADR-0045, for controls.
fn merged_by_overlap(held: &EvidenceSet, detected: &[BBox], fresh: EvidenceSet) -> Vec<Control> {
    let mut out: Vec<Control> = held
        .controls
        .iter()
        .filter(|c| !detected.iter().any(|r| r.overlaps(&c.bbox)))
        .cloned()
        .collect();
    out.extend(fresh.controls);
    out
}

const FRAME: (f64, f64) = (640.0, 320.0);

fn bounds() -> BBox {
    bx(0.0, 0.0, FRAME.0, FRAME.1)
}

/// UT-84: a spanned surface a region meets is kept, unchanged and once, even when the crop
/// reports its box again; a cut-prone box inside a region is replaced by what the crop found;
/// a box outside every region is kept.
#[test]
fn ut_84_a_spanned_surface_is_kept_and_a_box_inside_a_region_is_replaced() {
    let surface = bx(100.0, 50.0, 500.0, 300.0);
    let inside = bx(210.0, 110.0, 240.0, 130.0);
    let outside = bx(560.0, 10.0, 600.0, 30.0);
    let held = evidence(&[surface, inside, outside], &[surface]);
    let region = bx(200.0, 100.0, 260.0, 140.0);
    let moved = bx(212.0, 112.0, 242.0, 132.0);
    // The crop reports the moved control, and (as a detector that broke its contract might)
    // the surface again.
    let fresh = evidence(&[moved, surface], &[surface]);
    let next = held
        .merged(&[region], &[region], fresh, false)
        .normalised(&bounds());
    let boxes: Vec<BBox> = next.controls.iter().map(|c| c.bbox).collect();
    assert_eq!(boxes, vec![surface, moved, outside]);
    assert_eq!(next.surfaces, vec![surface]);
}

/// UT-84: a surface dropped with its control leaves the surface list; the list never names a
/// box that is not a control.
#[test]
fn ut_84_a_surface_goes_with_its_control() {
    let surface = bx(100.0, 50.0, 200.0, 150.0);
    let held = evidence(&[surface], &[surface]);
    let region = bx(90.0, 40.0, 210.0, 160.0);
    let next = held
        .merged(&[region], &[region], EvidenceSet::default(), false)
        .normalised(&bounds());
    assert_eq!(next.controls, Vec::<Control>::new());
    assert_eq!(next.surfaces, Vec::<BBox>::new());
}

/// UT-84: for a classic plan (every held box a region meets lies inside it) the merge is the
/// overlap merge it replaced, on generated evidence.
#[test]
fn ut_84_a_classic_plans_merge_is_todays_on_generated_evidence() {
    let mut rng = Rng(0x5EED_0084);
    let mut compared = 0;
    for _ in 0..500 {
        let regions: Vec<BBox> = (0..=rng.below(3))
            .map(|_| rng.rect(640, 320, 200))
            .collect();
        // Held boxes the classic closure allows: inside a region, or meeting none.
        let held: Vec<BBox> = (0..rng.below(12))
            .map(|_| rng.rect(640, 320, 60))
            .filter(|b| {
                regions.iter().all(|r| !r.overlaps(b)) || regions.iter().any(|r| r.contains(b))
            })
            .collect();
        let fresh: Vec<BBox> = (0..rng.below(4)).map(|_| rng.rect(640, 320, 40)).collect();
        let old = evidence(&held, &[]);
        let new_rule = old
            .merged(&regions, &regions, evidence(&fresh, &[]), false)
            .normalised(&bounds());
        let old_rule = EvidenceSet {
            controls: merged_by_overlap(&old, &regions, evidence(&fresh, &[])),
            ..EvidenceSet::default()
        }
        .normalised(&bounds());
        assert_eq!(new_rule.controls, old_rule.controls);
        compared += 1;
    }
    assert_eq!(compared, 500);
}

/// The three lists of the support-aware closure are the planner's supports, as multisets.
#[test]
fn the_spanning_lists_are_the_supports_split() {
    let surface = bx(100.0, 50.0, 500.0, 300.0);
    let mut e = evidence(&[surface, bx(10.0, 10.0, 20.0, 20.0)], &[surface]);
    e.words = vec![Word {
        bbox: bx(30.0, 10.0, 60.0, 20.0),
        text: "a".into(),
    }];
    let lists = e.spanning_lists();
    let mut split: Vec<BBox> = lists
        .cut_prone
        .iter()
        .chain(&lists.surfaces)
        .chain(&lists.whole_on_read)
        .copied()
        .collect();
    let mut supports = e.supports();
    split.sort_by(BBox::cmp_corners);
    supports.sort_by(BBox::cmp_corners);
    assert_eq!(split, supports);
    assert_eq!(lists.surfaces, vec![surface]);
}

// ---- UT-85: regional against full ----

/// splitmix64: the same scenes on every run.
struct Rng(u64);

impl Rng {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: u32) -> u32 {
        u32::try_from(self.next_u64() % u64::from(n.max(1))).unwrap()
    }

    fn between(&mut self, lo: u32, hi: u32) -> u32 {
        lo + self.below(hi - lo + 1)
    }

    /// A box with whole-pixel corners inside `w x h`, sides at most `max_side`.
    fn rect(&mut self, w: u32, h: u32, max_side: u32) -> BBox {
        let bw = 1 + self.below(max_side.min(w));
        let bh = 1 + self.below(max_side.min(h));
        let x = self.below(w - bw + 1);
        let y = self.below(h - bh + 1);
        bx(
            f64::from(x),
            f64::from(y),
            f64::from(x + bw),
            f64::from(y + bh),
        )
    }
}

/// One accessibility element of the scene and the pixel that shows its state.
#[derive(Debug, Clone)]
struct SceneElement {
    bbox: BBox,
    name: String,
    enabled: bool,
}

/// What the fakes read: solid surfaces (nested or disjoint), controls, words, accessibility
/// elements and caret pixels. Every change of the scene changes pixels inside the box of what
/// changed, as a real screen does.
#[derive(Debug, Clone)]
struct Scene {
    w: u32,
    h: u32,
    surfaces: Vec<(BBox, [u8; 3])>,
    controls: Vec<(BBox, [u8; 3])>,
    words: Vec<(BBox, String, [u8; 3])>,
    elements: Vec<SceneElement>,
    carets: Vec<(u32, u32)>,
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // whole-pixel boxes
fn px(v: f64) -> usize {
    v as usize
}

impl Scene {
    fn render(&self) -> BgraFrame {
        let stride = 4 * self.w as usize;
        let mut buf = vec![0u8; stride * self.h as usize];
        let mut fill = |b: &BBox, c: [u8; 3]| {
            for y in px(b.y0())..px(b.y1()) {
                for x in px(b.x0())..px(b.x1()) {
                    let i = y * stride + 4 * x;
                    buf[i..i + 3].copy_from_slice(&c);
                    buf[i + 3] = 255;
                }
            }
        };
        fill(
            &bx(0.0, 0.0, f64::from(self.w), f64::from(self.h)),
            [12, 12, 12],
        );
        for (b, c) in &self.surfaces {
            fill(b, *c);
        }
        for (b, c) in &self.controls {
            fill(b, *c);
        }
        for (b, _, c) in &self.words {
            fill(b, *c);
        }
        for e in &self.elements {
            let (cx, cy) = e.bbox.center();
            let (x, y) = (cx.floor(), cy.floor());
            let mark = if e.enabled { [0, 250, 0] } else { [250, 0, 0] };
            fill(&bx(x, y, x + 1.0, y + 1.0), mark);
        }
        for &(x, y) in &self.carets {
            let (x, y) = (f64::from(x), f64::from(y));
            fill(&bx(x, y, x + 1.0, y + 8.0), [255, 255, 255]);
        }
        BgraFrame::new(self.w, self.h, stride, 1, buf).unwrap()
    }
}

type SharedScene = Arc<Mutex<Scene>>;

fn scene_of(s: &SharedScene) -> MutexGuard<'_, Scene> {
    s.lock().unwrap_or_else(PoisonError::into_inner)
}

struct SceneCapturer(SharedScene, Arc<ManualClock>, u64);

impl FrameCapturer for SceneCapturer {
    fn name(&self) -> &'static str {
        "scene-capturer"
    }

    fn capture(&mut self) -> Result<CapturedFrame, PortError> {
        self.2 += 1;
        Ok(CapturedFrame {
            frame: scene_of(&self.0).render(),
            frame_seq: self.2,
            captured_at: self.1.now(),
            scaling: Scaling::Dpi(96),
        })
    }
}

/// Reports a surface whole when it lies inside a region and not at all otherwise, and cuts
/// every other control at the edge of each region it meets, as a glyph group would be cut.
/// Declares a band of 4 pixels.
struct SceneDetector(SharedScene, Vec<BBox>, Option<u32>);

impl VisionDetector for SceneDetector {
    fn name(&self) -> &'static str {
        "scene-detector"
    }

    fn models_loaded(&self) -> bool {
        false
    }

    fn detect(&mut self, _: &BgraFrame, regions: &[BBox]) -> Result<Vec<Control>, PortError> {
        let s = scene_of(&self.0);
        self.1 = s
            .surfaces
            .iter()
            .map(|(b, _)| *b)
            .filter(|b| regions.iter().any(|r| r.contains(b)))
            .collect();
        let mut out: Vec<Control> = self.1.iter().copied().map(unknown).collect();
        for (b, _) in &s.controls {
            out.extend(regions.iter().filter_map(|r| b.clip(r)).map(unknown));
        }
        Ok(out)
    }

    fn surface_band_px(&self) -> Option<u32> {
        self.2
    }

    fn surfaces(&self) -> Vec<BBox> {
        self.1.clone()
    }
}

/// Cuts words at the edge of each region they meet.
struct SceneRecognizer(SharedScene);

impl TextRecognizer for SceneRecognizer {
    fn name(&self) -> &'static str {
        "scene-ocr"
    }

    fn recognize(&mut self, _: &BgraFrame, regions: &[BBox]) -> Result<Vec<Word>, PortError> {
        let s = scene_of(&self.0);
        Ok(s.words
            .iter()
            .flat_map(|(b, text, _)| {
                regions.iter().filter_map(move |r| {
                    b.clip(r).map(|bbox| Word {
                        bbox,
                        text: text.clone(),
                    })
                })
            })
            .collect())
    }
}

/// Returns every element whose box meets the regions, whole.
struct SceneAccess(SharedScene);

impl AccessibilityProvider for SceneAccess {
    fn name(&self) -> &'static str {
        "scene-accessibility"
    }

    fn snapshot(&mut self, regions: &[BBox]) -> Result<AccessibleRead, PortError> {
        let s = scene_of(&self.0);
        Ok(AccessibleRead::complete(
            s.elements
                .iter()
                .filter(|e| regions.iter().any(|r| r.overlaps(&e.bbox)))
                .map(|e| AccessibleElement {
                    bbox: e.bbox,
                    role: Role::Button,
                    name: e.name.clone(),
                    enabled: e.enabled,
                    offscreen: false,
                    password: false,
                    patterns: AccessiblePatterns {
                        invoke: true,
                        ..AccessiblePatterns::default()
                    },
                    native: None,
                })
                .collect(),
        ))
    }

    fn reads_keyboard_focus(&self) -> bool {
        false
    }

    fn keyboard_focus(&mut self) -> Result<Option<AccessibleElement>, PortError> {
        Ok(None)
    }
}

fn scene_engine(scene: &SharedScene, params: PlanParams) -> Engine {
    scene_engine_with_band(scene, params, Some(4))
}

fn scene_engine_with_band(scene: &SharedScene, params: PlanParams, band: Option<u32>) -> Engine {
    scene_engine_reading(
        scene,
        params,
        band,
        Box::new(SceneRecognizer(scene.clone())),
    )
}

fn scene_engine_reading(
    scene: &SharedScene,
    params: PlanParams,
    band: Option<u32>,
    recognizer: Box<dyn TextRecognizer>,
) -> Engine {
    scene_engine_with(
        scene,
        params,
        Box::new(SceneDetector(scene.clone(), Vec::new(), band)),
        recognizer,
    )
}

fn scene_engine_with(
    scene: &SharedScene,
    params: PlanParams,
    detector: Box<dyn VisionDetector>,
    recognizer: Box<dyn TextRecognizer>,
) -> Engine {
    let world = Arc::new(Mutex::new(World::default()));
    let clock = Arc::new(ManualClock::new());
    let operator_stop = lock(&world).operator_stop.clone();
    let mut config = EngineConfig::new(SESSION);
    config.plan = params;
    Engine::new(
        config,
        Ports {
            capturer: Box::new(SceneCapturer(scene.clone(), clock.clone(), 0)),
            detector,
            recognizer,
            accessibility: Box::new(SceneAccess(scene.clone())),
            actuator: Box::new(Actuator(world)),
            policy: Arc::new(Policy(
                Mutex::default(),
                Mutex::default(),
                PurgeFile::default(),
                operator_stop,
            )),
            clock,
        },
    )
}

/// What must be equal: the normalised evidence (accessibility elements without the instant
/// of their read, which is stated to differ) and the page.
type Published = (
    Vec<Control>,
    Vec<Word>,
    Vec<AccessibleControl>,
    Vec<BBox>,
    Vec<ElementView>,
    usize,
);

fn published(engine: &mut Engine) -> Published {
    let page = engine
        .observe_screen(&ObserveRequest {
            limit: 200,
            ..ObserveRequest::default()
        })
        .unwrap();
    let e = &engine.store.published().unwrap().evidence;
    (
        e.controls.clone(),
        e.words.clone(),
        e.accessible.iter().map(|a| a.element.clone()).collect(),
        e.surfaces.clone(),
        page.elements,
        page.total_elements,
    )
}

/// Whether a region of the plan met an evidence box larger than itself: a box it overlaps
/// without containing, which only the support-aware closure allows.
fn spanned_something(trace: &PlanTrace) -> bool {
    trace.chosen == PlanChoice::Spanning
        && trace
            .regions
            .iter()
            .any(|r| r.largest_support_px > r.area_px)
}

/// Runs `steps` on two engines over the same scene, one incremental with the support-aware
/// closure and one with the full-frame baseline (B0), and requires equal evidence and pages
/// after every frame. Returns how many regional observations spanned a box.
fn differential(
    scene: Scene,
    params: PlanParams,
    steps: &mut dyn FnMut(&mut Scene, usize) -> bool,
    label: &str,
) -> usize {
    let shared = Arc::new(Mutex::new(scene));
    let mut regional = scene_engine(&shared, params);
    regional.set_plan_trace(true);
    let mut full = scene_engine(
        &shared,
        PlanParams {
            experimental_full_frame: true,
            ..params
        },
    );
    let mut engaged = 0;
    let mut step = 0;
    loop {
        let a = published(&mut regional);
        let b = published(&mut full);
        if a != b {
            let trace = regional.last_plan_trace().cloned();
            panic!(
                "{label}, step {step}: the regional observation differs from the full read\n\
                 regional {a:?}\nfull {b:?}\ntrace {trace:?}"
            );
        }
        if regional.last_plan_trace().is_some_and(spanned_something) {
            engaged += 1;
        }
        if !steps(&mut scene_of(&shared), step) {
            break;
        }
        step += 1;
    }
    engaged
}

fn small_params() -> PlanParams {
    PlanParams {
        tile_size: 16,
        halo_px: 8.0,
        max_regions: 6,
        max_area_fraction: 0.6,
        ..PlanParams::default()
    }
}

/// A random scene on a small frame.
fn small_scene(rng: &mut Rng) -> Scene {
    let (w, h) = (240, 180);
    let mut surfaces = Vec::new();
    for i in 0..rng.between(1, 3) {
        let b = rng.rect(w, h, 160);
        let c = [60 + 40 * i as u8, 90, 140];
        surfaces.push((b, c));
        // Sometimes a surface nested inside the last one, 10 px or more in.
        if rng.below(2) == 0 && b.width() > 40.0 && b.height() > 40.0 {
            let inner = bx(b.x0() + 10.0, b.y0() + 10.0, b.x1() - 10.0, b.y1() - 10.0);
            surfaces.push((inner, [70 + 40 * i as u8, 160, 90]));
        }
    }
    let controls = (0..rng.below(4))
        .map(|i| (rng.rect(w, h, 30), [200, 40 + 30 * i as u8, 40]))
        .collect();
    let words = (0..rng.below(4))
        .map(|i| {
            (
                rng.rect(w, h, 24),
                format!("w{i}"),
                [20, 200 - 20 * i as u8, 200],
            )
        })
        .collect();
    let elements = (0..rng.below(3))
        .map(|i| SceneElement {
            bbox: rng.rect(w, h, 200),
            name: format!("e{i}"),
            enabled: true,
        })
        .collect();
    Scene {
        w,
        h,
        surfaces,
        controls,
        words,
        elements,
        carets: Vec::new(),
    }
}

/// One random change of the kinds the unit lists: a caret toggling inside a surface, a small
/// change near a surface's edge, a surface's edge moving, a window appearing, a word
/// changing, an element's state changing, a control appearing or going.
#[allow(clippy::too_many_lines)]
fn change(scene: &mut Scene, rng: &mut Rng, counter: &mut u8) {
    *counter = counter.wrapping_add(1);
    let (w, h) = (scene.w, scene.h);
    match rng.below(7) {
        0 if !scene.surfaces.is_empty() => {
            let (b, _) = scene.surfaces[rng.below(scene.surfaces.len() as u32) as usize];
            let x = rng.between(px(b.x0()) as u32, px(b.x1()) as u32 - 1);
            let y = rng.between(px(b.y0()) as u32, px(b.y1()) as u32 - 1);
            if y + 8 <= h {
                match scene.carets.iter().position(|&c| c == (x, y)) {
                    Some(i) => {
                        scene.carets.remove(i);
                    }
                    None => scene.carets.push((x, y)),
                }
            }
        }
        1 if !scene.surfaces.is_empty() => {
            // A small control just outside or just inside a surface's edge, within the halo.
            let (b, _) = scene.surfaces[rng.below(scene.surfaces.len() as u32) as usize];
            let d = f64::from(rng.below(12)) - 6.0;
            let x = (b.x0() + d).clamp(0.0, f64::from(w) - 3.0);
            let y = f64::midpoint(b.y0(), b.y1()).floor();
            let mark = bx(x, y, x + 3.0, (y + 3.0).min(f64::from(h)));
            scene.controls.push((mark, [210, 210, *counter]));
        }
        2 if !scene.surfaces.is_empty() => {
            let i = rng.below(scene.surfaces.len() as u32) as usize;
            let (b, c) = scene.surfaces[i];
            let grow = f64::from(rng.below(5)) - 2.0;
            if let Ok(moved) = BBox::new(b.x0(), b.y0(), (b.x1() + grow).min(f64::from(w)), b.y1())
            {
                scene.surfaces[i] = (moved, c);
            }
        }
        3 => {
            let b = rng.rect(w, h, 90);
            scene.surfaces.push((b, [30, 30, 100 + *counter / 2]));
            if b.width() > 20.0 && b.height() > 20.0 {
                let inner = bx(b.x0() + 5.0, b.y0() + 5.0, b.x0() + 15.0, b.y0() + 15.0);
                scene.controls.push((inner, [220, 120, *counter]));
            }
        }
        4 if !scene.words.is_empty() => {
            let i = rng.below(scene.words.len() as u32) as usize;
            let (b, text, c) = scene.words[i].clone();
            let c = [c[0], c[1], c[2].wrapping_add(17)];
            if rng.below(2) == 0 {
                scene.words[i] = (b, format!("{text}+"), c);
            } else {
                scene.words[i] = (rng.rect(w, h, 24), text, c);
            }
        }
        5 if !scene.elements.is_empty() => {
            let i = rng.below(scene.elements.len() as u32) as usize;
            scene.elements[i].enabled = !scene.elements[i].enabled;
        }
        _ => {
            if !scene.controls.is_empty() && rng.below(2) == 0 {
                let i = rng.below(scene.controls.len() as u32) as usize;
                scene.controls.remove(i);
            } else {
                let b = rng.rect(w, h, 20);
                scene.controls.push((b, [230, *counter, 30]));
            }
        }
    }
}

/// UT-85 (ADR-0045, the differential test of E): on many small generated scenes, after every
/// frame, the evidence and the page of the incremental engine with the support-aware closure
/// equal those of the full-frame baseline, except the instant each accessibility element was
/// read. The fakes cut every box but a surface at a crop's edge, so a region that cut a box
/// would show here.
#[test]
fn ut_85_regional_observations_equal_a_full_read_on_generated_scenes() {
    let mut rng = Rng(0x5EED_0085);
    let mut engaged = 0;
    let mut observations = 0;
    for sequence in 0..60 {
        let scene = small_scene(&mut rng);
        let steps = 10;
        let mut counter = 0u8;
        let mut seq_rng = Rng(rng.next_u64());
        engaged += differential(
            scene,
            small_params(),
            &mut |s, step| {
                if step >= steps {
                    return false;
                }
                // A change no pixel shows (a control drawn under a word) is not a change a
                // screen can make: draw another one.
                let before = s.render();
                for _ in 0..50 {
                    let saved = s.clone();
                    change(s, &mut seq_rng, &mut counter);
                    if s.render().pixels() != before.pixels() {
                        return true;
                    }
                    *s = saved;
                }
                panic!("no visible change found");
            },
            &format!("sequence {sequence}"),
        );
        observations += steps + 1;
    }
    assert_eq!(observations, 660);
    eprintln!("UT-85: {engaged} of {observations} observations spanned a box");
    assert!(
        engaged >= 60,
        "only {engaged} observations spanned a box: the test would pass vacuously"
    );
}

/// UT-85 at 1920 x 1200 with the boxes measured in the lab (section 2 of the unit): a
/// Notepad window's surface, the Document as an accessibility element, menu words, the
/// clock. A caret blinks inside the window and the Document, the clock ticks, a menu word
/// changes, the window's edge moves. The product's plan parameters.
#[test]
fn ut_85_regional_observations_equal_a_full_read_on_the_measured_scene() {
    let window = bx(247.0, 195.0, 1673.0, 1006.0);
    let scene = Scene {
        w: 1920,
        h: 1200,
        surfaces: vec![(window, [243, 243, 243])],
        controls: vec![(bx(260.0, 205.0, 300.0, 225.0), [90, 90, 90])],
        words: vec![
            (bx(270.0, 230.0, 320.0, 246.0), "File".into(), [20, 20, 20]),
            (bx(330.0, 230.0, 380.0, 246.0), "Edit".into(), [21, 20, 20]),
            (
                bx(1850.0, 1100.0, 1890.0, 1180.0),
                "12:00".into(),
                [200, 200, 200],
            ),
        ],
        elements: vec![SceneElement {
            bbox: bx(0.0, 99.0, 1920.0, 1082.0),
            name: "Text editor".into(),
            enabled: true,
        }],
        carets: Vec::new(),
    };
    let params = PlanParams {
        adaptive_min_regions: 2,
        ..PlanParams::default()
    };
    let engaged = differential(
        scene,
        params,
        &mut |s, step| {
            match step {
                0 | 2 | 4 => s.carets.push((700, 500)),
                1 | 3 | 5 => s.carets.clear(),
                6 => s.words[2] = (s.words[2].0, "12:01".into(), [201, 200, 200]),
                7 => s.words[1] = (s.words[1].0, "Edit*".into(), [22, 20, 20]),
                8 => s.surfaces[0].0 = bx(247.0, 195.0, 1675.0, 1006.0),
                9 => s.carets.push((1000, 800)),
                _ => return false,
            }
            true
        },
        "measured scene",
    );
    assert!(engaged >= 6, "only {engaged} observations spanned a box");
    eprintln!("UT-85 measured scene: {engaged} of 11 observations spanned a box");
}

/// R0 in the engine: a detector that declares no band gets today's plan, even with a
/// surface and an accessibility box that the support-aware closure would leave out.
#[test]
fn ut_85_a_detector_without_a_band_plans_todays_plan() {
    let window = bx(40.0, 40.0, 600.0, 300.0);
    let scene = Scene {
        w: 640,
        h: 320,
        surfaces: vec![(window, [243, 243, 243])],
        controls: Vec::new(),
        words: Vec::new(),
        elements: vec![SceneElement {
            bbox: bx(0.0, 20.0, 640.0, 310.0),
            name: "Document".into(),
            enabled: true,
        }],
        carets: Vec::new(),
    };
    let plans = |band: Option<u32>| {
        let shared = Arc::new(Mutex::new(scene.clone()));
        let mut engine = scene_engine_with_band(&shared, PlanParams::default(), band);
        engine.set_plan_trace(true);
        engine.observe_screen(&ObserveRequest::default()).unwrap();
        scene_of(&shared).carets.push((300, 150));
        engine.observe_screen(&ObserveRequest::default()).unwrap();
        (
            engine.last_plan().unwrap(),
            engine.last_plan_trace().unwrap().chosen,
        )
    };
    let (today, chosen) = plans(None);
    assert_eq!(today.work, WorkKind::Full);
    assert_eq!(
        today.full_refresh_reason,
        Some(FullRefreshReason::AreaBudget)
    );
    assert_eq!(chosen, PlanChoice::Classic);
    let (aware, chosen) = plans(Some(4));
    assert_eq!(aware.work, WorkKind::Regions);
    assert_eq!(chosen, PlanChoice::Spanning);
}

// ---- O3d: what `--plan-check` says of a differing word ----

/// How a recogniser reads a crop differently from the whole frame: the stated limit L3 of
/// ADR-0045, in three shapes. On the whole frame every shape reads the scene's words exactly.
#[derive(Debug, Clone, Copy)]
enum CropReading {
    /// One pixel lower.
    Lower,
    /// A caret within 6 pixels right of a word read as part of it.
    CaretGlued,
    /// One pixel short on the right (a tighter box than the whole frame's).
    Tighter,
    /// [`CropReading::Tighter`] on the first crop read only, exact afterwards.
    TighterOnce,
    /// [`CropReading::Tighter`] on every other crop read: a box held from one crop can be
    /// tighter than the next crop's reading of the same pixels.
    TighterAlternately,
    /// Words of one line less than [`JOIN_GAP_PX`] apart read as one word (their union, the
    /// texts joined): a crop's context changes how a recogniser splits a line. The joined
    /// word can reach far past the region, to the crop's edge and past it.
    Joined,
    /// On every other crop read, every word [`WIDER_PX`] wider and taller (L3: a box read
    /// larger than the whole frame reads it): to the right and down on the first of each two
    /// such reads, to the left and up on the second (O3h: so that a widening reaches all four
    /// sides of a crop's band), within the crop; and on every crop read, every word whose
    /// **pixels** (its box in the scene) are not [`readable_in`] the area asked about is
    /// **dropped**, not clipped: a recogniser that loses a word the crop's edge cuts or comes
    /// close to (O3g). Judged on the pixels, not on the larger reading: a word dropped because
    /// its reading, larger than its pixels, reaches the band is the stated limit L4 (see
    /// `o3h_a_word_read_past_the_band_and_dropped_is_the_stated_limit_l4`).
    WiderDroppingBand,
}

/// How much wider and taller [`CropReading::WiderDroppingBand`] reads a word.
const WIDER_PX: f64 = 14.0;

/// The gap below which [`CropReading::Joined`] joins two words of a line.
const JOIN_GAP_PX: f64 = 10.0;

/// The words of a scene as [`CropReading::Joined`] reads them from a crop.
fn joined(words: &[(BBox, String, [u8; 3])]) -> Vec<(BBox, String)> {
    let mut sorted: Vec<(BBox, String)> = words.iter().map(|(b, t, _)| (*b, t.clone())).collect();
    sorted.sort_by(|a, b| {
        a.0.y0()
            .total_cmp(&b.0.y0())
            .then(a.0.x0().total_cmp(&b.0.x0()))
    });
    let mut out: Vec<(BBox, String)> = Vec::new();
    for (b, t) in sorted {
        if let Some((last, text)) = out.last_mut()
            // Whole-pixel boxes: the same line has the same top and bottom.
            && (last.y0() - b.y0()).abs() < 0.5
            && (last.y1() - b.y1()).abs() < 0.5
            && b.x0() >= last.x1()
            && b.x0() - last.x1() < JOIN_GAP_PX
        {
            *last = last.union(&b);
            text.push(' ');
            text.push_str(&t);
            continue;
        }
        out.push((b, t));
    }
    out
}

/// [`SceneRecognizer`] on the whole frame; on a crop, the word read per [`CropReading`] and
/// clipped to the area it was asked about, as the Windows recogniser clips. A word it clipped
/// is returned with [`CLIPPED`] after its text, as a recogniser that sees part of a word reads
/// another text. The count is the crop reads so far.
struct CropRecognizer(SharedScene, CropReading, usize);

impl TextRecognizer for CropRecognizer {
    fn name(&self) -> &'static str {
        "scene-ocr-crop"
    }

    fn recognize(&mut self, frame: &BgraFrame, regions: &[BBox]) -> Result<Vec<Word>, PortError> {
        let s = scene_of(&self.0);
        let whole = frame.bounds();
        let cropped = regions.iter().any(|r| *r != whole);
        let first_crop = cropped && self.2 == 0;
        self.2 += usize::from(cropped);
        let source: Vec<(BBox, String)> = if cropped && matches!(self.1, CropReading::Joined) {
            joined(&s.words)
        } else {
            s.words.iter().map(|(b, t, _)| (*b, t.clone())).collect()
        };
        let mut out = Vec::new();
        for r in regions {
            for (b, text) in &source {
                let (mut b, mut text) = (*b, text.clone());
                // The word's own pixels, before a crop reads its box otherwise.
                let pixels = b;
                if *r != whole {
                    match self.1 {
                        CropReading::Lower => {
                            b = bx(b.x0(), b.y0() + 1.0, b.x1(), b.y1() + 1.0);
                        }
                        CropReading::CaretGlued => {
                            let glued = s.carets.iter().find(|&&(x, y)| {
                                let (x, y) = (f64::from(x), f64::from(y));
                                x >= b.x1() && x < b.x1() + 6.0 && y >= b.y0() && y < b.y1()
                            });
                            if let Some(&(x, _)) = glued {
                                b = bx(b.x0(), b.y0(), f64::from(x) + 1.0, b.y1());
                                text.push('|');
                            }
                        }
                        CropReading::Tighter if b.width() > 2.0 => {
                            b = bx(b.x0(), b.y0(), b.x1() - 1.0, b.y1());
                        }
                        CropReading::TighterOnce if first_crop && b.width() > 2.0 => {
                            b = bx(b.x0(), b.y0(), b.x1() - 1.0, b.y1());
                        }
                        CropReading::TighterAlternately if self.2 % 2 == 1 && b.width() > 2.0 => {
                            b = bx(b.x0(), b.y0(), b.x1() - 1.0, b.y1());
                        }
                        CropReading::WiderDroppingBand if self.2 % 4 == 1 => {
                            b = bx(
                                b.x0(),
                                b.y0(),
                                (b.x1() + WIDER_PX).min(whole.x1()),
                                (b.y1() + WIDER_PX).min(whole.y1()),
                            );
                        }
                        CropReading::WiderDroppingBand if self.2 % 4 == 3 => {
                            b = bx(
                                (b.x0() - WIDER_PX).max(whole.x0()),
                                (b.y0() - WIDER_PX).max(whole.y0()),
                                b.x1(),
                                b.y1(),
                            );
                        }
                        CropReading::Tighter
                        | CropReading::TighterOnce
                        | CropReading::TighterAlternately
                        | CropReading::Joined
                        | CropReading::WiderDroppingBand => {}
                    }
                    if matches!(self.1, CropReading::WiderDroppingBand) {
                        // Dropped when its pixels are cut by the crop or come near its edge;
                        // otherwise returned as read, larger, within the crop.
                        if readable_in(&pixels, r, &whole)
                            && let Some(bbox) = b.clip(r)
                        {
                            out.push(Word { bbox, text });
                        }
                        continue;
                    }
                }
                if let Some(bbox) = b.clip(r) {
                    if bbox != b {
                        text.push(CLIPPED);
                    }
                    out.push(Word { bbox, text });
                }
            }
        }
        Ok(out)
    }
}

/// What [`CropRecognizer`] appends to the text of a word it clipped.
const CLIPPED: char = '~';

/// The shape of the lab's caret scene (L6, S1) on a 640x320 frame: two words on a line of
/// a document whose four nested accessibility boxes contain every region.
fn document_scene() -> Scene {
    let element = |b: BBox, name: &str| SceneElement {
        bbox: b,
        name: name.into(),
        enabled: true,
    };
    Scene {
        w: 640,
        h: 320,
        surfaces: Vec::new(),
        controls: Vec::new(),
        words: vec![
            (bx(200.0, 150.0, 260.0, 166.0), "dos,".into(), [20, 20, 20]),
            (bx(400.0, 150.0, 450.0, 166.0), "tres:".into(), [21, 20, 20]),
        ],
        elements: vec![
            element(bx(0.0, 0.0, 640.0, 320.0), "Window"),
            element(bx(0.0, 20.0, 640.0, 320.0), "Pane"),
            element(bx(0.0, 40.0, 640.0, 310.0), "Document"),
            element(bx(10.0, 50.0, 630.0, 300.0), "Text"),
        ],
        carets: Vec::new(),
    }
}

/// Observes once (a full refresh), applies each change and observes again; returns the check
/// of each regional observation after the first, with its trace's count of accessibility
/// boxes cut.
fn checks(
    reading: CropReading,
    changes: &[&dyn Fn(&mut Scene)],
) -> Vec<(PlanCheck, usize, String)> {
    let shared = Arc::new(Mutex::new(document_scene()));
    let mut engine = scene_engine_reading(
        &shared,
        PlanParams::default(),
        Some(4),
        Box::new(CropRecognizer(shared.clone(), reading, 0)),
    );
    engine.set_plan_trace(true);
    engine.set_plan_check(true);
    engine.observe_screen(&ObserveRequest::default()).unwrap();
    let mut out = Vec::new();
    for change in changes {
        change(&mut scene_of(&shared));
        engine.observe_screen(&ObserveRequest::default()).unwrap();
        let trace = engine.last_plan_trace().unwrap();
        assert_eq!(trace.chosen, PlanChoice::Spanning, "{trace:?}");
        let cut = trace.spanning.unwrap().whole_on_read_cut;
        let check = *engine.last_plan_check().expect("a regional observation");
        out.push((check, cut, format!("{check:?} {trace:?}")));
    }
    out
}

/// The lab's line: one word differs on each side, both over the boxes R1 leaves out of the
/// closure, nothing else.
fn assert_lab_line(check: &PlanCheck, line: &str) {
    let words = CheckCount {
        at_spanned_or_whole_on_read: 1,
        elsewhere: 0,
    };
    let only = CheckKinds {
        words,
        ..CheckKinds::default()
    };
    assert_eq!(check.only_full, only, "{line}");
    assert_eq!(check.only_regional, only, "{line}");
    assert!(!check.full_truncated, "{line}");
}

fn caret_left_of_the_word(s: &mut Scene) {
    s.carets.push((150, 150));
}

/// The lab's line built with the fakes and the crop read one pixel lower: the word is inside
/// the region on both sides, the same text, another box. The limit L3.
#[test]
fn o3d_the_lab_line_with_a_word_read_lower_from_the_crop_is_l3() {
    let out = checks(CropReading::Lower, &[&caret_left_of_the_word]);
    let (check, cut, line) = &out[0];
    assert_eq!(*cut, 4, "{line}");
    assert_lab_line(check, line);
    let inside = WordPlaces {
        inside_a_region: 1,
        ..WordPlaces::default()
    };
    assert_eq!(check.words.only_full, inside, "{line}");
    assert_eq!(check.words.only_regional, inside, "{line}");
    assert_eq!(check.words.pairs.same_text_other_box, 1, "{line}");
    assert!(
        (check.words.pairs.largest_edge_difference_px - 1.0).abs() < 1e-9,
        "{line}"
    );
    assert_eq!(check.words.unexplained, Unexplained::default(), "{line}");
    assert_eq!(check.words.never_l3(), 0, "{line}");
}

/// The caret drawn right after the word, read as part of it from the crop: another text over
/// the dirty tile. The limit L3, at the place the screen changed.
#[test]
fn o3d_a_caret_read_into_the_word_from_the_crop_is_l3_at_the_dirty_tiles() {
    let out = checks(
        CropReading::CaretGlued,
        &[&|s: &mut Scene| {
            s.carets.push((263, 150));
        }],
    );
    let (check, cut, line) = &out[0];
    assert_eq!(*cut, 4, "{line}");
    assert_lab_line(check, line);
    assert_eq!(check.words.only_full.inside_a_region, 1, "{line}");
    assert_eq!(check.words.only_regional.inside_a_region, 1, "{line}");
    assert_eq!(check.words.pairs.other_text, 1, "{line}");
    assert_eq!(
        check.words.unexplained,
        Unexplained {
            at_dirty_tiles: 2,
            away_from_dirty_tiles: 0
        },
        "{line}"
    );
    assert_eq!(check.words.never_l3(), 0, "{line}");
}

/// UT-89 (ADR-0045 rule R5): the cut O3d built cannot happen any more. The first crop reads
/// the word one pixel tighter than its pixels and the snapshot holds that box; the next region
/// closes over the held box, so its edge passes through the word's last column. The crop,
/// grown by the margin, sees the word whole: it crosses the region's edge, the region widens
/// over it, and the committed word equals the full read's.
#[test]
fn ut_89_a_word_held_tighter_than_its_pixels_widens_the_region_instead_of_being_cut() {
    let shared = Arc::new(Mutex::new(document_scene()));
    let mut engine = scene_engine_reading(
        &shared,
        PlanParams::default(),
        Some(4),
        Box::new(CropRecognizer(shared.clone(), CropReading::TighterOnce, 0)),
    );
    engine.set_plan_check(true);
    engine.observe_screen(&ObserveRequest::default()).unwrap();
    caret_left_of_the_word(&mut scene_of(&shared));
    engine.observe_screen(&ObserveRequest::default()).unwrap();
    let held = |e: &Engine| {
        e.store
            .published()
            .unwrap()
            .evidence
            .words
            .iter()
            .find(|w| w.text == "dos,")
            .map(|w| w.bbox)
    };
    assert_eq!(held(&engine), Some(bx(200.0, 150.0, 259.0, 166.0)));
    scene_of(&shared).carets.clear();
    engine.observe_screen(&ObserveRequest::default()).unwrap();
    let edge = *engine.last_crop_edge().expect("a regional observation");
    assert_eq!(edge.crossing.words, 1, "{edge:?}");
    assert_eq!(edge.outcome, CropEdgeOutcome::Widened, "{edge:?}");
    assert_eq!(held(&engine), Some(bx(200.0, 150.0, 260.0, 166.0)));
    let check = engine.last_plan_check().expect("checked");
    assert_eq!(check.only_full.words, CheckCount::default(), "{check:?}");
    assert_eq!(
        check.only_regional.words,
        CheckCount::default(),
        "{check:?}"
    );
}

/// A window with lines of words, 30 to 70 px long, and a few controls, on a small frame.
fn word_scene(rng: &mut Rng) -> Scene {
    let (w, h) = (240, 180);
    let mut words = Vec::new();
    for i in 0..rng.between(4, 8) {
        let len = f64::from(rng.between(30, 70));
        let x = f64::from(rng.between(12, 228 - 70));
        let y = f64::from(rng.between(14, 160));
        words.push((
            bx(x, y, x + len, y + 12.0),
            format!("w{i}"),
            [20, 200 - 20 * i as u8, 200],
        ));
    }
    let controls = (0..rng.below(3))
        .map(|i| (rng.rect(w, h, 30), [200, 40 + 30 * i as u8, 40]))
        .collect();
    Scene {
        w,
        h,
        surfaces: vec![(bx(8.0, 8.0, 232.0, 172.0), [243, 243, 243])],
        controls,
        words,
        elements: Vec::new(),
        carets: Vec::new(),
    }
}

/// Lines of words 18 to 40 px long, most of them less than [`JOIN_GAP_PX`] apart, and a few
/// controls, on a small frame: lines that [`CropReading::Joined`] reads as long words.
fn line_scene(rng: &mut Rng) -> Scene {
    let (w, h) = (240, 180);
    let mut words = Vec::new();
    for line in 0..rng.between(3, 5) {
        let y = f64::from(14 + 30 * line + rng.below(10));
        let mut x = f64::from(rng.between(12, 60));
        while x < 200.0 {
            let len = f64::from(rng.between(18, 40)).min(228.0 - x);
            let n = words.len();
            words.push((
                bx(x, y, x + len, y + 12.0),
                format!("l{line}w{n}"),
                [20, 40 + 5 * (n % 30) as u8, 200],
            ));
            let gap = if rng.below(3) == 0 {
                rng.between(14, 24)
            } else {
                rng.between(3, 9)
            };
            x += len + f64::from(gap);
        }
    }
    let controls = (0..rng.below(3))
        .map(|i| (rng.rect(w, h, 30), [200, 40 + 30 * i as u8, 40]))
        .collect();
    Scene {
        w,
        h,
        surfaces: vec![(bx(8.0, 8.0, 232.0, 172.0), [243, 243, 243])],
        controls,
        words,
        elements: Vec::new(),
        carets: Vec::new(),
    }
}

/// Mostly a caret toggling just before or at the start of a word (its region then ends inside
/// the word's held box), sometimes any change of [`change`].
fn word_change(scene: &mut Scene, rng: &mut Rng, counter: &mut u8) {
    if scene.words.is_empty() || rng.below(4) == 0 {
        change(scene, rng, counter);
        return;
    }
    let (b, _, _) = &scene.words[rng.below(scene.words.len() as u32) as usize];
    let x = (px(b.x0()) as u32).saturating_sub(3) + rng.below(8);
    let y = px(b.y0()) as u32;
    match scene.carets.iter().position(|&c| c == (x, y)) {
        Some(i) => {
            scene.carets.remove(i);
        }
        None => scene.carets.push((x, y)),
    }
}

/// UT-89, the property: on generated scenes, with a recogniser that marks every word it
/// clipped at the edge of the area it is asked about and a detector that clips every control
/// there, no committed word is a clipped one, every committed detection is a box of the scene,
/// and the defence never commits more regions than the plan had. Three kinds of sequence: (i)
/// 40 of scattered words, the recogniser one pixel tight on every other crop (L3 drift, so a
/// held box can be tighter than the next reading of its pixels: the cut O3d built); (ii) 40 of
/// lines of close words that a crop reads joined, so a word reaches the crop's edge and past
/// it; (iii) 40 of lines of close words, the recogniser [`WIDER_PX`] wider and taller on
/// every other crop (right and down, then left and up: all four sides of the band, O3h) and
/// **dropping** every word near a crop's edge (O3g): the widening brings held
/// neighbours into the band, where the crop drops them; there no word of the scene may be
/// lost (each lies inside a committed word). Every fourth sequence of (i) and (ii) has a
/// tighter area budget. With this seed (O3h, the reading of (iii) larger on all four sides):
/// 982 regional observations (662 of (i)-(ii), as in O3f, and 320 of (iii)), the defence's
/// outcome none 573, widened 147, reread 193, full_refresh 69 (63 `closure_budget`, 6
/// `area_budget`); a final region within the band of its crop on 27 observations of (i)-(ii)
/// and 31 of (iii) (20 in O3g, horizontal only). The bounds below leave room under those
/// counts.
#[test]
#[allow(clippy::too_many_lines)] // one generator, three kinds of sequence, one property
fn ut_89_no_committed_word_or_detection_is_clipped_at_a_crop_edge() {
    let mut rng = Rng(0x5EED_0089);
    // Regional observations by the defence's outcome: none, widened, reread, full_refresh.
    let mut outcomes = [0usize; 4];
    // Full refreshes of the defence by reason: closure_budget, area_budget.
    let mut full_by = [0usize; 2];
    // Regional observations of the dropping kind (iii), and how many observations of kinds
    // (i)-(ii) and of kind (iii) had a final region within the band of its crop (O3g).
    let (mut dropping_regional, mut in_band) = (0usize, [0usize; 2]);
    for sequence in 0..120 {
        // The first 80 as in O3f. Even ones: scattered words and a recogniser one pixel tight
        // on every other crop (the cut O3d built). Odd ones: lines read joined from a crop,
        // which reach the crop's edge (the second read) and past the second crop's (the full
        // refresh). Every fourth has a tighter area budget, which a widening can break. The
        // last 40: lines read wider and dropped near a crop's edge (O3g).
        let dropping = sequence >= 80;
        let lines = dropping || sequence % 2 == 1;
        let params = PlanParams {
            max_area_fraction: if !dropping && sequence % 4 == 3 {
                0.3
            } else {
                0.6
            },
            ..small_params()
        };
        let scene = if lines {
            line_scene(&mut rng)
        } else {
            word_scene(&mut rng)
        };
        let reading = if dropping {
            CropReading::WiderDroppingBand
        } else if lines {
            CropReading::Joined
        } else {
            CropReading::TighterAlternately
        };
        let shared = Arc::new(Mutex::new(scene));
        let mut engine = scene_engine_reading(
            &shared,
            params,
            Some(4),
            Box::new(CropRecognizer(shared.clone(), reading, 0)),
        );
        engine.set_plan_trace(true);
        let mut seq_rng = Rng(rng.next_u64());
        let mut counter = 0u8;
        for step in 0..10 {
            engine.observe_screen(&ObserveRequest::default()).unwrap();
            let at = format!("sequence {sequence}, step {step}");
            let scene = scene_of(&shared).clone();
            let e = &engine.store.published().unwrap().evidence;
            for w in &e.words {
                assert!(!w.text.ends_with(CLIPPED), "{at}: clipped word {w:?}");
            }
            for c in &e.controls {
                let ok = scene.controls.iter().any(|(b, _)| *b == c.bbox)
                    || scene.surfaces.iter().any(|(b, _)| *b == c.bbox);
                assert!(ok, "{at}: clipped detection {c:?}");
            }
            if dropping {
                for (b, text, _) in &scene.words {
                    assert!(
                        e.words.iter().any(|w| w.bbox.contains(b)),
                        "{at}: the scene's word {text} at {b:?} was lost"
                    );
                }
            }
            if let Some(edge) = engine.last_crop_edge() {
                dropping_regional += usize::from(dropping);
                in_band[usize::from(dropping)] += usize::from(edge.regions_in_band > 0);
                outcomes[match edge.outcome {
                    CropEdgeOutcome::None => 0,
                    CropEdgeOutcome::Widened => 1,
                    CropEdgeOutcome::Reread => 2,
                    CropEdgeOutcome::FullRefresh => 3,
                }] += 1;
                if edge.outcome == CropEdgeOutcome::FullRefresh {
                    match engine.last_plan_trace().unwrap().decided_by {
                        Some(FullRefreshReason::ClosureBudget) => full_by[0] += 1,
                        Some(FullRefreshReason::AreaBudget) => full_by[1] += 1,
                        other => panic!("{at}: a full refresh of R5 decided by {other:?}"),
                    }
                }
                // The widening never adds a region: the budgets on the number of regions,
                // which the plan passed, cannot be broken by R5.
                if edge.outcome != CropEdgeOutcome::FullRefresh {
                    let trace = engine.last_plan_trace().unwrap();
                    let planned = trace
                        .spanning
                        .as_ref()
                        .and_then(|s| s.classic.as_ref())
                        .map_or(trace.regions.len(), |c| c.region_count);
                    let committed = engine.last_plan().unwrap().region_count;
                    assert!(committed <= planned, "{at}: {committed} > {planned}");
                }
            }
            let mut s = scene_of(&shared);
            let before = s.render();
            for _ in 0..50 {
                let saved = s.clone();
                word_change(&mut s, &mut seq_rng, &mut counter);
                if s.render().pixels() != before.pixels() {
                    break;
                }
                *s = saved;
            }
        }
    }
    let [none, widened, reread, full] = outcomes;
    let regional: usize = outcomes.iter().sum();
    let [closure, area] = full_by;
    eprintln!(
        "UT-89: {regional} regional observations: none {none}, widened {widened}, \
         reread {reread}, full_refresh {full} (closure_budget {closure}, area_budget {area}); \
         a final region in the band on {} of kinds (i)-(ii) and {} of kind (iii) \
         ({dropping_regional} regional)",
        in_band[0], in_band[1]
    );
    assert!(regional >= 500, "only {regional} regional observations");
    assert!(widened >= 40, "{outcomes:?}");
    assert!(reread >= 45, "{outcomes:?}");
    assert!(full >= 45, "{outcomes:?}");
    assert!(closure >= 40 && area >= 3, "{full_by:?}");
    assert!(
        dropping_regional >= 250 && in_band[1] >= 12,
        "{dropping_regional} {in_band:?}"
    );
}

/// A word held from an earlier crop and read otherwise by the full read, away from every
/// region of this observation: outside, never L3 of this observation.
#[test]
fn o3d_a_word_held_from_an_earlier_crop_is_outside_every_region() {
    let out = checks(
        CropReading::Tighter,
        &[&caret_left_of_the_word, &|s: &mut Scene| {
            s.carets.push((560, 260));
        }],
    );
    let (check, _, line) = &out[1];
    let outside = WordPlaces {
        outside_every_region: 1,
        ..WordPlaces::default()
    };
    assert_eq!(check.words.only_full, outside, "{line}");
    assert_eq!(check.words.only_regional, outside, "{line}");
    assert_eq!(check.words.pairs.same_text_other_box, 1, "{line}");
    assert_eq!(check.words.never_l3(), 2, "{line}");
    assert_eq!(check.only_full.words.elsewhere, 0, "{line}");
}

/// The check holds counts, closed classes and one distance: no text of the screen and no
/// coordinate, whatever the words were.
#[test]
fn o3d_the_word_detail_holds_no_text_and_no_coordinate() {
    let mut seen: Vec<WordDetail> = Vec::new();
    for reading in [
        CropReading::Lower,
        CropReading::CaretGlued,
        CropReading::Tighter,
    ] {
        let out = checks(
            reading,
            &[
                &caret_left_of_the_word,
                &|s: &mut Scene| s.carets.clear(),
                &|s: &mut Scene| s.carets.push((263, 150)),
                &|s: &mut Scene| s.carets.push((560, 260)),
            ],
        );
        seen.extend(out.iter().map(|(c, _, _)| c.words));
    }
    let scene = document_scene();
    for detail in &seen {
        let text = format!("{detail:?}");
        for (b, word, _) in &scene.words {
            assert!(!text.contains(word.trim_end_matches([',', ':'])), "{text}");
            for v in [b.x0(), b.y0(), b.x1(), b.y1()] {
                assert!(!text.contains(&format!("{v:?}")), "{v} in {text}");
            }
        }
        // The one number that is not a count is a distance of a few pixels.
        assert!(detail.pairs.largest_edge_difference_px <= 8.0, "{text}");
    }
    assert!(seen.iter().any(|d| d.never_l3() > 0));
    assert!(seen.iter().any(|d| d.pairs.other_text > 0));
}

// ---- O3f: each check of the crop-edge defence pinned by a scene of its own ----
//
// The frame is 640x320 with the default plan (tiles of 64, halo 32). A caret at (100, 100)
// dirties the tile (64..128, 64..128), so the planned region is (32..160, 32..160), 16 384 px2,
// and its crop is (0..192, 0..192): its left and top edges are the frame's (exempt), its right
// and bottom edges are not. The recogniser's crop readings are scripted: each stands for what
// a real recogniser can return from a crop (a word read wider than the snapshot holds it, a
// word it dropped because the crop cut it).

/// Reads the scene's words exactly from the whole frame. From the `i`-th crop read it returns
/// `script[i]` when given (words with those boxes), and otherwise every word of the scene that
/// lies wholly inside an area it was asked about: a recogniser that drops what a crop cuts.
/// Records the areas of every read.
struct ScriptedRecognizer {
    scene: SharedScene,
    script: Vec<Option<Vec<BBox>>>,
    reads: Arc<Mutex<Vec<Vec<BBox>>>>,
}

impl TextRecognizer for ScriptedRecognizer {
    fn name(&self) -> &'static str {
        "scene-ocr-scripted"
    }

    fn recognize(&mut self, frame: &BgraFrame, regions: &[BBox]) -> Result<Vec<Word>, PortError> {
        let s = scene_of(&self.scene);
        let whole = frame.bounds();
        let crop_reads = self
            .reads
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .filter(|r| r.as_slice() != [whole])
            .count();
        if !regions.is_empty() {
            self.reads
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(regions.to_vec());
        }
        let word = |bbox: BBox, text: &str| Word {
            bbox,
            text: text.into(),
        };
        if regions == [whole] {
            return Ok(s.words.iter().map(|(b, t, _)| word(*b, t)).collect());
        }
        if let Some(Some(boxes)) = self.script.get(crop_reads) {
            return Ok(boxes
                .iter()
                .map(|b| word(*b, &format!("read{crop_reads}")))
                .collect());
        }
        Ok(s.words
            .iter()
            .filter(|(b, _, _)| regions.iter().any(|r| r.contains(b)))
            .map(|(b, t, _)| word(*b, t))
            .collect())
    }
}

/// Every control of the scene, as an `Input` (a role that takes an external label, so fusion
/// relates it to a word on its left), cut at the edge of each area it is asked about. No
/// surfaces and no band: the classic plan, which R5 defends as well.
struct InputDetector(SharedScene);

impl VisionDetector for InputDetector {
    fn name(&self) -> &'static str {
        "scene-detector-inputs"
    }

    fn models_loaded(&self) -> bool {
        false
    }

    fn detect(&mut self, _: &BgraFrame, regions: &[BBox]) -> Result<Vec<Control>, PortError> {
        let s = scene_of(&self.0);
        Ok(s.controls
            .iter()
            .flat_map(|(b, _)| regions.iter().filter_map(|r| b.clip(r)))
            .map(|bbox| Control {
                bbox,
                role: Role::Input,
                affordance: Affordance {
                    actionability: Actionability::Unknown,
                    enabled: true,
                    visible: true,
                },
            })
            .collect())
    }
}

fn empty_scene() -> Scene {
    Scene {
        w: 640,
        h: 320,
        surfaces: Vec::new(),
        controls: Vec::new(),
        words: Vec::new(),
        elements: Vec::new(),
        carets: Vec::new(),
    }
}

/// Observes `scene` once (a full refresh), draws the caret at (100, 100) and observes again
/// with the scripted crop readings. Returns the engine and the areas of every recogniser read
/// of the second observation.
fn caret_observation(
    scene: Scene,
    params: PlanParams,
    script: Vec<Option<Vec<BBox>>>,
) -> (Engine, Vec<Vec<BBox>>) {
    let shared = Arc::new(Mutex::new(scene));
    let reads = Arc::new(Mutex::new(Vec::new()));
    let mut engine = scene_engine_with(
        &shared,
        params,
        Box::new(InputDetector(shared.clone())),
        Box::new(ScriptedRecognizer {
            scene: shared.clone(),
            script: Vec::new(),
            reads: reads.clone(),
        }),
    );
    engine.set_plan_trace(true);
    engine.observe_screen(&ObserveRequest::default()).unwrap();
    assert_eq!(engine.last_plan().unwrap().work, WorkKind::Full);
    reads.lock().unwrap_or_else(PoisonError::into_inner).clear();
    engine.ports.recognizer = Box::new(ScriptedRecognizer {
        scene: shared.clone(),
        script,
        reads: reads.clone(),
    });
    scene_of(&shared).carets.push((100, 100));
    engine.observe_screen(&ObserveRequest::default()).unwrap();
    let trace = engine.last_plan_trace().unwrap();
    assert_eq!(trace.regions.len(), 1, "{trace:?}");
    assert!(
        (trace.covered_area_px - 16_384.0).abs() < 1e-9,
        "the planned region is (32..160, 32..160): {trace:?}"
    );
    let seen = reads.lock().unwrap_or_else(PoisonError::into_inner).clone();
    (engine, seen)
}

fn decided(engine: &Engine) -> (Option<FullRefreshReason>, Option<usize>) {
    let t = engine.last_plan_trace().unwrap();
    (t.decided_by, t.decisive_region)
}

/// O3f, rule R5's second read: the first crop reads a word that crosses the region and ends
/// 7 px from the crop's right edge, so the region widens and the observation reads again; the
/// second crop reads one more word 7 px from its own right edge. A second (b) is a full
/// refresh with `closure_budget`, and the trace's `decided_by` says so. There is no third
/// regional read.
#[test]
fn o3f_a_second_read_that_still_finds_a_word_near_the_crops_edge_is_a_full_refresh() {
    let (engine, reads) = caret_observation(
        empty_scene(),
        PlanParams::default(),
        vec![
            Some(vec![bx(150.0, 100.0, 185.0, 112.0)]),
            // The second crop is the widened region (32..185) grown by 32: (0..217, 0..192).
            Some(vec![bx(180.0, 100.0, 210.0, 112.0)]),
            // What a third regional read would return: nothing near any edge.
            Some(Vec::new()),
        ],
    );
    let edge = *engine.last_crop_edge().unwrap();
    assert_eq!(edge.outcome, CropEdgeOutcome::FullRefresh, "{edge:?}");
    assert_eq!(edge.rereads, 1, "{edge:?}");
    assert_eq!(edge.crossing.words, 1, "{edge:?}");
    assert_eq!(edge.near_crop_edge.words, 1, "{edge:?}");
    let frame = bx(0.0, 0.0, 640.0, 320.0);
    assert_eq!(
        reads,
        vec![
            vec![bx(0.0, 0.0, 192.0, 192.0)],
            vec![bx(0.0, 0.0, 217.0, 192.0)],
            vec![frame]
        ],
        "two crop reads, then the whole frame"
    );
    let plan = engine.last_plan().unwrap();
    assert_eq!(plan.work, WorkKind::Full);
    assert_eq!(
        plan.full_refresh_reason,
        Some(FullRefreshReason::ClosureBudget)
    );
    assert_eq!(
        decided(&engine),
        (Some(FullRefreshReason::ClosureBudget), None)
    );
}

/// O3f, the budgets on the final regions: the planned region (16 384 px2) is under an area
/// budget of 0.085 of the frame (17 408 px2); a word the crop reads across its right edge,
/// clear of the crop's band, widens it to (32..175, 32..160), 18 304 px2, over the budget. The
/// observation is a full refresh with `area_budget`, decided by R5, where without the check
/// the widened region would have been committed. The two budgets on the number of regions
/// (`max_regions`, `adaptive_min_regions`) cannot be broken here: the widening only grows and
/// merges regions, so the final regions are never more than the planned ones, which passed
/// both (UT-89's property checks the count on every regional observation).
#[test]
fn o3f_widened_regions_over_the_area_budget_make_a_full_refresh() {
    let (engine, reads) = caret_observation(
        empty_scene(),
        PlanParams {
            max_area_fraction: 0.085,
            ..PlanParams::default()
        },
        vec![Some(vec![bx(150.0, 100.0, 175.0, 112.0)])],
    );
    let edge = *engine.last_crop_edge().unwrap();
    assert_eq!(edge.outcome, CropEdgeOutcome::FullRefresh, "{edge:?}");
    assert_eq!(edge.rereads, 0, "{edge:?}");
    assert_eq!(edge.crossing.words, 1, "{edge:?}");
    assert_eq!(edge.near_crop_edge, CropEdgeKinds::default(), "{edge:?}");
    assert_eq!(
        reads.len(),
        2,
        "one crop read, then the whole frame: {reads:?}"
    );
    let plan = engine.last_plan().unwrap();
    assert_eq!(plan.work, WorkKind::Full);
    assert_eq!(
        plan.full_refresh_reason,
        Some(FullRefreshReason::AreaBudget)
    );
    assert_eq!(
        decided(&engine),
        (Some(FullRefreshReason::AreaBudget), None)
    );
}

/// O3f, a final region that left its crop: the snapshot holds a word at (170..260, 120..132),
/// past the planned region. The first crop reads another word across the region's edge and
/// over the held one's first two pixels, and drops the held word, which the crop cuts. The
/// widening closes over the held word and leaves the crop; no fresh item came near the
/// crop's edge, so only that check sees it. The observation reads again with a crop that
/// holds the held word, which is read whole.
#[test]
fn o3f_a_final_region_that_left_its_crop_is_read_again() {
    let held = bx(170.0, 120.0, 260.0, 132.0);
    let mut scene = empty_scene();
    scene.words.push((held, "held".into(), [20, 20, 20]));
    let (engine, reads) = caret_observation(
        scene,
        PlanParams::default(),
        vec![Some(vec![bx(150.0, 120.0, 172.0, 132.0)])],
    );
    let edge = *engine.last_crop_edge().unwrap();
    assert_eq!(edge.outcome, CropEdgeOutcome::Reread, "{edge:?}");
    assert_eq!(edge.rereads, 1, "{edge:?}");
    assert_eq!(edge.crossing.words, 1, "{edge:?}");
    // The region that left its crop is counted as a detection; no word came near the edge.
    assert_eq!(
        edge.near_crop_edge,
        CropEdgeKinds {
            words: 0,
            detections: 1
        },
        "{edge:?}"
    );
    assert_eq!(
        reads,
        vec![
            vec![bx(0.0, 0.0, 192.0, 192.0)],
            vec![bx(0.0, 0.0, 292.0, 192.0)]
        ]
    );
    let plan = engine.last_plan().unwrap();
    assert_eq!(plan.work, WorkKind::Regions);
    assert!(
        (plan.region_area_px - 228.0 * 128.0).abs() < 1e-9,
        "{plan:?}"
    );
    let words = &engine.store.published().unwrap().evidence.words;
    assert_eq!(words.len(), 1, "{words:?}");
    assert_eq!((words[0].bbox, words[0].text.as_str()), (held, "held"));
}

/// O3f, a held relation between two cut-prone boxes: the snapshot holds a label word at
/// (168..200, 100..112) and, on its right, an input at (230..330, 98..114) that fusion relates
/// to it. Neither meets the planned region. The crop reads a word across the region's edge
/// and over the label; the widening closes over the label, and the relation pulls in the
/// input although no region meets it. The second read's crop then holds the input whole.
#[test]
fn o3f_a_held_relation_pulls_its_other_member_into_the_widening() {
    let label = bx(168.0, 100.0, 200.0, 112.0);
    let input = bx(230.0, 98.0, 330.0, 114.0);
    let mut scene = empty_scene();
    scene.words.push((label, "Name:".into(), [20, 20, 20]));
    scene.controls.push((input, [200, 200, 200]));
    let (engine, reads) = caret_observation(
        scene,
        PlanParams::default(),
        vec![Some(vec![bx(150.0, 100.0, 170.0, 112.0)])],
    );
    let held = engine.store.published().unwrap();
    assert_eq!(held.relations, vec![(input, label)], "the relation is held");
    let edge = *engine.last_crop_edge().unwrap();
    assert_eq!(edge.outcome, CropEdgeOutcome::Reread, "{edge:?}");
    // The final region is (32..330, 32..160): the label and the input it is related to.
    assert!(
        (edge.final_area_px - 298.0 * 128.0).abs() < 1e-9,
        "{edge:?}"
    );
    assert_eq!(reads[1], vec![bx(0.0, 0.0, 362.0, 192.0)]);
    let e = &held.evidence;
    assert_eq!(
        e.controls.iter().map(|c| c.bbox).collect::<Vec<_>>(),
        [input]
    );
    assert_eq!(e.words.iter().map(|w| w.bbox).collect::<Vec<_>>(), [label]);
}

// ---- O3g: a held item the widening brings into a crop's band, which the crop drops ----
//
// The same 640x320 scene and caret as O3f. A held word W at (150..170, 100..112) meets the
// tile's region (32..160, 32..160), so the planner closes over it: the planned region is
// (32..170, 32..160) and its crop (0..202, 0..192), whose right edge is 202.

/// The band of the dropping fakes below, written here rather than taken from the engine, so
/// that a mutant of the engine's band does not change what the fakes read.
const FAKE_BAND_PX: f64 = 16.0;

/// Whether a fake reading `area` of `frame` returns an item at `b`: only one inside the area
/// and at least [`FAKE_BAND_PX`] from each of its edges that is not the frame's edge. What a
/// recogniser or a detector does when it loses an item a crop's edge cuts or comes close to.
fn readable_in(b: &BBox, area: &BBox, frame: &BBox) -> bool {
    area.contains(b)
        && (area.x0() <= frame.x0() || b.x0() - area.x0() >= FAKE_BAND_PX)
        && (area.y0() <= frame.y0() || b.y0() - area.y0() >= FAKE_BAND_PX)
        && (area.x1() >= frame.x1() || area.x1() - b.x1() >= FAKE_BAND_PX)
        && (area.y1() >= frame.y1() || area.y1() - b.y1() >= FAKE_BAND_PX)
}

/// Reads the scene's words; on the `i`-th crop read, the words `script[i]` names (by their
/// index in the scene) at the boxes it gives and the others at their own; and it drops every
/// word that is not [`readable_in`] an area it was asked about. Records every read's areas.
struct BandDropRecognizer {
    scene: SharedScene,
    script: Vec<Vec<(usize, BBox)>>,
    crop_reads: usize,
    reads: Arc<Mutex<Vec<Vec<BBox>>>>,
}

impl TextRecognizer for BandDropRecognizer {
    fn name(&self) -> &'static str {
        "scene-ocr-band-drop"
    }

    fn recognize(&mut self, frame: &BgraFrame, regions: &[BBox]) -> Result<Vec<Word>, PortError> {
        if regions.is_empty() {
            return Ok(Vec::new());
        }
        let s = scene_of(&self.scene);
        let whole = frame.bounds();
        self.reads
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(regions.to_vec());
        let script = if regions == [whole] {
            Vec::new()
        } else {
            self.crop_reads += 1;
            self.script
                .get(self.crop_reads - 1)
                .cloned()
                .unwrap_or_default()
        };
        Ok(s.words
            .iter()
            .enumerate()
            .filter_map(|(i, (b, text, _))| {
                let b = script.iter().find(|(j, _)| *j == i).map_or(*b, |(_, r)| *r);
                regions
                    .iter()
                    .any(|r| readable_in(&b, r, &whole))
                    .then(|| Word {
                        bbox: b,
                        text: text.clone(),
                    })
            })
            .collect())
    }
}

/// Every control of the scene, as an `Input`, dropped unless [`readable_in`] an area it is
/// asked about. No surfaces and no band: the classic plan.
struct BandDropDetector(SharedScene);

impl VisionDetector for BandDropDetector {
    fn name(&self) -> &'static str {
        "scene-detector-band-drop"
    }

    fn models_loaded(&self) -> bool {
        false
    }

    fn detect(&mut self, frame: &BgraFrame, regions: &[BBox]) -> Result<Vec<Control>, PortError> {
        let s = scene_of(&self.0);
        let whole = frame.bounds();
        Ok(s.controls
            .iter()
            .filter(|(b, _)| regions.iter().any(|r| readable_in(b, r, &whole)))
            .map(|(bbox, _)| Control {
                bbox: *bbox,
                role: Role::Input,
                affordance: Affordance {
                    actionability: Actionability::Unknown,
                    enabled: true,
                    visible: true,
                },
            })
            .collect())
    }
}

/// Observes `scene` (a full refresh), draws the caret at (100, 100) and observes again with
/// the dropping fakes and the scripted crop readings. Returns the engine and the areas of
/// every recogniser read of the second observation.
fn dropping_observation(scene: Scene, script: Vec<Vec<(usize, BBox)>>) -> (Engine, Vec<Vec<BBox>>) {
    let shared = Arc::new(Mutex::new(scene));
    let reads = Arc::new(Mutex::new(Vec::new()));
    let mut engine = scene_engine_with(
        &shared,
        PlanParams::default(),
        Box::new(BandDropDetector(shared.clone())),
        Box::new(BandDropRecognizer {
            scene: shared.clone(),
            script,
            crop_reads: 0,
            reads: reads.clone(),
        }),
    );
    engine.set_plan_trace(true);
    engine.observe_screen(&ObserveRequest::default()).unwrap();
    assert_eq!(engine.last_plan().unwrap().work, WorkKind::Full);
    reads.lock().unwrap_or_else(PoisonError::into_inner).clear();
    scene_of(&shared).carets.push((100, 100));
    engine.observe_screen(&ObserveRequest::default()).unwrap();
    let trace = engine.last_plan_trace().unwrap();
    assert_eq!(trace.regions.len(), 1, "{trace:?}");
    assert!(
        (trace.covered_area_px - 138.0 * 128.0).abs() < 1e-9,
        "the planned region is (32..170, 32..160): {trace:?}"
    );
    let seen = reads.lock().unwrap_or_else(PoisonError::into_inner).clone();
    (engine, seen)
}

/// O3g: the crop reads W 4 px wider, across the region's edge (174), clear of the crop's
/// band; the widening meets the held word H at (172..190, 100..112), which ends 12 px from
/// the crop's right edge, and the recogniser drops H there. No fresh item comes near the
/// crop's edge and the final region (32..190) stays inside its crop, so before O3g nothing
/// read again and the merge dropped H: the held word was lost. Now the final region within
/// the band makes the one re-read, whose crop (0..222) reads H whole.
#[test]
fn o3g_a_held_word_the_widening_brings_into_the_crops_band_is_read_again_not_lost() {
    let w = bx(150.0, 100.0, 170.0, 112.0);
    let h = bx(172.0, 100.0, 190.0, 112.0);
    let mut scene = empty_scene();
    scene.words.push((w, "Linea".into(), [20, 20, 20]));
    scene.words.push((h, "tres".into(), [20, 20, 20]));
    let (engine, reads) =
        dropping_observation(scene, vec![vec![(0, bx(150.0, 100.0, 174.0, 112.0))]]);
    let words: Vec<(BBox, &str)> = engine
        .store
        .published()
        .unwrap()
        .evidence
        .words
        .iter()
        .map(|w| (w.bbox, w.text.as_str()))
        .collect();
    assert_eq!(
        words,
        [(w, "Linea"), (h, "tres")],
        "the held word H is kept"
    );
    let edge = *engine.last_crop_edge().unwrap();
    assert_eq!(edge.outcome, CropEdgeOutcome::Reread, "{edge:?}");
    assert_eq!(edge.crossing.words, 1, "{edge:?}");
    // Neither earlier trigger fired: no fresh item near the edge, no region out of its crop.
    assert_eq!(edge.near_crop_edge, CropEdgeKinds::default(), "{edge:?}");
    assert_eq!(edge.regions_in_band, 1, "{edge:?}");
    assert_eq!(
        reads,
        vec![
            vec![bx(0.0, 0.0, 202.0, 192.0)],
            vec![bx(0.0, 0.0, 222.0, 192.0)]
        ]
    );
    let plan = engine.last_plan().unwrap();
    assert_eq!(plan.work, WorkKind::Regions);
    assert!(
        (plan.region_area_px - 158.0 * 128.0).abs() < 1e-9,
        "{plan:?}"
    );
}

/// O3h, the stated limit L4 pinned (ADR-0045): the held word W lies on the planned region's
/// edge (32..170); the crop reads it joined with a neighbour, to (150..190), 12 px from the
/// crop's right edge (202), and the recogniser drops that reading. Nothing fresh is returned,
/// so (a) does not widen, the final region stays 32 px clear of the crop's edge and (b) does
/// not fire: the merge replaces W with nothing, until the next full refresh. This test says
/// what the engine does today; if a later rule closes the limit, it must change.
#[test]
fn o3h_a_word_read_past_the_band_and_dropped_is_the_stated_limit_l4() {
    let w = bx(150.0, 100.0, 170.0, 112.0);
    let mut scene = empty_scene();
    scene.words.push((w, "Linea".into(), [20, 20, 20]));
    let (engine, reads) =
        dropping_observation(scene, vec![vec![(0, bx(150.0, 100.0, 190.0, 112.0))]]);
    let edge = *engine.last_crop_edge().unwrap();
    assert_eq!(edge.outcome, CropEdgeOutcome::None, "{edge:?}");
    assert_eq!(edge.regions_in_band, 0, "{edge:?}");
    assert_eq!(edge.near_crop_edge, CropEdgeKinds::default(), "{edge:?}");
    assert_eq!(reads, vec![vec![bx(0.0, 0.0, 202.0, 192.0)]]);
    let words = &engine.store.published().unwrap().evidence.words;
    assert!(words.is_empty(), "W is lost, the limit L4: {words:?}");
}

/// O3g, the same through a held relation and the detector: the widening meets a held label L
/// at (172..180, 100..112), clear of the band and read again; the relation pulls in its input
/// I at (184..196, 98..114), 6 px from the crop's right edge, which the detector drops there.
/// Before O3g the input was lost; now the re-read's crop (0..228) detects it whole.
#[test]
fn o3g_a_held_input_a_relation_brings_into_the_crops_band_is_read_again_not_lost() {
    let w = bx(150.0, 100.0, 170.0, 112.0);
    let label = bx(172.0, 100.0, 180.0, 112.0);
    let input = bx(184.0, 98.0, 196.0, 114.0);
    let mut scene = empty_scene();
    scene.words.push((w, "x".into(), [20, 20, 20]));
    scene.words.push((label, "Id:".into(), [20, 20, 20]));
    scene.controls.push((input, [200, 200, 200]));
    let (engine, reads) =
        dropping_observation(scene, vec![vec![(0, bx(150.0, 100.0, 174.0, 112.0))]]);
    let held = engine.store.published().unwrap();
    assert_eq!(
        held.evidence
            .controls
            .iter()
            .map(|c| c.bbox)
            .collect::<Vec<_>>(),
        [input],
        "the held input is kept"
    );
    assert!(
        held.relations.contains(&(input, label)),
        "{:?}",
        held.relations
    );
    let edge = *engine.last_crop_edge().unwrap();
    assert_eq!(edge.outcome, CropEdgeOutcome::Reread, "{edge:?}");
    assert_eq!(edge.near_crop_edge, CropEdgeKinds::default(), "{edge:?}");
    assert_eq!(edge.regions_in_band, 1, "{edge:?}");
    assert_eq!(
        reads,
        vec![
            vec![bx(0.0, 0.0, 202.0, 192.0)],
            vec![bx(0.0, 0.0, 228.0, 192.0)]
        ]
    );
}
