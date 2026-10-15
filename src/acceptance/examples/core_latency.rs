//! PERF-01 (plan_pruebas.csv, RNF-04): the latency of the Ommatix core on a synthetic screen
//! of 300 controls and 600 text spans, with no detector, no OCR, no transport and no
//! screen: Algorithm 1 (dirty tiles and region plan), fusion, identity and the snapshot
//! commit, alone and as the whole `Engine::observe_screen` over instant port doubles.
//!
//! ```text
//! cargo run --release -p ommatix-acceptance --example core_latency -- --out perf01.json
//! ```
//!
//! Reproducible by construction: the scene is generated, not captured (nothing here reads
//! the screen or sends input), the same arguments build the same scene, and the report
//! names the build, the machine and the commit. Percentiles are nearest-rank over the
//! samples after a declared warm-up. Chapter 09 §9.6: the percentiles of the stages are
//! not added up; the composite is timed as one block. It refuses to run unoptimised.

// The benchmark's report is its output: it may print to stdout (the workspace denies it for
// the MCP server).
#![allow(clippy::print_stdout)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use ommatix_application::contract::ObserveRequest;
use ommatix_application::{
    CapturedFrame, Clock, Engine, EngineConfig, FrameCapturer, LocalPolicy, NoAccessibility,
    OsActuator, PolicyStore, PortError, Ports, TextRecognizer, VisionDetector,
};
use ommatix_domain::bbox::BBox;
use ommatix_domain::fusion::{
    Actionability, Affordance, Control, Fusion, FusionParams, LabelChoice, Role, Word, fuse,
};
use ommatix_domain::identity::{AssociationPolicy, Observation, Registry};
use ommatix_domain::incremental::{BgraFrame, Evidence, PlanParams, Work, plan_regions};
use ommatix_domain::snapshot::SnapshotStore;
use serde_json::{Value, json};

// ---- the synthetic screen ----

const WIDTH: u32 = 1920;
const HEIGHT: u32 = 1080;
const COLUMNS: usize = 20;
const CELL: (f64, f64) = (96.0, 72.0);
/// The scene of RNF-04.
const CONTROLS: usize = 300;
const SPANS: usize = 600;
const EPOCH: u64 = 1;

fn bx(x0: f64, y0: f64, x1: f64, y1: f64) -> BBox {
    BBox::new(x0, y0, x1, y1).expect("the generated boxes are valid")
}

/// The role of control `i`: six kinds in turn.
const fn role_of(i: usize) -> Role {
    match i % 6 {
        0 => Role::Button,
        1 => Role::Input,
        2 => Role::Checkbox,
        3 => Role::Dropdown,
        4 => Role::IconButton,
        _ => Role::Unknown,
    }
}

/// What a detector and an OCR engine would report for the screen, and the pixels.
#[derive(Debug, Clone)]
struct Scene {
    controls: Vec<Control>,
    words: Vec<Word>,
    frame: BgraFrame,
}

fn cell_origin(i: usize) -> (f64, f64) {
    ((i % COLUMNS) as f64 * CELL.0, (i / COLUMNS) as f64 * CELL.1)
}

/// The scene with `moved` cells repainted and their words shifted by two pixels (a small
/// change that is not a different screen): `moved = []` is the base screen.
fn scene(moved: &[usize]) -> Scene {
    let mut controls = Vec::with_capacity(CONTROLS);
    let mut words = Vec::with_capacity(SPANS);
    let mut free_slots = Vec::new();
    for i in 0..CONTROLS {
        let (x, y) = cell_origin(i);
        let shift = if moved.contains(&i) { 2.0 } else { 0.0 };
        let role = role_of(i);
        let confirmed = matches!(
            role,
            Role::Button | Role::Input | Role::Checkbox | Role::Dropdown
        );
        controls.push(Control {
            bbox: bx(x + 8.0, y + 6.0, x + 88.0, y + 38.0),
            role,
            affordance: Affordance {
                actionability: if confirmed {
                    Actionability::Confirmed
                } else {
                    Actionability::Unknown
                },
                enabled: true,
                visible: true,
            },
        });
        match role {
            // Text inside the control.
            Role::Button | Role::Dropdown | Role::Unknown => words.push(Word::new(
                format!("Item {i}"),
                bx(x + 14.0 + shift, y + 12.0, x + 82.0 + shift, y + 32.0),
            )),
            // A label under the control.
            Role::Input | Role::Checkbox => words.push(Word::new(
                format!("Label {i}"),
                bx(x + 8.0 + shift, y + 40.0, x + 60.0 + shift, y + 54.0),
            )),
            _ => free_slots.push(i),
        }
        words.push(Word::new(
            format!("Note {i}"),
            bx(x + 8.0 + shift, y + 58.0, x + 88.0 + shift, y + 70.0),
        ));
    }
    // Free text up to the 600 spans of the scene, in the cells that have room for it.
    for i in free_slots.iter().copied().cycle().take(SPANS - words.len()) {
        let (x, y) = cell_origin(i);
        let shift = if moved.contains(&i) { 2.0 } else { 0.0 };
        words.push(Word::new(
            format!("Tag {i} {}", words.len()),
            bx(x + 20.0 + shift, y + 40.0, x + 70.0 + shift, y + 54.0),
        ));
    }
    Scene {
        controls,
        words,
        frame: paint(moved),
    }
}

