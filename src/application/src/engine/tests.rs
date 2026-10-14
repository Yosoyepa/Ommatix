//! Engine flows over deterministic test doubles and a [`ManualClock`]. Tests that implement
//! a planned test carry its ID; their expected outcome follows the oracle case in brackets.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use ommatix_domain::bbox::BBox;
use ommatix_domain::fusion::{Actionability, Affordance, Control, Role, TextKind, Word};
use ommatix_domain::identity::ActionStatus;
use ommatix_domain::incremental::BgraFrame;

use super::*;
use crate::contract::{
    ActionKind, ErrorCode, ExpectedEffect, LabelSource, NextStep, ObservedEffect, ScrollDirection,
    ValidatedAction, Zone,
};
use crate::ports::{CapturedFrame, DeliveryReceipt, LocalPolicy};
use crate::{ManualClock, PortError};

const W: u32 = 640;
const H: u32 = 320;
const SESSION: &str = "test-session";
const APP: &str = "test-app";

fn bx(x0: f64, y0: f64, x1: f64, y1: f64) -> BBox {
    BBox::new(x0, y0, x1, y1).unwrap()
}

fn confirmed() -> Affordance {
    Affordance {
        actionability: Actionability::Confirmed,
        enabled: true,
        visible: true,
    }
}

/// What the doubles share: a scene painted into frames, failure switches and records.
#[derive(Debug)]
#[allow(clippy::struct_excessive_bools)] // independent switches of a test double
struct World {
    controls: Vec<Control>,
    words: Vec<Word>,
    epoch: u64,
    seq: u64,
    fail_capture: bool,
    fail_detect: bool,
    detect_calls: Vec<Vec<BBox>>,
    live: bool,
    receipt: Result<DeliveryReceipt, ()>,
    delivered: Vec<ValidatedAction>,
    /// Paints a mark after each delivery, as a UI reacting to input would.
    react_to_input: bool,
    reactions: u8,
    /// What the actuator attests as the foreground application.
    foreground: Option<String>,
    /// Captures and deliveries, in order.
    events: Vec<&'static str>,
}

impl Default for World {
    fn default() -> Self {
        Self {
            controls: Vec::new(),
            words: Vec::new(),
            epoch: 1,
            seq: 0,
            fail_capture: false,
            fail_detect: false,
            detect_calls: Vec::new(),
            live: false,
            receipt: Ok(DeliveryReceipt {
                submitted: 2,
                inserted: 2,
            }),
            delivered: Vec::new(),
            react_to_input: false,
            reactions: 0,
            foreground: Some(APP.into()),
            events: Vec::new(),
        }
    }
}

type Shared = Arc<Mutex<World>>;

fn lock(w: &Shared) -> MutexGuard<'_, World> {
    w.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Paints every item as a filled rectangle whose colour derives from its index.
fn render(world: &World) -> BgraFrame {
    let stride = 4 * W as usize;
    let mut px = vec![0u8; stride * H as usize];
    let mut paint = |b: &BBox, colour: [u8; 3]| {
        let x0 = b.x0().max(0.0) as usize;
        let y0 = b.y0().max(0.0) as usize;
        let x1 = (b.x1().ceil() as usize).min(W as usize);
        let y1 = (b.y1().ceil() as usize).min(H as usize);
        for y in y0..y1 {
            for x in x0..x1 {
                let i = y * stride + 4 * x;
                px[i..i + 3].copy_from_slice(&colour);
                px[i + 3] = 255;
            }
        }
    };
    for (i, c) in world.controls.iter().enumerate() {
        paint(&c.bbox, [40 + i as u8 * 20, 100, 200]);
    }
    for (i, w) in world.words.iter().enumerate() {
        paint(&w.bbox, [10, 10 + i as u8 * 20, 10]);
    }
    if world.reactions > 0 {
        paint(&bx(600.0, 300.0, 610.0, 310.0), [255, world.reactions, 0]);
    }
    BgraFrame::new(W, H, stride, world.epoch, px).unwrap()
}

struct Capturer(Shared, Arc<ManualClock>);

impl FrameCapturer for Capturer {
    fn name(&self) -> &'static str {
        "test-capturer"
    }

    fn capture(&mut self) -> Result<CapturedFrame, PortError> {
        let mut w = lock(&self.0);
        if w.fail_capture {
            return Err(PortError::new("test-capturer", "gone"));
        }
        w.seq += 1;
        w.events.push("capture");
        Ok(CapturedFrame {
            frame: render(&w),
            frame_seq: w.seq,
            captured_at: self.1.now(),
        })
    }
}

struct Detector(Shared);

impl VisionDetector for Detector {
    fn name(&self) -> &'static str {
        "test-detector"
    }

    fn models_loaded(&self) -> bool {
        false
    }

    fn detect(&mut self, _: &BgraFrame, regions: &[BBox]) -> Result<Vec<Control>, PortError> {
        let mut w = lock(&self.0);
        w.detect_calls.push(regions.to_vec());
        if w.fail_detect {
            return Err(PortError::new("test-detector", "model crashed"));
        }
        Ok(w.controls
            .iter()
            .filter(|c| regions.iter().any(|r| r.overlaps(&c.bbox)))
            .cloned()
            .collect())
    }
}

struct Recognizer(Shared);

impl TextRecognizer for Recognizer {
    fn name(&self) -> &'static str {
        "test-recognizer"
    }

    fn recognize(&mut self, _: &BgraFrame, regions: &[BBox]) -> Result<Vec<Word>, PortError> {
        let w = lock(&self.0);
        Ok(w.words
            .iter()
            .filter(|t| regions.iter().any(|r| r.overlaps(&t.bbox)))
            .cloned()
            .collect())
    }
}

struct Actuator(Shared);

