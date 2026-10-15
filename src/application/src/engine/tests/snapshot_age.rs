//! The age bound of INC-03 (ADR-0025) over the doubles of [`super`]: the capture instant of the
//! snapshot and the start of the accessibility read that confirms the target, each measured
//! against `[engine] max_snapshot_age_ms` on the injected clock. Each test implements a planned
//! ID (UT-46 to UT-54). When a bound has run out the answer is `STALE_SNAPSHOT` with
//! `observe_again`, and nothing is sent.

use super::typing::{name_field, typing};
use super::*;

/// The window of the threshold tests, in milliseconds.
const WINDOW_MS: u64 = 100;

fn save_box() -> BBox {
    bx(400.0, 250.0, 480.0, 280.0)
}

fn rig_aged(world: World, window_ms: u64) -> Rig {
    rig_with(
        world,
        EngineConfig {
            max_snapshot_age: Duration::from_millis(window_ms),
            ..EngineConfig::new(SESSION)
        },
    )
}

/// A rig whose live actuator and policy allow delivery to the application under test.
fn live_aged(world: World, window_ms: u64) -> Rig {
    let r = rig_aged(world, window_ms);
    r.world().live = true;
    r.set_policy(live_policy());
    r
}

/// The Save button, confirmed only by the accessibility tree: the detector boxes it without
/// an affordance of its own (ADR-0017), so its confirmation is dated by that read.
fn accessible_save() -> World {
    World {
        controls: vec![Control {
            bbox: save_box(),
            role: Role::Unknown,
            affordance: Affordance {
                actionability: Actionability::Unknown,
                enabled: true,
                visible: true,
            },
        }],
        words: vec![Word::new("Save", bx(420.0, 258.0, 460.0, 272.0))],
        accessible: vec![AccessibleElement {
            bbox: save_box(),
            role: Role::Button,
            name: "Save".into(),
            enabled: true,
            offscreen: false,
            password: false,
            patterns: AccessiblePatterns {
                invoke: true,
                ..AccessiblePatterns::default()
            },
            native: None,
        }],
        ..World::default()
    }
}

/// `STALE_SNAPSHOT` for an expired bound: nothing sent, not retryable, a new observation
/// asked for, and a message that names the bound (chapter 07 §7.7).
fn assert_observe_again(result: &ActionResult, reason: &str) {
    assert_eq!(result.status, ActionStatus::Rejected, "{result:?}");
    assert!(!result.delivered, "{result:?}");
    let error = result.error.as_ref().expect("an error");
    assert_eq!(
        (error.code, error.retryable, error.next_step),
        (ErrorCode::StaleSnapshot, false, NextStep::ObserveAgain)
    );
    assert!(error.message.contains(reason), "{:?}", error.message);
}

/// UT-46: the bound is inclusive. In simulation, an action 99, 100 and 101 ms after the
/// capture is within the window, at its edge, and refused.
#[test]
fn ut_46_the_window_is_inclusive_before_at_and_after_the_threshold() {
    let mut r = rig_aged(form(), WINDOW_MS);
    let page = r.observe();
    r.clock.advance(Duration::from_millis(99));
    let before = r
        .engine
        .execute_action(&Rig::action(&page, "Save", "before", ActionKind::Click));
    assert_eq!(before.status, ActionStatus::Simulated, "{before:?}");
    r.clock.advance(Duration::from_millis(1));
    let at = r
        .engine
        .execute_action(&Rig::action(&page, "Save", "at", ActionKind::Click));
    assert_eq!(at.status, ActionStatus::Simulated, "{at:?}");
    r.clock.advance(Duration::from_millis(1));
    let after = r
        .engine
        .execute_action(&Rig::action(&page, "Save", "after", ActionKind::Click));
    assert_observe_again(&after, "snapshot is older");
}

/// UT-46 (live): the same bound decides whether input is delivered.
#[test]
fn ut_46_live_input_follows_the_same_bound() {
    for (elapsed_ms, within) in [(99, true), (100, true), (101, false)] {
        let mut r = live_aged(form(), WINDOW_MS);
        let page = r.observe();
        r.clock.advance(Duration::from_millis(elapsed_ms));
        let result = r
            .engine
            .execute_action(&live_click(&page, "live", ExpectedEffect::None));
        if within {
            assert_eq!(
                result.status,
                ActionStatus::InputDelivered,
                "{elapsed_ms} ms: {result:?}"
            );
            assert_eq!(r.world().delivered.len(), 1, "{elapsed_ms} ms");
        } else {
            assert_observe_again(&result, "snapshot is older");
            assert!(r.world().delivered.is_empty(), "{elapsed_ms} ms");
        }
    }
}