/// Pixels: a flat background, a coloured box per control, and a bright 24x24 mark in each
/// of the `moved` cells.
fn paint(moved: &[usize]) -> BgraFrame {
    let stride = 4 * WIDTH as usize;
    let mut pixels = vec![0x20u8; stride * HEIGHT as usize];
    let mut fill = |x0: usize, y0: usize, x1: usize, y1: usize, colour: [u8; 3]| {
        for y in y0..y1 {
            for x in x0..x1 {
                let at = y * stride + 4 * x;
                pixels[at..at + 3].copy_from_slice(&colour);
                pixels[at + 3] = 255;
            }
        }
    };
    for i in 0..CONTROLS {
        let (x, y) = cell_origin(i);
        let (x, y) = (x as usize, y as usize);
        let tone = (60 + (i * 37) % 160) as u8;
        fill(x + 8, y + 6, x + 88, y + 38, [tone, 90, 200 - tone / 2]);
        if moved.contains(&i) {
            fill(x + 30, y + 10, x + 54, y + 34, [250, 250, 20]);
        }
    }
    BgraFrame::new(WIDTH, HEIGHT, stride, EPOCH, pixels).expect("a valid frame")
}

// ---- the core, composed as the engine composes it ----

fn plan_params() -> PlanParams {
    PlanParams::default()
}

/// What `Engine::observe` builds from the fusion: one observation per control, labelled
/// with its inner text or its external label, and one per orphan word.
fn observations(fusion: &Fusion) -> Vec<Observation> {
    let controls = fusion.controls.iter().map(|fc| {
        let role = fc.control.role;
        let label = if role.takes_external_label() {
            match &fc.label {
                Some(LabelChoice::Label(word)) => Some(word.text.clone()),
                _ => None,
            }
        } else {
            Some(fc.inner_text()).filter(|t| !t.is_empty())
        };
        Observation {
            bbox: fc.control.bbox,
            role,
            label,
            discriminant: None,
            affordance: fc.control.affordance,
        }
    });
    let text = fusion.text_nodes.iter().map(|w| Observation {
        bbox: w.bbox,
        role: Role::TextNode,
        label: Some(w.text.clone()),
        discriminant: None,
        affordance: Affordance::TEXT_NODE,
    });
    controls.chain(text).collect()
}

fn relations(fusion: &Fusion) -> Vec<(BBox, BBox)> {
    fusion
        .controls
        .iter()
        .filter_map(|fc| match &fc.label {
            Some(LabelChoice::Label(word)) => Some((fc.control.bbox, word.bbox)),
            _ => None,
        })
        .collect()
}

/// The published state of a commit, as heavy as the engine's: the registry and the evidence.
#[derive(Debug, Clone)]
struct Published {
    registry: Registry,
    observations: Vec<Observation>,
    relations: Vec<(BBox, BBox)>,
}

/// The core state between two observations.
struct Core {
    store: SnapshotStore<Published>,
}

/// Where one observation spent its time, in nanoseconds.
#[derive(Debug, Clone, Copy, Default)]
struct Split {
    plan: u64,
    fuse: u64,
    identity: u64,
    commit: u64,
    total: u64,
    /// What the plan decided.
    work: &'static str,
}

