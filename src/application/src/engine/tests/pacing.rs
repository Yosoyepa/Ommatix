//! A `type` the actuator refuses before sending anything (decision D49, review O4r): a paced
//! `type` whose pauses would hold the engine longer than the actuator allows. The agent is told
//! the truth for each of the three cases: refused before the first key (a rejection, nothing
//! sent, the snapshot kept), stopped after some keys (`indeterminate`, input delivered), or
//! completed.

use super::typing::{name_field, typing, typing_rig};
use super::*;
use crate::ports::{KeysStopCheck, KeysStopped};

/// What the pacing of the real actuator says when the pauses are too long for the text.
const REFUSAL: &str = "a paced type may take at most 30 s with the pauses the operator \
                       configured; send less text at a time; nothing was sent";

/// A live rig whose actuator refuses every `type` before sending anything.
fn refusing_rig() -> Rig {
    let r = typing_rig();
    {
        let mut w = r.world();
        w.focused = Some(name_field());
        w.delivery_refusal = Some(REFUSAL.to_owned());
    }
    r
}

fn nothing_was_sent(r: &Rig) {
    let w = r.world();
    assert!(
        w.clicks.is_empty() && w.keys.is_empty() && w.delivered.is_empty(),
        "no click, key or delivery reached the actuator"
    );
    assert!(
        !w.events
            .iter()
            .any(|e| matches!(*e, "click" | "keys" | "deliver")),
        "{:?}",
        w.events
    );
}

/// UT-87: the refusal is a rejection the agent can act on, not an uncertain delivery.
#[test]
fn ut_87_an_over_bound_paced_type_is_rejected_before_any_key_with_limit_exceeded() {
    let mut r = refusing_rig();
    let page = r.observe();
    r.world().events.clear();

    let result = r
        .engine
        .execute_action(&typing(&page, "t1", ExpectedEffect::None));

    assert_eq!(result.status, ActionStatus::Rejected, "{result:?}");
    assert!(!result.input_delivered() && !result.verified());
    assert_eq!(error_code(&result), Some(ErrorCode::LimitExceeded));
    let error = result.error.as_ref().expect("a refusal carries its error");
    assert_eq!(error.next_step, NextStep::ReduceRequest);
    assert!(!error.retryable);
    assert!(error.message.contains("nothing was sent"), "{error:?}");
    assert!(!error.message.contains("may or may not"), "{error:?}");
    nothing_was_sent(&r);
    let m = r.engine.engine_metrics();
    assert_eq!((m.delivered_actions, m.rejected_actions), (0, 1));
}

/// UT-87: nothing was sent, so the observation is still good for a smaller request.
#[test]
fn ut_87_the_rejection_does_not_spend_the_snapshot() {
    let mut r = refusing_rig();
    let page = r.observe();
    let refused = r
        .engine
        .execute_action(&typing(&page, "t1", ExpectedEffect::None));
    assert_eq!(refused.status, ActionStatus::Rejected);

    // The operator's pauses no longer refuse (a shorter text, in the real actuator): the same
    // observation is accepted and delivers.
    r.world().delivery_refusal = None;
    let retried = r
        .engine
        .execute_action(&typing(&page, "t2", ExpectedEffect::None));
    assert_eq!(retried.status, ActionStatus::InputDelivered, "{retried:?}");
    assert!(retried.input_delivered());
}

/// UT-87: a dry run tells the refusal a live call would give; `simulated` promises nothing it
/// would not keep.
#[test]
fn ut_87_a_dry_run_of_an_over_bound_type_is_rejected_the_same_way() {
    let mut r = refusing_rig();
    let page = r.observe();
    r.world().events.clear();
    let dry = ActionRequest {
        dry_run: true,
        ..typing(&page, "d1", ExpectedEffect::None)
    };

    let result = r.engine.execute_action(&dry);

    assert_eq!(result.status, ActionStatus::Rejected, "{result:?}");
    assert_eq!(error_code(&result), Some(ErrorCode::LimitExceeded));
    nothing_was_sent(&r);

    // The control: with nothing to refuse, the same dry run is simulated.
    r.world().delivery_refusal = None;
    let ok = r.engine.execute_action(&ActionRequest {
        request_id: "d2".into(),
        ..dry
    });
    assert_eq!(ok.status, ActionStatus::Simulated, "{ok:?}");
}

