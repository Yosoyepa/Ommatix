//! Concurrent observation stages (SDD §2.7, ADR-0019): detection, text recognition and the
//! accessibility read of one observation overlap, and the evidence stays exactly what the
//! sequential order produces.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration as StdDuration, Instant};

use super::*;

/// A form whose window exposes the Save button and the Name field to the accessibility
/// tree, so that all three stages produce evidence.
fn scene() -> World {
    let mut w = form();
    w.accessible = vec![
        AccessibleElement {
            bbox: bx(400.0, 250.0, 480.0, 280.0),
            role: Role::Button,
            name: "Guardar".into(),
            enabled: true,
            offscreen: false,
            password: false,
            patterns: AccessiblePatterns {
                invoke: true,
                ..AccessiblePatterns::default()
            },
        },
        AccessibleElement {
            bbox: bx(100.0, 40.0, 300.0, 70.0),
            role: Role::Input,
            name: "Nombre".into(),
            enabled: true,
            offscreen: false,
            password: false,
            patterns: AccessiblePatterns {
                editable_value: true,
                ..AccessiblePatterns::default()
            },
        },
    ];
    w
}

fn rig_in_mode(concurrent: bool) -> Rig {
    let mut config = EngineConfig::new(SESSION);
    config.concurrent_stages = concurrent;
    rig_with(scene(), config)
}

/// Everything an observation publishes except what depends on the run (its timings).
fn published(r: &Rig, mut page: ObservePage) -> (ObservePage, String) {
    page.timings = StageTimings::default();
    let evidence = r
        .engine
        .store
        .published()
        .map(|p| {
            format!(
                "{:?}|{:?}|{:?}",
                p.evidence, p.relations, p.accessibility_gaps
            )
        })
        .unwrap_or_default();
    (page, evidence)
}

/// Runs the same scripted session: a full refresh, one region, two distant regions,
/// nothing, and returns what each observation published plus the regions the detector saw.
fn session(r: &mut Rig) -> (Vec<(ObservePage, String)>, Vec<Vec<BBox>>) {
    let mut out = Vec::new();
    let page = r.observe();
    out.push(published(r, page));

    {
        let mut w = r.world();
        w.controls[2].bbox = bx(502.0, 250.0, 582.0, 280.0);
        w.words[3].bbox = bx(517.0, 258.0, 567.0, 272.0);
    }
    let page = r.observe();
    out.push(published(r, page));

    {
        let mut w = r.world();
        w.words[0].bbox = bx(42.0, 48.0, 92.0, 62.0);
        w.controls[2].bbox = bx(504.0, 252.0, 584.0, 282.0);
    }
    let page = r.observe();
    out.push(published(r, page));

    let page = r.observe();
    out.push(published(r, page));
    let calls = r.world().detect_calls.clone();
    (out, calls)
}

#[test]
fn concurrent_and_sequential_stages_publish_identical_snapshots() {
    let (sequential, seq_calls) = session(&mut rig_in_mode(false));
    let (concurrent, con_calls) = session(&mut rig_in_mode(true));
    assert_eq!(sequential.len(), 4);
    assert_eq!(concurrent, sequential, "same frames, same elements and ids");
    assert_eq!(con_calls, seq_calls, "the same regions reach the detector");
    assert!(
        sequential.iter().all(|(page, _)| page.total_elements > 0),
        "the scene yields elements to compare"
    );
}

#[test]
fn a_failed_stage_fails_the_observation_the_same_way_in_both_modes() {
    for name in ["detector", "accessibility"] {
        let mut outcomes = Vec::new();
        for concurrent in [false, true] {
            let mut r = rig_in_mode(concurrent);
            match name {
                "detector" => r.world().fail_detect = true,
                _ => r.world().fail_access = true,
            }
            let error = r.observe_with(&ObserveRequest::default()).unwrap_err();
            assert_eq!(error.code, ErrorCode::InferenceFailed, "{name}");
            // Recovered, the next observation analyses again what the failure left pending.
            {
                let mut w = r.world();
                w.fail_detect = false;
                w.fail_access = false;
            }
            let page = r.observe();
            outcomes.push((error.message, published(&r, page)));
        }
        assert_eq!(outcomes[0], outcomes[1], "{name}");
    }
}

/// A stage double that proves overlap: it announces itself and waits, for up to `patience`,
/// until all `parties` stages are inside at once.
struct Rendezvous {
    inside: Arc<AtomicUsize>,
    widest: Arc<AtomicUsize>,
    parties: usize,
    patience: StdDuration,
}