fn ns(d: Duration) -> u64 {
    u64::try_from(d.as_nanos()).unwrap_or(u64::MAX)
}

impl Core {
    fn new() -> Self {
        Self {
            store: SnapshotStore::new(),
        }
    }

    /// One observation of `scene` against the committed base: Algorithm 1, fusion,
    /// identity, staging and commit, each stage timed and the whole timed as one block.
    fn observe(&mut self, scene: &Scene) -> Split {
        let previous = self.store.published();
        let held_supports =
            previous.map(|p| p.observations.iter().map(|o| o.bbox).collect::<Vec<_>>());
        let held_relations = previous.map(|p| p.relations.clone());
        let frame = scene.frame.clone();

        let t0 = Instant::now();
        let plan = plan_regions(
            self.store.base(),
            &frame,
            self.store.pending_dirty(),
            Evidence {
                supports: held_supports.as_deref().unwrap_or(&[]),
                relations: held_relations.as_deref().unwrap_or(&[]),
            },
            &plan_params(),
        )
        .expect("the synthetic frame can be planned");
        let t1 = Instant::now();
        let fusion = fuse(&scene.controls, &scene.words, FusionParams::default())
            .expect("the synthetic scene fuses");
        let observed = observations(&fusion);
        let linked = relations(&fusion);
        let t2 = Instant::now();
        let base = self
            .store
            .published()
            .map_or_else(|| Registry::new("perf", EPOCH), |p| p.registry.clone());
        let (registry, _changes) = base
            .observe(EPOCH, &observed, AssociationPolicy::default())
            .expect("the association policy is valid");
        let t3 = Instant::now();
        let staged = self.store.stage(
            frame,
            Published {
                registry,
                observations: observed,
                relations: linked,
            },
        );
        self.store.commit(staged).expect("a single writer");
        let t4 = Instant::now();
        Split {
            plan: ns(t1 - t0),
            fuse: ns(t2 - t1),
            identity: ns(t3 - t2),
            commit: ns(t4 - t3),
            total: ns(t4 - t0),
            work: match plan.work {
                Work::Nothing => "nothing",
                Work::Regions(_) => "regions",
                Work::FullRefresh(_) => "full_refresh",
            },
        }
    }
}

// ---- statistics ----

/// Nearest-rank percentiles of a set of durations, in nanoseconds.
#[derive(Debug, Clone, PartialEq)]
struct Summary {
    n: usize,
    min: u64,
    p50: u64,
    p95: u64,
    p99: u64,
    max: u64,
    mean: f64,
}

/// Nearest rank: the value at position `ceil(p/100 * n)` (1-based) of the sorted samples.
fn nearest_rank(sorted: &[u64], p: f64) -> u64 {
    let rank = (p / 100.0 * sorted.len() as f64).ceil().max(1.0) as usize;
    sorted[rank.min(sorted.len()) - 1]
}

fn summarise(samples: &[u64]) -> Summary {
    assert!(!samples.is_empty(), "no samples");
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    Summary {
        n: sorted.len(),
        min: sorted[0],
        p50: nearest_rank(&sorted, 50.0),
        p95: nearest_rank(&sorted, 95.0),
        p99: nearest_rank(&sorted, 99.0),
        max: sorted[sorted.len() - 1],
        mean: sorted.iter().map(|&v| v as f64).sum::<f64>() / sorted.len() as f64,
    }
}

/// Milliseconds with three decimals (microsecond resolution), for the report.
fn ms(nanos: u64) -> f64 {
    (nanos as f64 / 1e6 * 1e3).round() / 1e3
}

fn summary_json(s: &Summary) -> Value {
    json!({
        "n": s.n, "min_ms": ms(s.min), "p50_ms": ms(s.p50), "p95_ms": ms(s.p95),
        "p99_ms": ms(s.p99), "max_ms": ms(s.max), "mean_ms": (s.mean / 1e6 * 1e3).round() / 1e3,
    })
}

// ---- the layers of the benchmark ----

/// The three situations of the benchmark.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Situation {
    /// The first observation: no base, a full refresh.
    Cold,
    /// The same screen again: nothing to re-analyse, everything fused and tracked again.
    Steady,
    /// One cell changes at a time: one region.
    Incremental,
}