/// UT-87: the same on the one-shot path (an actuator that does not deliver in phases).
#[test]
fn ut_87_the_one_shot_path_rejects_an_over_bound_type_before_delivery_too() {
    let mut r = refusing_rig();
    {
        let mut w = r.world();
        w.phased = false;
        w.reads_focus = false;
    }
    let page = r.observe();
    r.world().events.clear();

    let result = r
        .engine
        .execute_action(&typing(&page, "t1", ExpectedEffect::None));

    assert_eq!(result.status, ActionStatus::Rejected, "{result:?}");
    assert_eq!(error_code(&result), Some(ErrorCode::LimitExceeded));
    nothing_was_sent(&r);
}

/// UT-87: only the action the actuator refuses is refused; a click goes through.
#[test]
fn ut_87_an_action_the_actuator_does_not_refuse_is_delivered() {
    let mut r = refusing_rig();
    let page = r.observe();
    let click = ActionRequest {
        dry_run: false,
        ..Rig::action(&page, "Save", "c1", ActionKind::Click)
    };
    let result = r.engine.execute_action(&click);
    assert_eq!(result.status, ActionStatus::InputDelivered, "{result:?}");
}

/// The other case, told apart: keys that began and could not be completed are `indeterminate`
/// with the click delivered, and the snapshot is spent: input may have reached the field.
#[test]
fn a_type_stopped_after_some_keys_is_indeterminate_with_input_delivered() {
    let mut r = typing_rig();
    {
        let mut w = r.world();
        w.focused = Some(name_field());
        // The actuator's own error when a check between two characters fails.
        w.fail_keys = true;
    }
    let page = r.observe();

    let result = r
        .engine
        .execute_action(&typing(&page, "t1", ExpectedEffect::None));

    assert_eq!(result.status, ActionStatus::Indeterminate, "{result:?}");
    assert!(result.input_delivered(), "the click was delivered");
    assert_eq!(error_code(&result), Some(ErrorCode::IndeterminateEffect));
    let error = result.error.as_ref().expect("the error");
    assert_eq!(error.next_step, NextStep::ReconcileEffect);
    // Spent: a second action on the same observation is refused as stale.
    let next = r
        .engine
        .execute_action(&typing(&page, "t2", ExpectedEffect::None));
    assert_eq!(
        error_code(&next),
        Some(ErrorCode::StaleSnapshot),
        "{next:?}"
    );
}

/// A text with no lowercase letter and no digit: none of the engine's closed sentences can hold
/// a character of it by accident.
const ASKED: &str = "QXJZ#@~|QXJZ";

fn typing_asked(page: &ObservePage, id: &str) -> ActionRequest {
    ActionRequest {
        action: ActionKind::Type { text: ASKED.into() },
        ..typing(page, id, ExpectedEffect::None)
    }
}

/// A live rig whose paced delivery a check ends after three of the twelve characters.
fn stopped_rig(check: KeysStopCheck) -> Rig {
    let r = typing_rig();
    {
        let mut w = r.world();
        w.focused = Some(name_field());
        w.keys_stopped = Some(KeysStopped {
            check,
            sent_chars: 3,
            of_chars: 12,
        });
    }
    r
}

fn error_message(result: &ActionResult) -> String {
    result
        .error
        .as_ref()
        .map(|e| e.message.clone())
        .unwrap_or_default()
}

/// UT-87 (review O4s): a stop raised after three characters is still `indeterminate`, now with
/// the check and the count in the closed text.
#[test]
fn ut_87_a_paced_delivery_stopped_by_the_stop_says_which_check_and_3_of_12() {
    let mut r = stopped_rig(KeysStopCheck::OperatorStop);
    let page = r.observe();

    let result = r.engine.execute_action(&typing_asked(&page, "t1"));

    assert_eq!(result.status, ActionStatus::Indeterminate, "{result:?}");
    assert!(
        result.input_delivered(),
        "the click and three characters went"
    );
    assert_eq!(error_code(&result), Some(ErrorCode::IndeterminateEffect));
    let message = error_message(&result);
    assert!(
        message.contains("the operator's stop was engaged"),
        "{message}"
    );
    assert!(
        message.contains("3 of 12 characters were sent"),
        "{message}"
    );
    assert!(message.contains("do not retry"), "{message}");
    assert!(
        !message.contains("failed on the keys"),
        "not the generic text: {message}"
    );
}