/// UT-47: simulation does not accept what the live path refuses. An expired snapshot is
/// `STALE_SNAPSHOT` in dry-run and live alike, and nothing is sent.
#[test]
fn ut_47_simulation_refuses_an_expired_snapshot_like_the_live_path() {
    let mut simulated = rig_aged(form(), WINDOW_MS);
    let page = simulated.observe();
    simulated
        .clock
        .advance(Duration::from_millis(WINDOW_MS + 1));
    let dry =
        simulated
            .engine
            .execute_action(&Rig::action(&page, "Save", "dry", ActionKind::Click));
    assert_observe_again(&dry, "snapshot is older");

    let mut live = live_aged(form(), WINDOW_MS);
    let page = live.observe();
    live.clock.advance(Duration::from_millis(WINDOW_MS + 1));
    let result = live
        .engine
        .execute_action(&live_click(&page, "live", ExpectedEffect::None));
    assert_observe_again(&result, "snapshot is older");
    assert!(live.world().delivered.is_empty(), "nothing was delivered");
}

/// UT-48: the window ends between validation and delivery. The gate accepts the snapshot at
/// 99 ms, the freshness capture takes 2 ms, and the last look at 101 ms refuses: no event
/// follows the capture.
#[test]
fn ut_48_a_snapshot_that_expires_before_delivery_sends_nothing() {
    let mut r = live_aged(form(), WINDOW_MS);
    let page = r.observe();
    r.clock.advance(Duration::from_millis(99));
    r.world().advance_on_capture = Duration::from_millis(2);
    r.world().events.clear();
    let result = r
        .engine
        .execute_action(&live_click(&page, "race", ExpectedEffect::None));
    assert_observe_again(&result, "snapshot is older");
    // The freshness capture ran after the gate, and nothing was sent after it.
    assert_eq!(
        r.world().events.last(),
        Some(&"capture"),
        "{:?}",
        r.world().events
    );
    assert!(r.world().delivered.is_empty(), "nothing was delivered");
}

/// UT-49: reused accessibility evidence expires between validation and delivery while the
/// snapshot is still fresh. The snapshot was published at 60 ms without a new read, the
/// evidence was read at 0 ms, and the freshness capture moves the last look past 100 ms.
#[test]
fn ut_49_evidence_that_expires_before_delivery_sends_nothing() {
    let mut r = live_aged(accessible_save(), WINDOW_MS);
    r.observe();
    r.clock.advance(Duration::from_millis(60));
    let page = r.observe();
    r.clock.advance(Duration::from_millis(39));
    r.world().advance_on_capture = Duration::from_millis(2);
    let result = r
        .engine
        .execute_action(&live_click(&page, "reused", ExpectedEffect::None));
    assert_observe_again(&result, "accessibility evidence");
    assert!(r.world().delivered.is_empty(), "nothing was delivered");
}

/// UT-50: a publication that reads nothing keeps the evidence's own age. The page published at
/// 60 ms is 70 ms old at 130 ms, but the Save evidence read at 0 ms is not: the action is
/// refused. Observing again reads the region whose evidence expired, and then the action runs.
#[test]
fn ut_50_a_new_publication_does_not_renew_an_accessibility_read() {
    let mut r = rig_aged(accessible_save(), WINDOW_MS);
    r.observe();
    r.clock.advance(Duration::from_millis(60));
    let reads = r.world().access_calls.len();
    let page = r.observe();
    assert_eq!(
        r.world().access_calls.len(),
        reads,
        "nothing expired, so nothing is read"
    );
    r.clock.advance(Duration::from_millis(70));
    let refused = r
        .engine
        .execute_action(&Rig::action(&page, "Save", "old", ActionKind::Click));
    assert_observe_again(&refused, "accessibility evidence");

    let fresh = r.observe();
    let read_again = r
        .world()
        .access_calls
        .last()
        .is_some_and(|regions| regions.iter().any(|region| region.overlaps(&save_box())));
    assert!(read_again, "the expired region is read again");
    let again = r
        .engine
        .execute_action(&Rig::action(&fresh, "Save", "again", ActionKind::Click));
    assert_eq!(again.status, ActionStatus::Simulated, "{again:?}");
}