impl OsActuator for Actuator {
    fn name(&self) -> &'static str {
        "test-actuator"
    }

    fn is_live(&self) -> bool {
        lock(&self.0).live
    }

    fn foreground_application(&self) -> Option<String> {
        lock(&self.0).foreground.clone()
    }

    fn deliver(&mut self, action: &ValidatedAction) -> Result<DeliveryReceipt, PortError> {
        let mut w = lock(&self.0);
        w.events.push("deliver");
        w.delivered.push(action.clone());
        if w.react_to_input {
            w.reactions = w.reactions.wrapping_add(1);
        }
        w.receipt
            .map_err(|()| PortError::new("test-actuator", "injection failed"))
    }
}

#[derive(Debug, Default)]
struct Policy(Mutex<LocalPolicy>);

impl PolicyStore for Policy {
    fn current(&self) -> LocalPolicy {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

struct Rig {
    engine: Engine,
    world: Shared,
    clock: Arc<ManualClock>,
    policy: Arc<Policy>,
}

impl Rig {
    fn world(&self) -> MutexGuard<'_, World> {
        lock(&self.world)
    }

    fn set_policy(&self, policy: LocalPolicy) {
        *self.policy.0.lock().unwrap_or_else(PoisonError::into_inner) = policy;
    }

    fn observe(&mut self) -> ObservePage {
        self.engine
            .observe_screen(&ObserveRequest::default())
            .unwrap()
    }

    fn observe_with(&mut self, request: &ObserveRequest) -> Result<ObservePage, ToolError> {
        self.engine.observe_screen(request)
    }

    /// The element of `page` whose label is `label`.
    fn id_of(page: &ObservePage, label: &str) -> (String, u64) {
        let e = page
            .elements
            .iter()
            .find(|e| e.label == label)
            .unwrap_or_else(|| panic!("no element labelled {label:?}"));
        (e.target_id.to_string(), e.revision)
    }

    fn action(
        page: &ObservePage,
        label: &str,
        request_id: &str,
        action: ActionKind,
    ) -> ActionRequest {
        let (target_id, expected_revision) = Self::id_of(page, label);
        ActionRequest {
            request_id: request_id.into(),
            session_id: page.session_id.clone(),
            snapshot_id: page.snapshot_id.clone(),
            geometry_epoch: page.geometry_epoch,
            target_id,
            expected_revision,
            action,
            dry_run: true,
            expected_effect: ExpectedEffect::None,
        }
    }
}

/// A form: "Name" to the left of an input, a Save button and a Cancel button, each with
/// its caption inside.
fn form() -> World {
    let input = bx(100.0, 40.0, 300.0, 70.0);
    let save = bx(400.0, 250.0, 480.0, 280.0);
    let cancel = bx(500.0, 250.0, 580.0, 280.0);
    World {
        controls: vec![
            Control {
                bbox: input,
                role: Role::Input,
                affordance: confirmed(),
            },
            Control {
                bbox: save,
                role: Role::Button,
                affordance: confirmed(),
            },
            Control {
                bbox: cancel,
                role: Role::Button,
                affordance: confirmed(),
            },
        ],
        words: vec![
            Word::new("Name", bx(40.0, 48.0, 90.0, 62.0)),
            Word::new("hello", bx(110.0, 48.0, 160.0, 62.0)),
            Word::new("Save", bx(420.0, 258.0, 460.0, 272.0)),
            Word::new("Cancel", bx(515.0, 258.0, 565.0, 272.0)),
        ],
        ..World::default()
    }
}

fn rig(world: World) -> Rig {
    let world = Arc::new(Mutex::new(world));
    let clock = Arc::new(ManualClock::new());
    let policy = Arc::new(Policy::default());
    let engine = Engine::new(
        EngineConfig::new(SESSION),
        Ports {
            capturer: Box::new(Capturer(world.clone(), clock.clone())),
            detector: Box::new(Detector(world.clone())),
            recognizer: Box::new(Recognizer(world.clone())),
            actuator: Box::new(Actuator(world.clone())),
            policy: policy.clone(),
            clock: clock.clone(),
        },
    );
    Rig {
        engine,
        world,
        clock,
        policy,
    }
}

fn live_policy() -> LocalPolicy {
    LocalPolicy {
        actuation_enabled: true,
        allowed_applications: vec![APP.into()],
        ..LocalPolicy::default()
    }
}

fn error_code(result: &ActionResult) -> Option<ErrorCode> {
    result.error.as_ref().map(|e| e.code)
}

// ---- observe_screen ----

#[test]
fn the_first_observation_publishes_a_sorted_full_snapshot() {
    let mut r = rig(form());
    r.clock.advance(Duration::from_millis(5));
    let page = r.observe();
    r.clock.advance(Duration::from_millis(7));

    assert_eq!(page.session_id, SESSION);
    assert_eq!(page.snapshot_id, "s1");
    assert_eq!((page.geometry_epoch, page.frame_seq), (1, 1));
    assert!(!page.degraded && !page.truncated());
    assert_eq!(page.total_elements, 3);
    let summary: Vec<_> = page
        .elements
        .iter()
        .map(|e| {
            (
                e.target_id.to_string(),
                e.role,
                e.label.as_str(),
                e.label_source,
            )
        })
        .collect();
    // IDs in reading order; "Name" labels the input, the captions label the buttons, and
    // "hello" is the input's (redacted) value, not an element.
    assert_eq!(
        summary,
        vec![
            ("e1".into(), Role::Input, "Name", LabelSource::Ocr),
            ("e2".into(), Role::Button, "Save", LabelSource::Ocr),
            ("e3".into(), Role::Button, "Cancel", LabelSource::Ocr),
        ]
    );
    assert!(page.elements.iter().all(|e| e.text.is_none()));
    assert!(page.elements.iter().all(|e| e.zone == Zone::Unknown));
    assert_eq!(page.elements[0].enabled, Some(true));
    // The page was built right after the commit, on the same virtual instant.
    assert_eq!(page.age_ms, 0);
    let again = r
        .observe_with(&ObserveRequest {
            limit: 1,
            ..ObserveRequest::default()
        })
        .unwrap();
    assert_eq!(again.age_ms, 0, "a new observation is a new capture");
}

#[test]
fn orphan_words_are_text_nodes_that_are_never_actionable() {
    let mut world = form();
    world
        .words
        .push(Word::new("Welcome", bx(300.0, 150.0, 380.0, 165.0)));
    let mut r = rig(world);
    let page = r.observe();
    let node = page.elements.iter().find(|e| e.label == "Welcome").unwrap();
    assert_eq!(node.role, Role::TextNode);
    assert_eq!(node.actionability, Actionability::Unknown);
    assert_eq!((node.enabled, node.visible), (None, Some(true)));

    let click = Rig::action(&page, "Welcome", "r1", ActionKind::Click);
    let result = r.engine.execute_action(&click);
    assert_eq!(result.status, ActionStatus::Rejected);
    assert_eq!(error_code(&result), Some(ErrorCode::NotActionable));
}

#[test]
fn values_need_both_the_request_and_the_local_policy() {
    let mut r = rig(form());
    let asked = ObserveRequest {
        include_values: true,
        ..ObserveRequest::default()
    };
    let page = r.observe_with(&asked).unwrap();
    assert!(
        page.elements.iter().all(|e| e.text.is_none()),
        "policy denies"
    );

    r.set_policy(LocalPolicy {
        include_values: true,
        ..LocalPolicy::default()
    });
    let page = r.observe_with(&asked).unwrap();
    let input = page
        .elements
        .iter()
        .find(|e| e.role == Role::Input)
        .unwrap();
    assert_eq!(input.text, Some(("hello".into(), TextKind::Unknown)));
    assert!(
        page.elements
            .iter()
            .filter(|e| e.role != Role::Input)
            .all(|e| e.text.is_none())
    );
    let page = r.observe();
    assert!(page.elements.iter().all(|e| e.text.is_none()), "not asked");
}

#[test]
fn a_query_filters_the_view_and_never_removes_elements_from_the_registry() {
    let mut r = rig(form());
    let all = r.observe();
    let filtered = r
        .observe_with(&ObserveRequest {
            query: Some("  SAVE ".into()),
            ..ObserveRequest::default()
        })
        .unwrap();
    assert_eq!(filtered.total_elements, 1);
    assert_eq!(filtered.elements[0].label, "Save");
    // Values are not searchable unless they are shown.
    let hidden = r
        .observe_with(&ObserveRequest {
            query: Some("hello".into()),
            ..ObserveRequest::default()
        })
        .unwrap();
    assert_eq!(hidden.total_elements, 0);

    let after = r.observe();
    assert_eq!(after.snapshot_id, "s4");
    let ids = |p: &ObservePage| {
        p.elements
            .iter()
            .map(|e| (e.target_id, e.revision))
            .collect::<Vec<_>>()
    };
    assert_eq!(ids(&after), ids(&all), "same tracks, same revisions");
}

#[test]
fn pagination_is_stable_within_a_snapshot_and_rejected_after_a_new_one() {
    let mut world = form();
    for i in 0..4u8 {
        let x = 40.0 + f64::from(i) * 60.0;
        world
            .words
            .push(Word::new(format!("w{i}"), bx(x, 150.0, x + 40.0, 165.0)));
    }
    let mut r = rig(world);
    let page_size = ObserveRequest {
        limit: 3,
        ..ObserveRequest::default()
    };
    let first = r.observe_with(&page_size).unwrap();
    assert_eq!(first.total_elements, 7);
    assert_eq!(first.elements.len(), 3);
    let cursor = first.next_cursor.clone().expect("truncated");
    assert!(cursor.len() <= crate::contract::limits::MAX_CURSOR_CHARS);

    let next = ObserveRequest {
        cursor: Some(cursor.clone()),
        ..page_size.clone()
    };
    let second = r.observe_with(&next).unwrap();
    assert_eq!(second.snapshot_id, first.snapshot_id);
    assert_eq!(second.frame_seq, first.frame_seq, "a cursor never captures");
    assert_eq!(second, r.observe_with(&next).unwrap(), "stable");
    let third = r
        .observe_with(&ObserveRequest {
            cursor: second.next_cursor.clone(),
            ..page_size.clone()
        })
        .unwrap();
    assert_eq!(third.elements.len(), 1);
    assert!(!third.truncated());
    let mut seen: Vec<_> = [&first, &second, &third]
        .iter()
        .flat_map(|p| p.elements.iter().map(|e| e.target_id))
        .collect();
    let all = seen.clone();
    seen.sort();
    seen.dedup();
    assert_eq!(seen, all, "pages are disjoint and sorted by ID");
    assert_eq!(seen.len(), 7);

    // A cursor bound to another query or value mode is not honoured.
    let other_query = ObserveRequest {
        query: Some("w".into()),
        ..next.clone()
    };
    let err = r.observe_with(&other_query).unwrap_err();
    assert_eq!(err.code, ErrorCode::StaleSnapshot);

    // Malformed cursors are stale, not internal errors.
    for bad in ["x", "1.3.0.zz", "1.999.0.0000000000000000"] {
        let err = r
            .observe_with(&ObserveRequest {
                cursor: Some(bad.into()),
                ..page_size.clone()
            })
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::StaleSnapshot, "{bad}");
    }

