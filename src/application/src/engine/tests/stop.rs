//! SEC-02 (chapter 08 §8.2, BDD-32): the operator's stop, engaged at each stage of an action
//! sequence. The operator is a double here: it engages the stop at a chosen event
//! (`Rig::stop_at`), and the policy reports it from then on, as the stop file or the in-process
//! switch does. Nothing reaches the OS; the doubles record what they are handed.
//!
//! The stop is read at the gate and again right before each delivery. It never undoes what
//! the OS already received: those results keep the status they were reported with.

use super::typing::{assert_clicked_but_not_typed, name_field, typing, typing_rig};
use super::*;

#[test]
fn sec_02_a_stop_engaged_before_the_request_sends_nothing() {
    let mut r = rig(form());
    r.world().live = true;
    r.set_policy(live_policy());
    let page = r.observe();
    r.world().events.clear();
    r.engage_operator_stop();
    let result = r
        .engine
        .execute_action(&live_click(&page, "r1", ExpectedEffect::None));
    assert_policy_denied(&result, STOPPED);
    assert!(
        r.world().events.is_empty(),
        "refused before the foreground, the frame or the actuator is touched"
    );
    assert!(!result.input_delivered());
}

/// Between the gate and the last look: the operator engages the stop while the freshness frame
/// is captured. The last look reads it and nothing is delivered.
#[test]
fn sec_02_a_stop_engaged_while_the_frame_is_captured_stops_the_click_at_the_last_look() {
    let mut r = rig(form());
    r.world().live = true;
    r.set_policy(live_policy());
    let page = r.observe();
    r.world().events.clear();
    r.stop_at("capture", 1);
    let result = r
        .engine
        .execute_action(&live_click(&page, "r1", ExpectedEffect::None));
    assert_policy_denied(&result, STOPPED);
    assert_eq!(
        r.world().events,
        ["foreground", "capture"],
        "the last look refuses before it asks for the foreground application again"
    );
    assert!(
        r.world().delivered.is_empty(),
        "no input reached the actuator"
    );
    // Nothing was delivered, so the snapshot is still usable once the operator clears the stop.
    r.clear_operator_stop();
    let retry = r
        .engine
        .execute_action(&live_click(&page, "r2", ExpectedEffect::None));
    assert_eq!(retry.status, ActionStatus::InputDelivered);
    assert_eq!(r.world().delivered.len(), 1);
}

/// During the single delivery: the click reaches the OS, the operator engages the stop, and the
/// result reports the click as it happened. The next action is refused.
#[test]
fn sec_02_a_stop_engaged_while_the_click_is_delivered_reports_it_and_sends_nothing_more() {
    let mut r = rig(form());
    r.world().live = true;
    r.set_policy(live_policy());
    let page = r.observe();
    r.world().events.clear();
    r.stop_at("deliver", 1);
    let result = r
        .engine
        .execute_action(&live_click(&page, "r1", ExpectedEffect::None));
    assert_eq!(result.status, ActionStatus::InputDelivered);
    assert!(result.input_delivered());
    assert_eq!(r.world().delivered.len(), 1);

    let page = r.observe();
    r.world().events.clear();
    let next = r
        .engine
        .execute_action(&live_click(&page, "r2", ExpectedEffect::None));
    assert_policy_denied(&next, STOPPED);
    assert!(r.world().events.is_empty(), "no new events");
    assert_eq!(r.world().delivered.len(), 1);
    let m = r.engine.engine_metrics();
    assert_eq!((m.delivered_actions, m.rejected_actions), (1, 1));
}