#[test]
fn ut_87_a_paced_delivery_stopped_by_a_foreground_change_says_so() {
    let mut r = stopped_rig(KeysStopCheck::ForegroundWindow);
    let page = r.observe();

    let result = r.engine.execute_action(&typing_asked(&page, "t1"));

    assert_eq!(result.status, ActionStatus::Indeterminate, "{result:?}");
    let message = error_message(&result);
    assert!(
        message.contains("the foreground window changed"),
        "{message}"
    );
    assert!(
        message.contains("3 of 12 characters were sent"),
        "{message}"
    );
}

/// The agent can tell the five checks apart; each has its own closed text.
#[test]
fn ut_87_each_check_that_can_stop_a_paced_delivery_is_named_apart() {
    let checks = [
        KeysStopCheck::OperatorStop,
        KeysStopCheck::ForegroundWindow,
        KeysStopCheck::Focus,
        KeysStopCheck::HeldKey,
        KeysStopCheck::Pacing,
    ];
    let mut told = Vec::new();
    for check in checks {
        let mut r = stopped_rig(check);
        let page = r.observe();
        let result = r.engine.execute_action(&typing_asked(&page, "t1"));
        assert_eq!(result.status, ActionStatus::Indeterminate, "{check:?}");
        let message = error_message(&result);
        assert!(message.contains(check.describe()), "{check:?}: {message}");
        told.push(message);
    }
    told.sort();
    told.dedup();
    assert_eq!(told.len(), checks.len(), "five distinct messages");
}

/// The message is the engine's closed text and a count: no character of the text asked, even
/// when the adapter's own error message carries it.
#[test]
fn ut_87_the_message_of_a_stopped_delivery_holds_no_character_of_the_text() {
    for check in [KeysStopCheck::OperatorStop, KeysStopCheck::ForegroundWindow] {
        let mut r = stopped_rig(check);
        let page = r.observe();
        let result = r.engine.execute_action(&typing_asked(&page, "t1"));
        let message = error_message(&result);
        assert_ne!(message.len(), 0, "an error message");
        assert!(
            !message.chars().any(|c| ASKED.contains(c)),
            "the text is in {message:?}"
        );
        assert!(!message.contains("stopped while typing"), "{message}");
    }
}

/// An error that carries no stop keeps the generic words, and a type that is not stopped is
/// delivered as before.
#[test]
fn ut_87_a_failure_without_a_stop_and_an_unstopped_type_are_unchanged() {
    let mut r = typing_rig();
    {
        let mut w = r.world();
        w.focused = Some(name_field());
        w.fail_keys = true;
    }
    let page = r.observe();
    let failed = r.engine.execute_action(&typing_asked(&page, "t1"));
    assert_eq!(failed.status, ActionStatus::Indeterminate);
    let message = error_message(&failed);
    assert!(message.contains("failed on the keys"), "{message}");
    assert!(
        message.contains("some may have reached the OS"),
        "{message}"
    );

    let mut r = typing_rig();
    r.world().focused = Some(name_field());
    let page = r.observe();
    let delivered = r.engine.execute_action(&typing_asked(&page, "t2"));
    assert_eq!(
        delivered.status,
        ActionStatus::InputDelivered,
        "{delivered:?}"
    );
    assert!(delivered.error.is_none());
}

/// The same on the one-shot path: some keys went, so the input was delivered.
#[test]
fn ut_87_the_one_shot_path_says_which_check_stopped_a_paced_delivery_too() {
    let mut r = stopped_rig(KeysStopCheck::Focus);
    {
        let mut w = r.world();
        w.phased = false;
        w.reads_focus = false;
    }
    let page = r.observe();

    let result = r.engine.execute_action(&typing_asked(&page, "t1"));

    assert_eq!(result.status, ActionStatus::Indeterminate, "{result:?}");
    assert!(result.input_delivered(), "{result:?}");
    let message = error_message(&result);
    assert!(
        message.contains(KeysStopCheck::Focus.describe()),
        "{message}"
    );
    assert!(
        message.contains("3 of 12 characters were sent"),
        "{message}"
    );
}