    r.observe();
    let err = r.observe_with(&next).unwrap_err();
    assert_eq!(
        (err.code, err.retryable, err.next_step),
        (ErrorCode::StaleSnapshot, false, NextStep::ObserveAgain)
    );
}

#[test]
fn a_policy_change_between_pages_invalidates_the_cursor() {
    let mut r = rig(form());
    r.set_policy(LocalPolicy {
        include_values: true,
        ..LocalPolicy::default()
    });
    let asked = ObserveRequest {
        limit: 2,
        include_values: true,
        ..ObserveRequest::default()
    };
    let first = r.observe_with(&asked).unwrap();
    let next = ObserveRequest {
        cursor: first.next_cursor.clone(),
        ..asked.clone()
    };
    assert!(r.observe_with(&next).is_ok(), "same effective view");

    // The operator withdraws values: the view the cursor paged no longer exists.
    r.set_policy(LocalPolicy::default());
    let err = r.observe_with(&next).unwrap_err();
    assert_eq!(
        (err.code, err.next_step),
        (ErrorCode::StaleSnapshot, NextStep::ObserveAgain)
    );
}

#[test]
fn only_planned_regions_reach_the_detector() {
    let mut r = rig(form());
    r.observe();
    assert_eq!(
        r.world().detect_calls,
        vec![vec![bx(0.0, 0.0, f64::from(W), f64::from(H))]],
        "the first observation is a full refresh"
    );

    // Nothing changed: no inference at all, but still a new snapshot.
    let page = r.observe();
    assert_eq!(page.snapshot_id, "s2");
    assert_eq!(r.world().detect_calls.len(), 1);

    // Move the Cancel button a little: only the region around it is analysed.
    {
        let mut w = r.world();
        w.controls[2].bbox = bx(502.0, 250.0, 582.0, 280.0);
        w.words[3].bbox = bx(517.0, 258.0, 567.0, 272.0);
    }
    let page = r.observe();
    let calls = r.world().detect_calls.clone();
    assert_eq!(calls.len(), 2);
    let regions = &calls[1];
    assert_eq!(regions, &vec![bx(400.0, 160.0, 640.0, 320.0)]);
    let full = bx(0.0, 0.0, f64::from(W), f64::from(H));
    assert!(regions.iter().all(|reg| *reg != full));
    // The Save button overlaps the halo, so it was re-analysed too; the input was not, and
    // its evidence was kept.
    let (cancel, rev) = Rig::id_of(&page, "Cancel");
    assert_eq!(
        (cancel.as_str(), rev),
        ("e3", 2),
        "moved: same ID, new revision"
    );
    assert_eq!(Rig::id_of(&page, "Name"), ("e1".into(), 1));
    assert_eq!(page.total_elements, 3);
}