/// During verification: the second capture of the sequence is the first verification read of
/// the effect. The operator's stop changes neither the observed effect nor the input sent, and
/// the same request replayed afterwards returns the stored result without events.
#[test]
fn sec_02_a_stop_engaged_during_verification_changes_no_result_and_sends_no_input() {
    let mut r = rig(form());
    r.world().live = true;
    r.world().react_to_input = true;
    r.set_policy(live_policy());
    let page = r.observe();
    r.world().events.clear();
    r.stop_at("capture", 2);
    let request = live_click(&page, "r1", ExpectedEffect::VisualChange);
    let result = r.engine.execute_action(&request);
    assert_eq!(result.status, ActionStatus::EffectObserved);
    assert_eq!(
        r.world().events,
        ["foreground", "capture", "foreground", "deliver", "capture"],
        "verification only reads the screen"
    );
    assert_eq!(r.world().delivered.len(), 1);

    let replay = r.engine.execute_action(&request);
    assert_eq!(replay, result);
    assert_eq!(r.world().events.len(), 5, "a replay captures nothing");
}

/// During a verification that sees no change: the report stays `indeterminate`, with the click
/// reported as delivered, and no input is sent again.
#[test]
fn sec_02_a_stop_engaged_during_a_silent_verification_keeps_the_indeterminate_report() {
    let mut r = rig(form());
    r.world().live = true;
    r.set_policy(live_policy());
    let page = r.observe();
    r.world().events.clear();
    r.stop_at("capture", 2);
    let result = r
        .engine
        .execute_action(&live_click(&page, "r1", ExpectedEffect::VisualChange));
    assert_eq!(result.status, ActionStatus::Indeterminate);
    assert_eq!(error_code(&result), Some(ErrorCode::IndeterminateEffect));
    assert_eq!(result.observed_effect, Some(ObservedEffect::Unknown));
    assert!(result.input_delivered());
    assert!(!result.verified());
    assert_eq!(r.world().delivered.len(), 1, "no retry");
}

/// Two-phase `type`, at the focusing click: the click reached the OS, the stop is read before
/// the keys, and no key is sent. A replay of the same request returns the stored report.
#[test]
fn sec_02_a_stop_engaged_at_the_focusing_click_sends_no_key() {
    let mut r = typing_rig();
    r.world().focused = Some(name_field());
    let page = r.observe();
    r.world().events.clear();
    r.stop_at("click", 1);
    let request = typing(&page, "t1", ExpectedEffect::None);
    let result = r.engine.execute_action(&request);
    assert_clicked_but_not_typed(&r, &result);
    assert!(result.input_delivered(), "the focusing click was delivered");

    let metrics = r.engine.engine_metrics();
    r.world().events.clear();
    let replay = r.engine.execute_action(&request);
    assert_eq!(replay, result);
    assert!(r.world().events.is_empty(), "a replay sends nothing");
    assert_eq!(
        r.engine.engine_metrics().delivered_actions,
        metrics.delivered_actions
    );
}

/// Two-phase `type`, while keyboard focus is awaited: the operator stops the engine at the first
/// read of focus, focus arrives at the second, and the stop is still read before the keys.
#[test]
fn sec_02_a_stop_engaged_while_keyboard_focus_is_awaited_sends_no_key() {
    let mut r = typing_rig();
    r.world().focus_script = VecDeque::from([None, Some(name_field())]);
    let page = r.observe();
    r.world().events.clear();
    r.stop_at("focus", 1);
    let result = r
        .engine
        .execute_action(&typing(&page, "t1", ExpectedEffect::None));
    assert_clicked_but_not_typed(&r, &result);
    assert_eq!(
        r.world().events.iter().filter(|e| **e == "focus").count(),
        2,
        "the engine waited for focus and then read the stop"
    );
}

/// Two-phase `type` with a visual effect: the reference frame is captured after the focusing
/// click and before the keys, and the operator's stop engaged at that capture still stops the
/// keys.
#[test]
fn sec_02_a_stop_engaged_before_the_last_look_of_a_visual_type_sends_no_key() {
    let mut r = typing_rig();
    r.world().focused = Some(name_field());
    r.world().react_to_click = true;
    let page = r.observe();
    r.world().events.clear();
    // The first capture is the freshness check, the second is the reference frame.
    r.stop_at("capture", 2);
    let result = r
        .engine
        .execute_action(&typing(&page, "t1", ExpectedEffect::VisualChange));
    assert_clicked_but_not_typed(&r, &result);
}

