//! The support-aware closure in the engine (ADR-0045): what the merge keeps (UT-84) and the
//! differential test of a regional observation against a full read of the same frames
//! (UT-85).

use super::*;

use crate::contract::ElementView;
use crate::engine::observe::EvidenceSet;
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
    let world = Arc::new(Mutex::new(World::default()));
    let clock = Arc::new(ManualClock::new());
    let operator_stop = lock(&world).operator_stop.clone();
    let mut config = EngineConfig::new(SESSION);
    config.plan = params;
    Engine::new(
        config,
        Ports {
            capturer: Box::new(SceneCapturer(scene.clone(), clock.clone(), 0)),
            detector: Box::new(SceneDetector(scene.clone(), Vec::new(), band)),
            recognizer: Box::new(SceneRecognizer(scene.clone())),
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