#[test]
fn a_capture_failure_is_retryable_and_publishes_nothing() {
    let mut r = rig(form());
    let page = r.observe();
    r.world().fail_capture = true;
    let err = r.observe_with(&ObserveRequest::default()).unwrap_err();
    assert_eq!(
        (err.code, err.retryable, err.next_step),
        (ErrorCode::CaptureUnavailable, true, NextStep::ObserveAgain)
    );
    assert_eq!(r.engine.engine_metrics().observations, 1);
    // The screen may have changed unseen: the old snapshot is no evidence to act on.
    let click = Rig::action(&page, "Save", "r1", ActionKind::Click);
    let result = r.engine.execute_action(&click);
    assert_eq!(result.status, ActionStatus::Rejected);
    assert_eq!(error_code(&result), Some(ErrorCode::StaleSnapshot));

    r.world().fail_capture = false;
    let page = r.observe();
    let click = Rig::action(&page, "Save", "r2", ActionKind::Click);
    assert_eq!(
        r.engine.execute_action(&click).status,
        ActionStatus::Simulated
    );
}

#[test]
fn an_unplannable_frame_blocks_actions() {
    let mut r = rig(form());
    let page = r.observe();
    r.engine.config.plan.tile_size = 0;
    let err = r.observe_with(&ObserveRequest::default()).unwrap_err();
    assert_eq!(
        (err.code, err.retryable, err.next_step),
        (ErrorCode::UnsupportedGeometry, false, NextStep::Stop)
    );
    assert_eq!(r.engine.engine_metrics().observations, 1);
    let click = Rig::action(&page, "Save", "r1", ActionKind::Click);
    let result = r.engine.execute_action(&click);
    assert_eq!(error_code(&result), Some(ErrorCode::StaleSnapshot));
}

#[test]
fn a_frame_with_epoch_zero_is_not_published() {
    // Not even as the first observation.
    let mut fresh = rig(World { epoch: 0, ..form() });
    let err = fresh.observe_with(&ObserveRequest::default()).unwrap_err();
    assert_eq!(err.code, ErrorCode::UnsupportedGeometry);
    assert_eq!(fresh.engine.engine_metrics().observations, 0);
    assert!(fresh.world().detect_calls.is_empty());

    let mut r = rig(form());
    let page = r.observe();
    r.world().epoch = 0;
    let err = r.observe_with(&ObserveRequest::default()).unwrap_err();
    assert_eq!(
        (err.code, err.retryable, err.next_step),
        (ErrorCode::UnsupportedGeometry, false, NextStep::Stop)
    );
    assert_eq!(r.engine.engine_metrics().observations, 1);
    let click = Rig::action(&page, "Save", "r1", ActionKind::Click);
    assert_eq!(
        error_code(&r.engine.execute_action(&click)),
        Some(ErrorCode::StaleSnapshot)
    );
}

