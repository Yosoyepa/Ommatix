//! The freshness check (SDD §2.9, chapter 04 §4.2): right before a live delivery the whole
//! screen is captured again and compared exactly with the committed base. That path has no
//! tolerance and no caret filter, so a single pixel that changed after the observation refuses
//! the action with `STALE_SNAPSHOT`, wherever it is on the screen, and nothing is sent. These
//! tests change one pixel of the form that [`super`] paints.

use super::*;

/// Far from every element of `form()`: where a blinking caret would not be.
const FAR_PIXEL: (u32, u32) = (630, 10);

/// Inside the Save button and outside its caption: a hover or focus mark on the target.
const TARGET_PIXEL: (u32, u32) = (405, 255);

/// Observes `form()`, paints one pixel of the screen white, and sends a live click on Save
/// that expects no effect.
fn live_click_after_one_pixel(pixel: (u32, u32)) -> (Rig, ObservePage, ActionResult) {
    let mut r = rig(form());
    r.world().live = true;
    r.set_policy(live_policy());
    let page = r.observe();
    r.world().pixels.push((pixel.0, pixel.1, [255, 255, 255]));
    r.world().events.clear();
    let result = r
        .engine
        .execute_action(&live_click(&page, "r1", ExpectedEffect::None));
    (r, page, result)
}

/// `STALE_SNAPSHOT` as chapter 07 §7.7 gives it (not retryable, observe again), and no event
/// reached the actuator: no delivery, no focus click, no keys.
fn assert_stale_and_nothing_sent(r: &Rig, result: &ActionResult) {
    assert_eq!(result.status, ActionStatus::Rejected, "{result:?}");
    assert!(!result.input_delivered(), "{result:?}");
    let error = result.error.as_ref().expect("an error");
    assert_eq!(
        (error.code, error.retryable, error.next_step),
        (ErrorCode::StaleSnapshot, false, NextStep::ObserveAgain)
    );
    assert!(
        error
            .message
            .contains("the screen changed since the observation"),
        "{}",
        error.message
    );
    let w = r.world();
    assert!(
        w.delivered.is_empty() && w.clicks.is_empty() && w.keys.is_empty(),
        "the actuator was handed nothing"
    );
    assert!(
        !w.events
            .iter()
            .any(|e| matches!(*e, "deliver" | "click" | "keys")),
        "{:?}",
        w.events
    );
}

/// A single pixel changed far from the target after the observation: the live click is refused
/// with `STALE_SNAPSHOT` and nothing is sent. The refused click is not retried on the spent
/// snapshot, even once the pixel is back; a new observation makes the click deliverable.
#[test]
fn a_single_pixel_change_before_delivery_is_stale_and_sends_nothing() {
    let (mut r, page, result) = live_click_after_one_pixel(FAR_PIXEL);
    assert_stale_and_nothing_sent(&r, &result);

    // The pixel goes back, so the screen matches the base again, but the snapshot is spent.
    r.world().pixels.clear();
    let retry = r
        .engine
        .execute_action(&live_click(&page, "r2", ExpectedEffect::None));
    assert_eq!(error_code(&retry), Some(ErrorCode::StaleSnapshot));
    assert!(r.world().delivered.is_empty(), "no click");

    // A new observation is the way on.
    let again = r.observe();
    let result = r
        .engine
        .execute_action(&live_click(&again, "r3", ExpectedEffect::None));
    assert_eq!(result.status, ActionStatus::InputDelivered, "{result:?}");
    let m = r.engine.engine_metrics();
    assert_eq!((m.delivered_actions, m.rejected_actions), (1, 2));
}

/// The same holds when the changed pixel belongs to the target itself: nothing is sent.
#[test]
fn a_single_pixel_change_under_the_target_is_stale_and_sends_nothing() {
    let (r, _page, result) = live_click_after_one_pixel(TARGET_PIXEL);
    assert_stale_and_nothing_sent(&r, &result);
}

#[test]
fn a_dry_run_never_captures_or_predicts_pixel_freshness() {
    let mut r = rig(form());
    r.world().live = true;
    r.set_policy(live_policy());
    let page = r.observe();
    r.world()
        .pixels
        .push((FAR_PIXEL.0, FAR_PIXEL.1, [255, 255, 255]));
    r.world().fail_capture = true;
    r.world().events.clear();
    let mut request = live_click(&page, "dry", ExpectedEffect::None);
    request.dry_run = true;
    let result = r.engine.execute_action(&request);
    assert_eq!(result.status, ActionStatus::Simulated);
    assert!(!r.world().events.contains(&"capture"));
    assert!(r.world().delivered.is_empty(), "no synthetic delivery");
    assert_eq!(
        r.engine.last_action_timings().expect("timings").freshness,
        Duration::ZERO
    );
}