/// UT-51: a late accessibility read spends the window from the capture, not from the
/// publication. A read of 150 ms publishes a snapshot that is already outside the window; a
/// read of 60 ms leaves the window open until 100 ms after the capture.
#[test]
fn ut_51_a_late_accessibility_read_spends_the_window_from_the_capture() {
    let late = World {
        access_delay: Duration::from_millis(150),
        ..accessible_save()
    };
    let mut r = rig_aged(late, WINDOW_MS);
    let page = r.observe();
    let result = r
        .engine
        .execute_action(&Rig::action(&page, "Save", "late", ActionKind::Click));
    assert_observe_again(&result, "snapshot is older");

    let timely = World {
        access_delay: Duration::from_millis(60),
        ..accessible_save()
    };
    let mut r = rig_aged(timely, WINDOW_MS);
    let page = r.observe();
    r.clock.advance(Duration::from_millis(40));
    let at_edge = r
        .engine
        .execute_action(&Rig::action(&page, "Save", "edge", ActionKind::Click));
    assert_eq!(at_edge.status, ActionStatus::Simulated, "{at_edge:?}");
}

/// UT-52: the window ends while the keyboard focus of `type` is awaited, after the focusing
/// click. No key is sent; the click is reported as delivered, the result is indeterminate and
/// nothing is retried.
#[test]
fn ut_52_expiry_between_the_focusing_click_and_the_keys_sends_no_key() {
    let mut r = rig_aged(form(), 90);
    {
        let mut w = r.world();
        w.live = true;
        w.phased = true;
        w.reads_focus = true;
    }
    r.set_policy(live_policy());
    let page = r.observe();
    // Focus reaches the field 100 ms after the click; the window ends at 90 ms.
    r.world().focus_script = VecDeque::from([None, None, Some(name_field())]);
    r.world().events.clear();
    let result = r
        .engine
        .execute_action(&typing(&page, "type-late", ExpectedEffect::None));
    assert_eq!(result.status, ActionStatus::Indeterminate, "{result:?}");
    let error = result.error.as_ref().expect("an error");
    assert_eq!(
        (error.code, error.retryable, error.next_step),
        (
            ErrorCode::IndeterminateEffect,
            false,
            NextStep::ReconcileEffect
        )
    );
    assert!(error.message.contains("snapshot expired"), "{error:?}");
    assert!(result.input_delivered(), "the focusing click was delivered");
    let w = r.world();
    assert_eq!(w.clicks.len(), 1, "the focusing click");
    assert!(w.keys.is_empty(), "no key was sent");
    assert!(w.delivered.is_empty(), "not through the one-shot path");
}

/// UT-53: the Save button becomes disabled in the accessibility tree with no repaint. The
/// evidence read at 0 ms is not renewed by the publication at 60 ms, so the live action at
/// 101 ms is refused. Observing again reads the element, which is then not actionable.
#[test]
fn ut_53_a_semantic_change_without_repaint_is_seen_after_the_window() {
    let mut r = live_aged(accessible_save(), WINDOW_MS);
    r.observe();
    r.world().accessible[0].enabled = false;
    r.clock.advance(Duration::from_millis(60));
    let page = r.observe();
    r.clock.advance(Duration::from_millis(41));
    let stale = r
        .engine
        .execute_action(&live_click(&page, "stale", ExpectedEffect::None));
    assert_observe_again(&stale, "accessibility evidence");
    assert!(r.world().delivered.is_empty(), "nothing was delivered");

    let fresh = r.observe();
    let result = r
        .engine
        .execute_action(&live_click(&fresh, "fresh", ExpectedEffect::None));
    assert_eq!(
        result.error.as_ref().map(|e| e.code),
        Some(ErrorCode::NotActionable),
        "{result:?}"
    );
    assert!(r.world().delivered.is_empty(), "nothing was delivered");
}

/// UT-54: the default window is ten seconds (ADR-0025). An observation is usable at ten
/// seconds and refused a millisecond later.
#[test]
fn ut_54_the_default_window_is_ten_seconds() {
    assert_eq!(
        EngineConfig::new(SESSION).max_snapshot_age,
        Duration::from_secs(10)
    );
    let mut r = rig(form());
    let page = r.observe();
    r.clock.advance(Duration::from_secs(10));
    let at = r
        .engine
        .execute_action(&Rig::action(&page, "Save", "ten", ActionKind::Click));
    assert_eq!(at.status, ActionStatus::Simulated, "{at:?}");
    r.clock.advance(Duration::from_millis(1));
    let after = r
        .engine
        .execute_action(&Rig::action(&page, "Save", "eleven", ActionKind::Click));
    assert_observe_again(&after, "snapshot is older");
}