/// UT-14 at engine level (OR-19): a failed inference publishes nothing, keeps the changed
/// tiles pending and blocks actions until a consistent observation is committed.
#[test]
fn ut_14_failed_inference_keeps_pending_dirty_and_blocks_actions() {
    let mut r = rig(form());
    let page = r.observe();
    {
        let mut w = r.world();
        w.controls[2].bbox = bx(502.0, 250.0, 582.0, 280.0);
        w.fail_detect = true;
    }
    let err = r.observe_with(&ObserveRequest::default()).unwrap_err();
    assert_eq!(
        (err.code, err.retryable, err.next_step),
        (ErrorCode::InferenceFailed, true, NextStep::ObserveAgain)
    );
    // Moving the button by 2 px changed its left and right edges: two separate tile
    // components (its caption, unchanged, covers the tiles in between).
    let m = r.engine.engine_metrics();
    assert_eq!((m.observations, m.pending_regions), (1, 2));

    let click = Rig::action(&page, "Save", "r1", ActionKind::Click);
    let result = r.engine.execute_action(&click);
    assert_eq!(result.status, ActionStatus::Rejected);
    assert_eq!(error_code(&result), Some(ErrorCode::StaleSnapshot));

    // Every observation is a fresh attempt: it fails again while the detector is down.
    let again = r
        .observe_with(&ObserveRequest {
            limit: 1,
            ..ObserveRequest::default()
        })
        .unwrap_err();
    assert_eq!(again.code, ErrorCode::InferenceFailed);

    // Even when the screen reverts, the pending tiles are analysed again.
    {
        let mut w = r.world();
        w.controls[2].bbox = bx(500.0, 250.0, 580.0, 280.0);
        w.fail_detect = false;
    }
    let before = r.world().detect_calls.len();
    let page = r.observe();
    assert_eq!(r.world().detect_calls.len(), before + 1);
    assert_eq!(page.snapshot_id, "s2");
    assert!(!page.degraded);
    assert_eq!(r.engine.engine_metrics().pending_regions, 0);
    let click = Rig::action(&page, "Save", "r2", ActionKind::Click);
    assert_eq!(
        r.engine.execute_action(&click).status,
        ActionStatus::Simulated
    );
}

#[test]
fn a_cursor_on_a_degraded_snapshot_is_served_as_degraded() {
    let mut world = form();
    world
        .words
        .push(Word::new("Welcome", bx(300.0, 150.0, 380.0, 165.0)));
    let mut r = rig(world);
    let first = r
        .observe_with(&ObserveRequest {
            limit: 2,
            ..ObserveRequest::default()
        })
        .unwrap();
    {
        let mut w = r.world();
        w.controls[2].bbox = bx(502.0, 250.0, 582.0, 280.0);
        w.fail_detect = true;
    }
    assert!(r.observe_with(&ObserveRequest::default()).is_err());
    let second = r
        .observe_with(&ObserveRequest {
            limit: 2,
            cursor: first.next_cursor,
            ..ObserveRequest::default()
        })
        .unwrap();
    assert!(second.degraded);
    assert_eq!(second.snapshot_id, first.snapshot_id);
}

#[test]
fn a_geometry_change_retires_every_track() {
    let mut r = rig(form());
    let before = r.observe();
    r.world().epoch = 2;
    let after = r.observe();
    assert_eq!(after.geometry_epoch, 2);
    assert_eq!(Rig::id_of(&after, "Save").0, "e5");
    let click = Rig::action(&before, "Save", "r1", ActionKind::Click);
    let result = r.engine.execute_action(&click);
    assert_eq!(error_code(&result), Some(ErrorCode::EpochMismatch));
}

// ---- execute_action ----

/// UT-38 at engine level (OR-39): an action on the previous snapshot is rejected after a
/// new observation, although its target still exists.
#[test]
fn ut_38_action_on_previous_snapshot_is_stale_after_a_new_observation() {
    let mut r = rig(form());
    let old = r.observe();
    let new = r.observe();
    assert_eq!(Rig::id_of(&new, "Save"), Rig::id_of(&old, "Save"));
    let result = r
        .engine
        .execute_action(&Rig::action(&old, "Save", "r1", ActionKind::Click));
    assert_eq!(result.status, ActionStatus::Rejected);
    assert!(!result.input_delivered());
    let error = result.error.unwrap();
    assert_eq!(
        (error.code, error.retryable, error.next_step),
        (ErrorCode::StaleSnapshot, false, NextStep::ObserveAgain)
    );
}

/// UT-39 at engine level (OR-37, OR-40): a moved element keeps its ID with revision 2, and
/// an action on revision 1 is stale.
#[test]
fn ut_39_action_on_an_old_revision_is_a_stale_target() {
    let mut r = rig(form());
    let old = r.observe();
    {
        let mut w = r.world();
        w.controls[1].bbox = bx(401.0, 250.0, 481.0, 280.0);
    }
    let new = r.observe();
    assert_eq!(Rig::id_of(&new, "Save"), ("e2".into(), 2));
    let mut stale = Rig::action(&new, "Save", "r1", ActionKind::Click);
    stale.expected_revision = Rig::id_of(&old, "Save").1;
    assert_eq!(
        error_code(&r.engine.execute_action(&stale)),
        Some(ErrorCode::StaleTarget)
    );
}

/// UT-40 at engine level (OR-42).
#[test]
fn ut_40_action_from_another_session_is_rejected() {
    let mut r = rig(form());
    let page = r.observe();
    let mut action = Rig::action(&page, "Save", "r1", ActionKind::Click);
    action.session_id = "other".into();
    assert_eq!(
        error_code(&r.engine.execute_action(&action)),
        Some(ErrorCode::SessionMismatch)
    );
}

/// UT-41 at engine level (OR-47): a retired ID is not found, and is never reused.
#[test]
fn ut_41_retired_or_unknown_target_is_not_found() {
    let mut r = rig(form());
    let old = r.observe();
    let (cancel, _) = Rig::id_of(&old, "Cancel");
    {
        let mut w = r.world();
        w.controls.pop();
        w.words.pop();
    }
    let new = r.observe();
    assert_eq!(new.total_elements, 2);
    let mut action = Rig::action(&new, "Save", "r1", ActionKind::Click);
    action.target_id = cancel;
    assert_eq!(
        error_code(&r.engine.execute_action(&action)),
        Some(ErrorCode::TargetNotFound)
    );
    action.request_id = "r2".into();
    action.target_id = "nonsense".into();
    assert_eq!(
        error_code(&r.engine.execute_action(&action)),
        Some(ErrorCode::TargetNotFound)
    );
}

