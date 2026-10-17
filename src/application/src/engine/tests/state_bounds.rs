//! Offline RNF-06 evidence; no new planned-test IDs and no real input.
use super::*;
use ommatix_domain::incremental::{DEFAULT_TILE_SIZE, dirty_tiles};
use std::collections::BTreeSet;

struct FailureRecognizer(Shared, Arc<AtomicBool>);
impl TextRecognizer for FailureRecognizer {
    fn name(&self) -> &'static str {
        "failing-recognizer"
    }
    fn recognize(&mut self, frame: &BgraFrame, regions: &[BBox]) -> Result<Vec<Word>, PortError> {
        if self.1.load(Ordering::Relaxed) {
            Err(PortError::new(self.name(), "injected recognizer failure"))
        } else {
            Recognizer(self.0.clone()).recognize(frame, regions)
        }
    }
}

#[test]
fn rnf_06_pending_tiles_equal_changed_union_after_1000_failures_and_clear_on_commit() {
    for detector_failure in [true, false] {
        check_pending_failure(detector_failure);
    }
}

fn check_pending_failure(detector_failure: bool) {
    let mut r = rig(World::default());
    r.engine.config.concurrent_stages = false;
    let failure = Arc::new(AtomicBool::new(false));
    if !detector_failure {
        r.engine.ports.recognizer = Box::new(FailureRecognizer(r.world.clone(), failure.clone()));
    }
    r.observe();
    failure.store(true, Ordering::Relaxed);
    let base = r.engine.store.base().unwrap().clone();
    let mut expected = BTreeSet::new();
    for i in 0..1000_u32 {
        {
            let mut w = r.world();
            w.fail_detect = detector_failure;
            w.events.clear();
            w.detect_calls.clear();
            w.access_calls.clear();
            // Revert previous changes: union must retain tiles no longer different.
            w.pixels = if i % 2 == 0 {
                vec![(i % 10 * 64, i / 10 % 5 * 64, [255, 0, 0])]
            } else {
                vec![]
            };
        }
        let current = render(&r.world());
        expected.extend(
            dirty_tiles(Some(&base), &current, DEFAULT_TILE_SIZE)
                .unwrap()
                .tiles,
        );
        assert!(r.observe_with(&ObserveRequest::default()).is_err());
        assert_eq!(r.engine.store.pending_dirty(), &expected);
        assert!(expected.len() <= 10 * 5);
        assert_eq!(r.engine.store.base(), Some(&base));
        r.clock.advance(Duration::from_millis(1));
    }
    assert!(!expected.is_empty());
    r.world().fail_detect = false;
    failure.store(false, Ordering::Relaxed);
    r.observe();
    assert!(r.engine.store.pending_dirty().is_empty());
}

struct SmallCapture(Shared, Arc<ManualClock>);
impl FrameCapturer for SmallCapture {
    fn name(&self) -> &'static str {
        "small-synthetic-frame"
    }
    fn capture(&mut self) -> Result<CapturedFrame, PortError> {
        let mut w = lock(&self.0);
        w.seq += 1;
        let mut pixels = vec![0; 128 * 64 * 4];
        for c in &w.controls {
            for y in c.bbox.y0() as usize..c.bbox.y1() as usize {
                for x in c.bbox.x0() as usize..c.bbox.x1() as usize {
                    pixels[(y * 128 + x) * 4] = 255;
                }
            }
        }
        Ok(CapturedFrame {
            frame: BgraFrame::new(128, 64, 512, w.epoch, pixels).unwrap(),
            frame_seq: w.seq,
            captured_at: self.1.now(),
            scaling: Scaling::Dpi(96),
        })
    }
}

fn small_rig() -> Rig {
    let mut r = rig(World::default());
    r.engine.config.concurrent_stages = false;
    r.engine.ports.capturer = Box::new(SmallCapture(r.world.clone(), r.clock.clone()));
    r
}

fn churn(r: &mut Rig, i: u32) {
    let mut w = r.world();
    w.controls = if i.is_multiple_of(3) {
        vec![]
    } else {
        let x = f64::from(i % 2 * 64);
        vec![Control {
            bbox: bx(x, 0.0, x + 40.0, 30.0),
            role: Role::Button,
            affordance: confirmed(),
        }]
    };
    // Test instrumentation is not engine state; keep its call/event logs bounded too.
    w.events.clear();
    w.detect_calls.clear();
    w.access_calls.clear();
}

#[test]
fn rnf_06_one_snapshot_one_frame_and_at_most_one_live_track_after_10000_observations() {
    let mut r = small_rig();
    for i in 0..10_000 {
        churn(&mut r, i);
        r.observe();
        let sizes = r.engine.retained_state_sizes();
        assert_eq!(sizes["snapshots"], 1);
        assert_eq!(sizes["base_frames"], 1);
        assert_eq!(sizes["base_pixel_bytes"], 128 * 64 * 4);
        assert!(sizes["live_tracks"] <= 1);
        assert!(sizes["registry_entries"] <= 1);
        assert!(sizes["elements"] <= 1);
        r.clock.advance(Duration::from_millis(1));
    }
}

#[test]
fn rnf_06_no_input_journal_stays_at_capacity_after_10000_requests() {
    let mut r = rig(form());
    let page = r.observe();
    for i in 0..10_000 {
        let request = Rig::action(&page, "Save", &format!("r{i}"), ActionKind::Click);
        assert_eq!(
            r.engine.execute_action(&request).status,
            ActionStatus::Simulated
        );
        let sizes = r.engine.retained_state_sizes();
        assert_eq!(sizes["journal_entries"], (i + 1).min(JOURNAL_CAPACITY));
        assert_eq!(sizes["journal_evictable"], (i + 1).min(JOURNAL_CAPACITY));
    }
}

#[test]
#[ignore = "unbounded: see docs/reviews/2026-10-17-engine-state-bounds.md"]
fn rnf_06_truncated_reads_accumulate_accessibility_on_unchanged_pixels() {
    let mut r = small_rig();
    r.world().access_limit = Some(1);
    for i in 0..100 {
        let x = f64::from(i) / 1000.0;
        let element = AccessibleElement {
            bbox: bx(x, 0.0, x + 1.0, 1.0),
            role: Role::Button,
            name: "synthetic".into(),
            enabled: true,
            offscreen: false,
            password: false,
            patterns: AccessiblePatterns {
                invoke: true,
                ..AccessiblePatterns::default()
            },
            native: None,
        };
        {
            let mut w = r.world();
            w.accessible = vec![element.clone(), element];
            w.events.clear();
            w.detect_calls.clear();
            w.access_calls.clear();
        }
        r.observe();
        assert_eq!(
            r.engine.retained_state_sizes()["accessible"],
            (i + 1) as usize
        );
        r.clock.advance(Duration::from_millis(1));
    }
    assert!(r.engine.retained_state_sizes()["accessible"] <= 1);
}