/// Two-phase `type`: the last look passed, then the keys went out. The operator's stop engaged
/// while they were handed to the OS is reported as delivered; nothing is rolled back, and the
/// next action is refused.
#[test]
fn sec_02_a_stop_engaged_while_the_keys_go_out_is_reported_and_not_rolled_back() {
    let mut r = typing_rig();
    r.world().focused = Some(name_field());
    let page = r.observe();
    r.world().events.clear();
    r.stop_at("keys", 1);
    let result = r
        .engine
        .execute_action(&typing(&page, "t1", ExpectedEffect::None));
    assert_eq!(result.status, ActionStatus::InputDelivered);
    assert!(result.input_delivered());
    {
        let w = r.world();
        assert_eq!((w.clicks.len(), w.keys.len()), (1, 1));
    }

    let page = r.observe();
    r.world().events.clear();
    let next = r
        .engine
        .execute_action(&typing(&page, "t2", ExpectedEffect::None));
    assert_policy_denied(&next, STOPPED);
    assert!(r.world().events.is_empty(), "no new events");
}

/// A `request_id` of an earlier delivery, replayed under the stop after the journal has seen
/// more requests than it keeps. The result that reported input is part of the record of the
/// session (chapter 02, SDD §2.9): the replay returns it, and nothing is sent.
#[test]
fn sec_02_a_delivery_stays_answerable_under_the_stop_after_the_journal_has_seen_other_requests() {
    let mut r = rig(form());
    r.world().live = true;
    r.set_policy(live_policy());
    let page = r.observe();
    let delivered = r
        .engine
        .execute_action(&live_click(&page, "r0", ExpectedEffect::None));
    assert_eq!(delivered.status, ActionStatus::InputDelivered);
    // The delivery spent the snapshot, so every later request is refused, and each refusal is
    // journaled.
    for i in 1..=JOURNAL_CAPACITY {
        let refused =
            r.engine
                .execute_action(&live_click(&page, &format!("r{i}"), ExpectedEffect::None));
        assert_eq!(error_code(&refused), Some(ErrorCode::StaleSnapshot));
    }
    r.world().events.clear();
    r.engage_operator_stop();
    let replay = r
        .engine
        .execute_action(&live_click(&page, "r0", ExpectedEffect::None));
    assert_eq!(replay, delivered, "the delivery is reported as it happened");
    assert!(r.world().events.is_empty(), "a replay sends nothing");
    assert_eq!(r.world().delivered.len(), 1);
}

/// SEC-02 across a restart: the engine that starts again has a session id of its own (by
/// default the start time and the process id), so a request built on the identifiers of the
/// previous engine is refused before the gate reads the stop or the screen.
#[test]
fn sec_02_a_restarted_engine_with_its_own_session_id_refuses_an_old_request() {
    let mut before = rig(form());
    before.world().live = true;
    before.set_policy(live_policy());
    let page = before.observe();
    let first = before
        .engine
        .execute_action(&live_click(&page, "r1", ExpectedEffect::None));
    assert_eq!(first.status, ActionStatus::InputDelivered);

    // The restarted process: the same configuration and the same screen, another session id.
    let mut after = rig_with(form(), EngineConfig::new("restarted-session"));
    after.world().live = true;
    after.set_policy(live_policy());
    after.observe();
    after.world().events.clear();
    let replay = after
        .engine
        .execute_action(&live_click(&page, "r1", ExpectedEffect::None));
    assert_eq!(error_code(&replay), Some(ErrorCode::SessionMismatch));
    assert!(
        after.world().events.is_empty(),
        "no event reached the actuator"
    );
    assert!(
        after.world().delivered.is_empty(),
        "the restarted engine delivered nothing"
    );
}