impl Situation {
    const ALL: [Self; 3] = [Self::Cold, Self::Steady, Self::Incremental];

    const fn name(self) -> &'static str {
        match self {
            Self::Cold => "cold_full_refresh",
            Self::Steady => "warm_unchanged",
            Self::Incremental => "warm_one_region",
        }
    }
}

/// The scenes of a situation: the base screen and the screen each iteration observes.
struct Scenes {
    base: Scene,
    /// Alternating screens for `Incremental`: each differs from the one before.
    changed: [Scene; 2],
}

impl Scenes {
    fn new() -> Self {
        Self {
            base: scene(&[]),
            changed: [scene(&[137]), scene(&[])],
        }
    }
}

/// Domain-level core: Algorithm 1, fusion, identity and the commit, stage by stage.
fn domain_layer(situation: Situation, scenes: &Scenes, warmup: usize, samples: usize) -> Value {
    let mut core = Core::new();
    let mut parts: [Vec<u64>; 5] = Default::default();
    let mut work = "";
    let mut count = 0usize;
    for round in 0..warmup + samples {
        let scene = match situation {
            Situation::Cold => {
                core = Core::new();
                &scenes.base
            }
            Situation::Steady => &scenes.base,
            Situation::Incremental => &scenes.changed[round % 2],
        };
        if situation != Situation::Cold && round == 0 {
            core.observe(&scenes.base);
        }
        let split = core.observe(scene);
        work = split.work;
        if round >= warmup {
            count += 1;
            for (slot, value) in parts.iter_mut().zip([
                split.plan,
                split.fuse,
                split.identity,
                split.commit,
                split.total,
            ]) {
                slot.push(value);
            }
        }
    }
    assert_eq!(count, samples);
    json!({
        "situation": situation.name(),
        "plan_decision": work,
        "algorithm_1_plan": summary_json(&summarise(&parts[0])),
        "fusion": summary_json(&summarise(&parts[1])),
        "identity": summary_json(&summarise(&parts[2])),
        "snapshot_commit": summary_json(&summarise(&parts[3])),
        "core_total": summary_json(&summarise(&parts[4])),
    })
}

/// A monotonic clock for the engine over the process start.
struct MonotonicClock(Instant);

impl Clock for MonotonicClock {
    fn now(&self) -> Duration {
        self.0.elapsed()
    }

    fn sleep(&self, duration: Duration) {
        std::thread::sleep(duration);
    }
}

/// Which of the two scenes the doubles are showing.
type Showing = Arc<AtomicUsize>;

struct Screen(Arc<[Scene; 2]>, Showing, u64, Arc<MonotonicClock>);

impl FrameCapturer for Screen {
    fn name(&self) -> &'static str {
        "synthetic-screen"
    }

    fn capture(&mut self) -> Result<CapturedFrame, PortError> {
        self.2 += 1;
        Ok(CapturedFrame {
            frame: self.0[self.1.load(Ordering::Relaxed)].frame.clone(),
            frame_seq: self.2,
            captured_at: self.3.now(),
        })
    }
}

fn overlapping(boxes: &[BBox], regions: &[BBox]) -> Vec<bool> {
    boxes
        .iter()
        .map(|b| regions.iter().any(|r| r.overlaps(b)))
        .collect()
}

struct Detector(Arc<[Scene; 2]>, Showing);

impl VisionDetector for Detector {
    fn name(&self) -> &'static str {
        "synthetic-detector"
    }

    fn models_loaded(&self) -> bool {
        false
    }

    fn detect(&mut self, _: &BgraFrame, regions: &[BBox]) -> Result<Vec<Control>, PortError> {
        let scene = &self.0[self.1.load(Ordering::Relaxed)];
        let boxes: Vec<BBox> = scene.controls.iter().map(|c| c.bbox).collect();
        Ok(scene
            .controls
            .iter()
            .zip(overlapping(&boxes, regions))
            .filter(|(_, hit)| *hit)
            .map(|(c, _)| c.clone())
            .collect())
    }
}

struct Recognizer(Arc<[Scene; 2]>, Showing);