impl Rendezvous {
    fn meet(&self) {
        let now = self.inside.fetch_add(1, Ordering::SeqCst) + 1;
        self.widest.fetch_max(now, Ordering::SeqCst);
        let deadline = Instant::now() + self.patience;
        while self.inside.load(Ordering::SeqCst) < self.parties && Instant::now() < deadline {
            thread::yield_now();
        }
        self.widest
            .fetch_max(self.inside.load(Ordering::SeqCst), Ordering::SeqCst);
        // Leave only when everybody had the chance to see the full house.
        thread::sleep(StdDuration::from_millis(5));
        self.inside.fetch_sub(1, Ordering::SeqCst);
    }
}

struct SlowDetector(Rendezvous);
struct SlowRecognizer(Rendezvous);
struct SlowAccess(Rendezvous);

impl VisionDetector for SlowDetector {
    fn name(&self) -> &'static str {
        "slow-detector"
    }

    fn models_loaded(&self) -> bool {
        false
    }

    fn detect(&mut self, _: &BgraFrame, _: &[BBox]) -> Result<Vec<Control>, PortError> {
        self.0.meet();
        Ok(Vec::new())
    }
}

impl TextRecognizer for SlowRecognizer {
    fn name(&self) -> &'static str {
        "slow-recognizer"
    }

    fn recognize(&mut self, _: &BgraFrame, _: &[BBox]) -> Result<Vec<Word>, PortError> {
        self.0.meet();
        Ok(Vec::new())
    }
}

impl AccessibilityProvider for SlowAccess {
    fn name(&self) -> &'static str {
        "slow-accessibility"
    }

    fn snapshot(&mut self, _: &[BBox]) -> Result<AccessibleRead, PortError> {
        self.0.meet();
        Ok(AccessibleRead::default())
    }

    fn reads_keyboard_focus(&self) -> bool {
        false
    }

    fn keyboard_focus(&mut self) -> Result<Option<AccessibleElement>, PortError> {
        Ok(None)
    }
}

/// How many stages were inside at once during one full-refresh observation.
fn widest_overlap(concurrent: bool, patience: StdDuration) -> usize {
    let mut config = EngineConfig::new(SESSION);
    config.concurrent_stages = concurrent;
    let mut r = rig_with(form(), config);
    let inside = Arc::new(AtomicUsize::new(0));
    let widest = Arc::new(AtomicUsize::new(0));
    let party = || Rendezvous {
        inside: inside.clone(),
        widest: widest.clone(),
        parties: 3,
        patience,
    };
    r.engine.ports.detector = Box::new(SlowDetector(party()));
    r.engine.ports.recognizer = Box::new(SlowRecognizer(party()));
    r.engine.ports.accessibility = Box::new(SlowAccess(party()));
    r.observe();
    widest.load(Ordering::SeqCst)
}

#[test]
fn the_three_stages_of_one_observation_overlap() {
    assert_eq!(widest_overlap(true, StdDuration::from_secs(10)), 3);
}

#[test]
fn sequential_stages_never_overlap() {
    assert_eq!(widest_overlap(false, StdDuration::from_millis(20)), 1);
}

/// A detector that takes a fixed time on the injected clock.
struct Ticking(Arc<ManualClock>, Duration);

impl VisionDetector for Ticking {
    fn name(&self) -> &'static str {
        "ticking"
    }

    fn models_loaded(&self) -> bool {
        false
    }

    fn detect(&mut self, _: &BgraFrame, _: &[BBox]) -> Result<Vec<Control>, PortError> {
        self.0.advance(self.1);
        Ok(Vec::new())
    }
}

#[test]
fn a_stage_timing_is_the_stage_own_duration() {
    for concurrent in [false, true] {
        let mut config = EngineConfig::new(SESSION);
        config.concurrent_stages = concurrent;
        let mut r = rig_with(form(), config);
        r.engine.ports.detector = Box::new(Ticking(r.clock.clone(), Duration::from_millis(10)));
        let page = r.observe();
        assert_eq!(
            page.timings.detect,
            Duration::from_millis(10),
            "{concurrent}"
        );
        assert_eq!(page.timings.recognize, Duration::ZERO, "{concurrent}");
        assert_eq!(
            page.timings.total,
            Duration::from_millis(10),
            "{concurrent}"
        );
    }
}