/// UT-42 at engine level (OR-43): a dry run is `simulated`, never delivered, even when
/// the policy and the actuator would allow live input.
#[test]
fn ut_42_dry_run_never_delivers() {
    let mut r = rig(form());
    r.world().live = true;
    r.set_policy(live_policy());
    let page = r.observe();
    for (i, action) in [
        ActionKind::Click,
        ActionKind::Scroll {
            direction: ScrollDirection::Down,
            steps: 3,
        },
    ]
    .into_iter()
    .enumerate()
    {
        let result = r
            .engine
            .execute_action(&Rig::action(&page, "Save", &format!("r{i}"), action));
        assert_eq!(result.status, ActionStatus::Simulated);
        assert!(!result.input_delivered() && !result.verified());
        assert_eq!((result.error, result.observed_effect), (None, None));
    }
    let typed = Rig::action(&page, "Name", "r9", ActionKind::Type { text: "x".into() });
    assert_eq!(
        r.engine.execute_action(&typed).status,
        ActionStatus::Simulated
    );
    assert!(r.world().delivered.is_empty());
    let m = r.engine.engine_metrics();
    assert_eq!((m.delivered_actions, m.rejected_actions), (0, 0));
}

/// UT-43 at engine level (OR-44): live input is denied unless the operator's policy
/// enables it, the operator has not stopped the engine and a live actuator exists. The
/// agent has no way to change that.
#[test]
fn ut_43_live_input_is_denied_without_policy_and_live_actuator() {
    let mut r = rig(form());
    let page = r.observe();
    let live = |id: &str| ActionRequest {
        dry_run: false,
        ..Rig::action(&page, "Save", id, ActionKind::Click)
    };
    let setups = [
        (true, LocalPolicy::default()),
        (false, live_policy()),
        (
            true,
            LocalPolicy {
                stopped: true,
                ..live_policy()
            },
        ),
    ];
    for (i, (is_live, policy)) in setups.into_iter().enumerate() {
        r.world().live = is_live;
        r.set_policy(policy);
        let result = r.engine.execute_action(&live(&format!("r{i}")));
        assert_eq!(result.status, ActionStatus::Rejected);
        let error = result.error.unwrap();
        assert_eq!(
            (error.code, error.retryable, error.next_step),
            (ErrorCode::PolicyDenied, false, NextStep::OperatorReview)
        );
    }
    assert!(r.world().delivered.is_empty());
    assert_eq!(r.engine.engine_metrics().rejected_actions, 3);
}

#[test]
fn type_needs_an_input_field() {
    let mut r = rig(form());
    let page = r.observe();
    let on_button = Rig::action(&page, "Save", "r1", ActionKind::Type { text: "x".into() });
    let result = r.engine.execute_action(&on_button);
    assert_eq!(error_code(&result), Some(ErrorCode::NotActionable));
}

#[test]
fn out_of_contract_arguments_are_rejected_before_the_journal() {
    let mut r = rig(form());
    let page = r.observe();
    let long = Rig::action(
        &page,
        "Name",
        "r1",
        ActionKind::Type {
            text: "x".repeat(2049),
        },
    );
    assert_eq!(
        error_code(&r.engine.execute_action(&long)),
        Some(ErrorCode::LimitExceeded)
    );
    let ok = Rig::action(&page, "Name", "r1", ActionKind::Type { text: "x".into() });
    assert_eq!(r.engine.execute_action(&ok).status, ActionStatus::Simulated);
    let scroll = Rig::action(
        &page,
        "Save",
        "r2",
        ActionKind::Scroll {
            direction: ScrollDirection::Up,
            steps: 6,
        },
    );
    assert_eq!(
        error_code(&r.engine.execute_action(&scroll)),
        Some(ErrorCode::LimitExceeded)
    );
}

#[test]
fn live_input_is_delivered_at_the_target_centre_and_never_claims_an_effect() {
    let mut r = rig(form());
    r.world().live = true;
    r.set_policy(live_policy());
    let page = r.observe();
    let mut click = Rig::action(&page, "Save", "r1", ActionKind::Click);
    click.dry_run = false;
    let result = r.engine.execute_action(&click);
    assert_eq!(result.status, ActionStatus::InputDelivered);
    assert!(result.input_delivered() && !result.verified());
    assert_eq!((result.error, result.observed_effect), (None, None));
    let delivered = r.world().delivered.clone();
    assert_eq!(delivered.len(), 1);
    assert_eq!(delivered[0].point(), (440.0, 265.0));
    assert_eq!(delivered[0].kind(), &ActionKind::Click);
    assert_eq!(
        delivered[0].target_id().to_string(),
        Rig::id_of(&page, "Save").0
    );
    let m = r.engine.engine_metrics();
    assert_eq!((m.mode, m.delivered_actions), (EngineMode::Native, 1));
}

#[test]
fn a_visual_change_after_delivery_is_an_observed_effect() {
    let mut r = rig(form());
    {
        let mut w = r.world();
        w.live = true;
        w.react_to_input = true;
    }
    r.set_policy(live_policy());
    let page = r.observe();
    let mut click = Rig::action(&page, "Save", "r1", ActionKind::Click);
    click.dry_run = false;
    click.expected_effect = ExpectedEffect::VisualChange;
    let result = r.engine.execute_action(&click);
    assert_eq!(result.status, ActionStatus::EffectObserved);
    assert!(result.input_delivered() && result.verified());
    assert_eq!(result.observed_effect, Some(ObservedEffect::VisualChange));
    assert_eq!(
        r.engine.engine_metrics().observations,
        1,
        "verification publishes nothing"
    );
}