impl TextRecognizer for Recognizer {
    fn name(&self) -> &'static str {
        "synthetic-ocr"
    }

    fn recognize(&mut self, _: &BgraFrame, regions: &[BBox]) -> Result<Vec<Word>, PortError> {
        let scene = &self.0[self.1.load(Ordering::Relaxed)];
        let boxes: Vec<BBox> = scene.words.iter().map(|w| w.bbox).collect();
        Ok(scene
            .words
            .iter()
            .zip(overlapping(&boxes, regions))
            .filter(|(_, hit)| *hit)
            .map(|(w, _)| w.clone())
            .collect())
    }
}

/// Never live: the benchmark sends no input.
struct NoInput;

impl OsActuator for NoInput {
    fn name(&self) -> &'static str {
        "none"
    }

    fn is_live(&self) -> bool {
        false
    }

    fn deliver(
        &mut self,
        _: &ommatix_application::contract::ValidatedAction,
    ) -> Result<ommatix_application::DeliveryReceipt, PortError> {
        Err(PortError::new("none", "the benchmark sends no input"))
    }
}

struct DenyAll;

impl PolicyStore for DenyAll {
    fn current(&self) -> LocalPolicy {
        LocalPolicy::default()
    }
}

fn engine(scenes: Arc<[Scene; 2]>, showing: Showing) -> Engine {
    let clock = Arc::new(MonotonicClock(Instant::now()));
    let mut config = EngineConfig::new("perf");
    // The stages are instant doubles: a thread per stage would only time the spawn.
    config.concurrent_stages = false;
    Engine::new(
        config,
        Ports {
            capturer: Box::new(Screen(scenes.clone(), showing.clone(), 0, clock.clone())),
            detector: Box::new(Detector(scenes.clone(), showing.clone())),
            recognizer: Box::new(Recognizer(scenes, showing)),
            accessibility: Box::new(NoAccessibility),
            actuator: Box::new(NoInput),
            policy: Arc::new(DenyAll),
            clock,
        },
    )
}

/// Application-level core: `Engine::observe_screen` over instant doubles, with the first
/// page built (limit 200). The stage times are the engine's own, on a monotonic clock.
fn engine_layer(situation: Situation, scenes: &Scenes, warmup: usize, samples: usize) -> Value {
    let pair = Arc::new([scenes.base.clone(), scenes.changed[0].clone()]);
    let showing: Showing = Arc::new(AtomicUsize::new(0));
    let request = ObserveRequest {
        limit: 200,
        ..ObserveRequest::default()
    };
    let mut engine = engine(pair.clone(), showing.clone());
    let mut wall = Vec::with_capacity(samples);
    let (mut capture, mut plan, mut fuse_us, mut track, mut inside) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut elements = 0;
    for round in 0..warmup + samples {
        match situation {
            Situation::Cold => {
                showing.store(0, Ordering::Relaxed);
                engine = self::engine(pair.clone(), showing.clone());
            }
            Situation::Steady => showing.store(0, Ordering::Relaxed),
            Situation::Incremental => {
                if round == 0 {
                    showing.store(0, Ordering::Relaxed);
                    engine
                        .observe_screen(&request)
                        .expect("the base observation");
                }
                showing.store((round + 1) % 2, Ordering::Relaxed);
            }
        }
        let started = Instant::now();
        let page = engine
            .observe_screen(&request)
            .expect("the synthetic screen is observed");
        let took = ns(started.elapsed());
        elements = page.total_elements;
        if round >= warmup {
            let t = engine.last_timings().expect("an observation was committed");
            wall.push(took);
            capture.push(ns(t.capture));
            plan.push(ns(t.plan));
            fuse_us.push(ns(t.fuse));
            track.push(ns(t.track));
            inside.push(ns(t.plan) + ns(t.fuse) + ns(t.track));
        }
    }
    json!({
        "situation": situation.name(),
        "elements_published": elements,
        "observe_screen_wall": summary_json(&summarise(&wall)),
        "stage_capture_double": summary_json(&summarise(&capture)),
        "stage_plan": summary_json(&summarise(&plan)),
        "stage_fuse": summary_json(&summarise(&fuse_us)),
        "stage_track": summary_json(&summarise(&track)),
        "plan_fuse_track": summary_json(&summarise(&inside)),
    })
}

// ---- the command line ----

