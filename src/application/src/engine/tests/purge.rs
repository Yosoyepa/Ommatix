//! SEC-07 (chapter 08 §8.4), the operator's purge: `[safety] purge_file` asks the engine to
//! delete its state and cache. The engine honours the request at a request boundary, once
//! per instance of the file, and never in the middle of a request.

use std::sync::atomic::Ordering;

use super::*;

/// The operator creates the purge file: a new instance of it.
fn operator_creates_the_purge_file(r: &Rig) {
    r.policy.2.create();
}

#[test]
fn sec_07_the_operator_purge_is_honoured_at_a_request_boundary_once_per_instance() {
    let mut r = rig(form());
    let page = r.observe();
    r.engine.honour_purge_request();
    assert!(
        r.engine.last_timings().is_some(),
        "without a request nothing is deleted"
    );
    assert_eq!(r.policy.2.purges(), 0);

    operator_creates_the_purge_file(&r);
    r.engine.honour_purge_request();
    assert!(r.engine.last_timings().is_none(), "the state is deleted");
    assert_eq!(r.policy.2.purges(), 1);
    let refused = r
        .engine
        .execute_action(&Rig::action(&page, "Save", "r1", ActionKind::Click));
    assert_eq!(error_code(&refused), Some(ErrorCode::StaleSnapshot));
    assert!(!refused.input_delivered());

    // The same instance is not honoured again, however many boundaries follow.
    r.observe();
    r.engine.honour_purge_request();
    r.engine.honour_purge_request();
    assert!(
        r.engine.last_timings().is_some(),
        "an observation made after the purge is kept"
    );
    assert_eq!(r.policy.2.purges(), 1);

    // A new instance of the file is a new request.
    operator_creates_the_purge_file(&r);
    r.engine.honour_purge_request();
    assert!(r.engine.last_timings().is_none());
    assert_eq!(r.policy.2.purges(), 2);
}

#[test]
fn sec_07_the_operator_purge_is_never_taken_in_the_middle_of_a_request() {
    let mut r = rig(form());
    r.world().live = true;
    r.set_policy(live_policy());
    let page = r.observe();
    // The operator creates the file while the engine reads the policy for this action.
    r.policy.2.create_on_read.store(true, Ordering::SeqCst);
    let result = r
        .engine
        .execute_action(&live_click(&page, "r1", ExpectedEffect::None));
    assert!(
        result.input_delivered(),
        "the request ran on its own snapshot"
    );
    assert_eq!(r.world().delivered.len(), 1);
    assert_eq!(
        r.policy.2.purges(),
        0,
        "nothing was deleted inside the request"
    );
    assert!(r.engine.last_timings().is_some());

    // The next request boundary takes the request.
    r.engine.honour_purge_request();
    assert_eq!(r.policy.2.purges(), 1);
    assert!(r.engine.last_timings().is_none());
}

#[test]
fn sec_07_an_action_on_a_purged_snapshot_delivers_nothing() {
    let mut r = rig(form());
    r.world().live = true;
    r.set_policy(live_policy());
    let page = r.observe();
    operator_creates_the_purge_file(&r);
    r.engine.honour_purge_request();

    let result = r
        .engine
        .execute_action(&live_click(&page, "r1", ExpectedEffect::None));
    assert_eq!(error_code(&result), Some(ErrorCode::StaleSnapshot));
    assert!(!result.input_delivered());
    assert!(
        r.world().delivered.is_empty(),
        "no input reached the actuator"
    );
    assert_eq!(r.engine.engine_metrics().delivered_actions, 0);
}

#[test]
fn sec_07_an_operator_purge_never_issues_a_target_or_snapshot_identifier_twice() {
    let mut r = rig(form());
    let before = r.observe();
    let old_ids: Vec<String> = before
        .elements
        .iter()
        .map(|e| e.target_id.to_string())
        .collect();

    operator_creates_the_purge_file(&r);
    r.engine.honour_purge_request();
    let after = r.observe();

    assert_eq!(after.elements.len(), before.elements.len());
    for element in &after.elements {
        let id = element.target_id.to_string();
        assert!(!old_ids.contains(&id), "{id} was issued before the purge");
    }
    assert!(snapshot_number(&after) > snapshot_number(&before));
}