#[test]
fn unverifiable_or_partial_delivery_is_indeterminate_and_never_retried() {
    let mut r = rig(form());
    r.world().live = true;
    r.set_policy(live_policy());
    // Every delivery spends its snapshot, so each attempt observes first.
    let live = |r: &mut Rig, id: &str, effect| {
        let page = r.observe();
        r.engine.execute_action(&live_click(&page, id, effect))
    };

    // No visual change although one was expected.
    let result = live(&mut r, "r1", ExpectedEffect::VisualChange);
    assert_eq!(result.status, ActionStatus::Indeterminate);
    // An effect this engine cannot check yet.
    let result = live(&mut r, "r2", ExpectedEffect::FocusChanged);
    assert_eq!(result.status, ActionStatus::Indeterminate);
    let error = result.error.unwrap();
    assert_eq!(
        (error.code, error.retryable, error.next_step),
        (
            ErrorCode::IndeterminateEffect,
            false,
            NextStep::ReconcileEffect
        )
    );
    assert_eq!(result.observed_effect, Some(ObservedEffect::Unknown));

    // The OS took fewer events than were sent.
    r.world().receipt = Ok(DeliveryReceipt {
        submitted: 2,
        inserted: 1,
    });
    let result = live(&mut r, "r3", ExpectedEffect::None);
    assert_eq!(result.status, ActionStatus::Indeterminate);
    assert!(!result.input_delivered());
    // The actuator failed outright.
    r.world().receipt = Err(());
    let result = live(&mut r, "r4", ExpectedEffect::None);
    assert_eq!(result.status, ActionStatus::Indeterminate);

    assert_eq!(r.world().delivered.len(), 4, "one attempt each, no retries");
    let m = r.engine.engine_metrics();
    assert_eq!((m.delivered_actions, m.rejected_actions), (0, 0));
}

/// A live click on the Save button of `page`, expecting `effect`.
fn live_click(page: &ObservePage, request_id: &str, effect: ExpectedEffect) -> ActionRequest {
    ActionRequest {
        dry_run: false,
        expected_effect: effect,
        ..Rig::action(page, "Save", request_id, ActionKind::Click)
    }
}

fn assert_policy_denied(result: &ActionResult, message: &str) {
    assert_eq!(result.status, ActionStatus::Rejected);
    let error = result.error.as_ref().unwrap();
    assert_eq!(
        (error.code, error.retryable, error.next_step),
        (ErrorCode::PolicyDenied, false, NextStep::OperatorReview)
    );
    assert_eq!(error.message, message);
}

#[test]
fn live_input_is_denied_when_no_application_is_allowlisted() {
    let mut r = rig(form());
    r.world().live = true;
    r.set_policy(LocalPolicy {
        allowed_applications: Vec::new(),
        ..live_policy()
    });
    let page = r.observe();
    let result = r
        .engine
        .execute_action(&live_click(&page, "r1", ExpectedEffect::None));
    assert_policy_denied(&result, "no application is on the local allowlist");
    assert!(r.world().delivered.is_empty());
}

#[test]
fn live_input_is_denied_when_the_foreground_application_is_unknown() {
    let mut r = rig(form());
    r.world().live = true;
    r.set_policy(live_policy());
    let page = r.observe();
    // The actuator cannot attest the foreground application.
    r.world().foreground = None;
    let result = r
        .engine
        .execute_action(&live_click(&page, "r1", ExpectedEffect::None));
    assert_policy_denied(
        &result,
        "foreground application is not on the local allowlist",
    );
    // It can, but the application is not on the list.
    r.world().foreground = Some("other-app".into());
    let result = r
        .engine
        .execute_action(&live_click(&page, "r2", ExpectedEffect::None));
    assert_policy_denied(
        &result,
        "foreground application is not on the local allowlist",
    );
    assert!(r.world().delivered.is_empty());
    // On the list: delivered.
    r.world().foreground = Some(APP.into());
    let result = r
        .engine
        .execute_action(&live_click(&page, "r3", ExpectedEffect::None));
    assert_eq!(result.status, ActionStatus::InputDelivered);
}

#[test]
fn a_change_before_delivery_is_not_an_observed_effect() {
    let mut r = rig(form());
    r.world().live = true;
    r.set_policy(live_policy());
    let page = r.observe();
    // Something unrelated to the action changes the screen after the observation.
    r.world().reactions = 1;
    let result = r
        .engine
        .execute_action(&live_click(&page, "r1", ExpectedEffect::VisualChange));
    assert_eq!(result.status, ActionStatus::Rejected);
    let error = result.error.unwrap();
    assert_eq!(
        (error.code, error.next_step),
        (ErrorCode::StaleSnapshot, NextStep::ObserveAgain)
    );
    assert!(r.world().delivered.is_empty(), "nothing delivered");
    assert_eq!(r.engine.engine_metrics().delivered_actions, 0);
}

#[test]
fn visual_change_is_measured_against_the_pre_delivery_frame() {
    let mut r = rig(form());
    {
        let mut w = r.world();
        w.live = true;
        w.react_to_input = true;
    }
    r.set_policy(live_policy());
    let page = r.observe();
    r.world().events.clear();
    let result = r
        .engine
        .execute_action(&live_click(&page, "r1", ExpectedEffect::VisualChange));
    assert_eq!(result.status, ActionStatus::EffectObserved);
    // A frame right before delivery and one after it: the effect is their difference.
    assert_eq!(r.world().events, ["capture", "deliver", "capture"]);
}