#[derive(Debug)]
struct Options {
    samples: usize,
    engine_samples: usize,
    warmup: usize,
    out: Option<String>,
    allow_debug: bool,
}

fn parse(args: &[String]) -> Result<Options, String> {
    let mut options = Options {
        samples: 10_000,
        engine_samples: 2_000,
        warmup: 200,
        out: None,
        allow_debug: false,
    };
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        let mut value = |name: &str| {
            args.next()
                .ok_or_else(|| format!("{name} needs a value"))
                .cloned()
        };
        let number = |text: String, name: &str| {
            text.parse::<usize>()
                .ok()
                .filter(|&n| n >= 1)
                .ok_or_else(|| format!("{name} must be a whole number of at least 1"))
        };
        match arg.as_str() {
            "--samples" => options.samples = number(value("--samples")?, "--samples")?,
            "--engine-samples" => {
                options.engine_samples = number(value("--engine-samples")?, "--engine-samples")?;
            }
            "--warmup" => options.warmup = number(value("--warmup")?, "--warmup")?,
            "--out" => options.out = Some(value("--out")?),
            "--allow-debug" => options.allow_debug = true,
            other => return Err(format!("unknown argument {other:?}")),
        }
    }
    Ok(options)
}

fn command_output(program: &str, args: &[&str]) -> Option<String> {
    let output = std::process::Command::new(program)
        .args(args)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn environment() -> Value {
    json!({
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "processor": std::env::var("PROCESSOR_IDENTIFIER").ok(),
        "logical_processors": std::thread::available_parallelism().map(usize::from).ok(),
        "optimised": !cfg!(debug_assertions),
        "rustc": command_output("rustc", &["-V"]),
        "commit": command_output("git", &["rev-parse", "HEAD"]),
        "dirty": command_output("git", &["status", "--porcelain", "--untracked-files=no"]).map(|s| !s.is_empty()),
    })
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let options = match parse(&args) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("core_latency: {message}");
            std::process::exit(2);
        }
    };
    if cfg!(debug_assertions) && !options.allow_debug {
        eprintln!("core_latency: this is not an optimised build; use --release (or --allow-debug)");
        std::process::exit(2);
    }
    let scenes = Scenes::new();
    let mut domain = Vec::new();
    let mut application = Vec::new();
    for situation in Situation::ALL {
        eprintln!("core_latency: {} ...", situation.name());
        domain.push(domain_layer(
            situation,
            &scenes,
            options.warmup,
            options.samples,
        ));
        application.push(engine_layer(
            situation,
            &scenes,
            options.warmup.min(options.engine_samples),
            options.engine_samples,
        ));
    }
    let report = json!({
        "test": "PERF-01",
        "requirement": "RNF-04: p95 <= 10 ms for 300 controls and 600 spans, core only",
        "scene": {
            "frame": [WIDTH, HEIGHT],
            "controls": scenes.base.controls.len(),
            "spans": scenes.base.words.len(),
            "tile_size": plan_params().tile_size,
        },
        "method": {
            "percentiles": "nearest rank over the samples after the warm-up",
            "warmup": options.warmup,
            "domain_samples": options.samples,
            "engine_samples": options.engine_samples,
            "core_total": "plan + fusion + identity + stage and commit, timed as one block",
        },
        "environment": environment(),
        "domain_core": domain,
        "application_core": application,
    });
    let text = serde_json::to_string_pretty(&report).expect("the report is JSON") + "\n";
    match &options.out {
        Some(path) => std::fs::write(path, &text).expect("the report is written"),
        None => println!("{text}"),
    }
    let worst = report["domain_core"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|s| s["core_total"]["p95_ms"].as_f64().unwrap_or(f64::NAN))
        .fold(0.0, f64::max);
    eprintln!(
        "core_latency: worst core_total p95 over the situations: {worst} ms (RNF-04 target 10 ms)"
    );
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn perf_01_the_synthetic_scene_has_300_controls_and_600_spans() {
        for moved in [&[][..], &[137][..]] {
            let s = scene(moved);
            assert_eq!(s.controls.len(), 300);
            assert_eq!(s.words.len(), 600);
            assert_eq!((s.frame.width(), s.frame.height()), (WIDTH, HEIGHT));
        }
        // Deterministic: the same arguments build the same scene.
        let (a, b) = (scene(&[]), scene(&[]));
        assert_eq!(a.controls, b.controls);
        assert_eq!(a.words, b.words);
        // Every kind of control is present.
        let kinds: BTreeSet<_> = scene(&[]).controls.iter().map(|c| c.role).collect();
        assert_eq!(kinds.len(), 6);
    }

    #[test]
    fn perf_01_fusion_of_the_scene_labels_what_it_should() {
        let s = scene(&[]);
        let fusion = fuse(&s.controls, &s.words, FusionParams::default()).unwrap();
        assert_eq!(fusion.controls.len(), 300);
        let observed = observations(&fusion);
        assert_eq!(observed.len(), 300 + fusion.text_nodes.len());
        // The words nobody owns are text nodes; the labels and the inner texts are not.
        assert!(
            fusion.text_nodes.len() >= 300,
            "{}",
            fusion.text_nodes.len()
        );
        let labelled = fusion
            .controls
            .iter()
            .filter(|fc| {
                !fc.inner_words.is_empty() || matches!(fc.label, Some(LabelChoice::Label(_)))
            })
            .count();
        // The 250 controls that can carry text; a few labels under an input sit as close to
        // the next row's control and fusion abstains on those.
        assert!((240..=250).contains(&labelled), "{labelled}");
        assert!(relations(&fusion).len() >= 100);
    }

    /// The three situations do what they are named for: the plan decides a full refresh,
    /// nothing, or one region, and the core keeps every element's identity across them.
    #[test]
    fn perf_01_the_situations_plan_what_they_name() {
        let scenes = Scenes::new();
        let mut core = Core::new();
        assert_eq!(core.observe(&scenes.base).work, "full_refresh");
        let published = core.store.published().expect("committed");
        assert_eq!(
            published.registry.tracks().count(),
            published.observations.len()
        );
        assert_eq!(core.observe(&scenes.base).work, "nothing");
        assert_eq!(core.observe(&scenes.changed[0]).work, "regions");
        assert_eq!(core.observe(&scenes.changed[1]).work, "regions");
        let ids: BTreeSet<_> = core
            .store
            .published()
            .unwrap()
            .registry
            .tracks()
            .map(|t| t.target_id)
            .collect();
        assert_eq!(
            ids.len(),
            core.store.published().unwrap().observations.len()
        );
    }

    #[test]
    fn perf_01_percentiles_are_nearest_rank() {
        let samples: Vec<u64> = (1..=100).rev().collect();
        let s = summarise(&samples);
        assert_eq!(
            (s.n, s.min, s.p50, s.p95, s.p99, s.max),
            (100, 1, 50, 95, 99, 100)
        );
        assert!((s.mean - 50.5).abs() < 1e-9);
        let one = summarise(&[7]);
        assert_eq!((one.p50, one.p95, one.p99), (7, 7, 7));
        // 10 samples: p95 and p99 are the largest; p50 the fifth.
        let ten = summarise(&(1..=10).collect::<Vec<_>>());
        assert_eq!((ten.p50, ten.p95, ten.p99), (5, 10, 10));
        assert_eq!(ms(1_234_567), 1.235);
    }

    /// Both layers run end to end in a few iterations, on every situation, and report the
    /// scene's size.
    #[test]
    fn perf_01_both_layers_report_every_situation() {
        let scenes = Scenes::new();
        for situation in Situation::ALL {
            let domain = domain_layer(situation, &scenes, 1, 2);
            assert_eq!(domain["core_total"]["n"], 2);
            let engine = engine_layer(situation, &scenes, 1, 2);
            assert_eq!(engine["observe_screen_wall"]["n"], 2);
            assert!(engine["elements_published"].as_u64().unwrap() >= 300);
        }
    }

    #[test]
    fn perf_01_the_command_line_is_validated() {
        let args = |list: &[&str]| list.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        let options = parse(&args(&["--samples", "5", "--out", "x.json"])).unwrap();
        assert_eq!(
            (options.samples, options.out.as_deref()),
            (5, Some("x.json"))
        );
        for bad in [
            &["--samples", "0"][..],
            &["--samples"][..],
            &["--samples", "many"][..],
            &["--bogus"][..],
        ] {
            assert!(parse(&args(bad)).is_err(), "{bad:?}");
        }
    }
}