#[test]
fn a_second_action_after_delivery_needs_a_new_observation() {
    let mut r = rig(form());
    r.world().live = true;
    r.set_policy(live_policy());
    let page = r.observe();
    let first = r
        .engine
        .execute_action(&live_click(&page, "r1", ExpectedEffect::None));
    assert_eq!(first.status, ActionStatus::InputDelivered);

    // Live or dry run, the delivered input may have changed what the snapshot shows.
    for (id, dry_run) in [("r2", false), ("r3", true)] {
        let request = ActionRequest {
            dry_run,
            ..live_click(&page, id, ExpectedEffect::None)
        };
        let result = r.engine.execute_action(&request);
        assert_eq!(result.status, ActionStatus::Rejected);
        let error = result.error.unwrap();
        assert_eq!(
            (error.code, error.next_step),
            (ErrorCode::StaleSnapshot, NextStep::ObserveAgain)
        );
    }
    assert_eq!(r.world().delivered.len(), 1);
    // A replay still returns the stored result.
    assert_eq!(
        r.engine
            .execute_action(&live_click(&page, "r1", ExpectedEffect::None)),
        first
    );

    // An indeterminate delivery spends the snapshot too.
    let page = r.observe();
    r.world().receipt = Err(());
    let result = r
        .engine
        .execute_action(&live_click(&page, "r4", ExpectedEffect::None));
    assert_eq!(result.status, ActionStatus::Indeterminate);
    let result = r
        .engine
        .execute_action(&Rig::action(&page, "Save", "r5", ActionKind::Click));
    assert_eq!(error_code(&result), Some(ErrorCode::StaleSnapshot));

    let page = r.observe();
    let result = r
        .engine
        .execute_action(&Rig::action(&page, "Save", "r6", ActionKind::Click));
    assert_eq!(result.status, ActionStatus::Simulated);
}

#[test]
fn a_replayed_request_id_returns_the_stored_result_without_new_events() {
    let mut r = rig(form());
    r.world().live = true;
    r.set_policy(live_policy());
    let page = r.observe();
    let typed = ActionRequest {
        dry_run: false,
        ..Rig::action(
            &page,
            "Name",
            "r1",
            ActionKind::Type {
                text: "secret".into(),
            },
        )
    };
    let first = r.engine.execute_action(&typed);
    assert_eq!(first.status, ActionStatus::InputDelivered);
    let metrics = r.engine.engine_metrics();

    // Even after a new observation made the original arguments stale.
    r.observe();
    let replay = r.engine.execute_action(&typed);
    assert_eq!(replay, first);
    assert_eq!(r.world().delivered.len(), 1, "no new input events");
    let after = r.engine.engine_metrics();
    assert_eq!(
        (after.delivered_actions, after.rejected_actions),
        (metrics.delivered_actions, metrics.rejected_actions)
    );
}

#[test]
fn a_reused_request_id_with_other_arguments_is_a_conflict() {
    let mut r = rig(form());
    let page = r.observe();
    let first = Rig::action(&page, "Save", "r1", ActionKind::Click);
    assert_eq!(
        r.engine.execute_action(&first).status,
        ActionStatus::Simulated
    );
    let other = Rig::action(&page, "Cancel", "r1", ActionKind::Click);
    let result = r.engine.execute_action(&other);
    assert_eq!(result.status, ActionStatus::Rejected);
    let error = result.error.unwrap();
    assert_eq!(
        (error.code, error.retryable, error.next_step),
        (ErrorCode::RequestConflict, false, NextStep::OperatorReview)
    );
    // The original stays journaled.
    assert_eq!(
        r.engine.execute_action(&first).status,
        ActionStatus::Simulated
    );
}

#[test]
fn snapshot_ids_that_this_engine_did_not_issue_are_stale() {
    let mut r = rig(form());
    let page = r.observe();
    for bad in ["1", "s01", "S1", "s2"] {
        let mut action = Rig::action(&page, "Save", bad, ActionKind::Click);
        action.snapshot_id = bad.into();
        assert_eq!(
            error_code(&r.engine.execute_action(&action)),
            Some(ErrorCode::StaleSnapshot),
            "{bad}"
        );
    }
}

#[test]
fn an_action_before_any_observation_is_stale() {
    let mut r = rig(form());
    let action = ActionRequest {
        request_id: "r1".into(),
        session_id: SESSION.into(),
        snapshot_id: "s1".into(),
        geometry_epoch: 1,
        target_id: "e1".into(),
        expected_revision: 1,
        action: ActionKind::Click,
        dry_run: true,
        expected_effect: ExpectedEffect::None,
    };
    assert_eq!(
        error_code(&r.engine.execute_action(&action)),
        Some(ErrorCode::StaleSnapshot)
    );
}

// ---- get_engine_metrics ----

#[test]
fn metrics_report_counters_without_inventing_timings() {
    let mut r = rig(form());
    let m = r.engine.engine_metrics();
    assert_eq!(m.mode, EngineMode::Simulation);
    assert_eq!(m.timings_source, TimingsSource::NotMeasured);
    assert_eq!((m.observations, m.pending_regions), (0, 0));
    assert!(!m.models_loaded);
    assert_eq!(r.engine.last_timings(), None);

    r.observe();
    let m = r.engine.engine_metrics();
    assert_eq!(m.timings_source, TimingsSource::RuntimeMonotonic);
    assert_eq!(m.observations, 1);
    assert_eq!(r.engine.last_timings(), Some(StageTimings::default()));
    assert_eq!(r.engine.session_id(), SESSION);
}

#[test]
fn identical_runs_produce_identical_pages() {
    let run = || {
        let mut r = rig(form());
        let a = r.observe();
        r.world().controls[1].bbox = bx(401.0, 250.0, 481.0, 280.0);
        let b = r.observe();
        (a, b)
    };
    assert_eq!(run(), run());
}
