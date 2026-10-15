//! `type` in two phases (ADR-0018, SDD §2.9) over the [`Actuator`] and [`Access`] doubles:
//! the focusing click, keyboard focus confirmed on the target element, then the keys.

use super::*;

/// The `Name` field of [`form`] as the accessibility tree reports its keyboard focus.
pub(super) fn name_field() -> AccessibleElement {
    AccessibleElement {
        bbox: bx(100.0, 40.0, 300.0, 70.0),
        role: Role::Input,
        name: "Name".into(),
        enabled: true,
        offscreen: false,
        password: false,
        patterns: AccessiblePatterns {
            editable_value: true,
            ..AccessiblePatterns::default()
        },
        native: None,
    }
}

/// A live rig whose actuator delivers `type` in phases and whose accessibility provider
/// reads keyboard focus.
pub(super) fn typing_rig() -> Rig {
    let r = rig(form());
    {
        let mut w = r.world();
        w.live = true;
        w.phased = true;
        w.reads_focus = true;
    }
    r.set_policy(live_policy());
    r
}

pub(super) fn typing(page: &ObservePage, id: &str, expected: ExpectedEffect) -> ActionRequest {
    ActionRequest {
        dry_run: false,
        expected_effect: expected,
        ..Rig::action(
            page,
            "Name",
            id,
            ActionKind::Type {
                text: "hola".into(),
            },
        )
    }
}

/// A named way to leave keyboard focus off the target.
type Setup = (&'static str, Box<dyn Fn(&mut World)>);

/// The recorded events that matter to the phases, in order.
fn phases(r: &Rig, names: &[&str]) -> Vec<&'static str> {
    r.world()
        .events
        .iter()
        .copied()
        .filter(|e| names.contains(e))
        .collect()
}

fn assert_clicked_but_not_typed(r: &Rig, result: &ActionResult) {
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
    assert_eq!(result.observed_effect, Some(ObservedEffect::Unknown));
    assert!(!result.verified());
    assert_eq!(
        r.world().clicks.len(),
        1,
        "the focusing click was delivered"
    );
    assert!(r.world().keys.is_empty(), "no key was sent");
    assert!(
        r.world().delivered.is_empty(),
        "not through the one-shot path"
    );
}

/// Finding 1 of the S4 UI Automation review: the keys must not race the focusing click.
#[test]
fn type_sends_no_keys_until_the_target_field_has_keyboard_focus() {
    let mut r = typing_rig();
    let page = r.observe();
    // Focus is elsewhere at first (the application is still handling the click), then
    // nowhere, then on the field.
    let save = AccessibleElement {
        bbox: bx(400.0, 250.0, 480.0, 280.0),
        role: Role::Button,
        name: "Save".into(),
        ..name_field()
    };
    r.world().focus_script = VecDeque::from([Some(save), None, Some(name_field())]);
    r.world().events.clear();
    let started = r.clock.now();

    let result = r
        .engine
        .execute_action(&typing(&page, "t1", ExpectedEffect::None));

    assert_eq!(result.status, ActionStatus::InputDelivered, "{result:?}");
    assert!(result.input_delivered() && !result.verified());
    // The click first, three reads of focus (the third confirms it), and only then the keys.
    assert_eq!(
        phases(&r, &["click", "focus", "keys"]),
        ["click", "focus", "focus", "focus", "keys"]
    );
    // Two polls of 50 ms on the injected clock; nothing slept after the confirmation.
    assert_eq!(
        r.clock.now().saturating_sub(started),
        Duration::from_millis(100)
    );
    let w = r.world();
    assert_eq!((w.clicks.len(), w.keys.len()), (1, 1));
    assert!(w.delivered.is_empty(), "not through the one-shot path");
    assert_eq!(
        w.keys[0].kind(),
        &ActionKind::Type {
            text: "hola".into()
        }
    );
    // The click aims at the field's centre, like any other action.
    assert_eq!(w.clicks[0].point(), (200.0, 55.0));
    drop(w);
    let m = r.engine.engine_metrics();
    assert_eq!((m.delivered_actions, m.rejected_actions), (1, 0));
}

/// Owner decision 1(d): no focus in time means no key, a delivered click reported as
/// `indeterminate`, and never a retry.
#[test]
fn type_without_focus_confirmation_is_indeterminate_and_sends_no_keys() {
    let wrong_password = AccessibleElement {
        password: true,
        ..name_field()
    };
    let disabled = AccessibleElement {
        enabled: false,
        ..name_field()
    };
    let setups: Vec<Setup> = vec![
        ("nothing has focus", Box::new(|_| {})),
        (
            "the provider fails",
            Box::new(|w: &mut World| w.fail_focus = true),
        ),
        (
            "a password field has focus",
            Box::new(move |w: &mut World| w.focused = Some(wrong_password.clone())),
        ),
        (
            "a disabled field has focus",
            Box::new(move |w: &mut World| w.focused = Some(disabled.clone())),
        ),
    ];
    for (case, setup) in setups {
        let mut r = typing_rig();
        let page = r.observe();
        setup(&mut r.world());
        r.world().events.clear();
        let started = r.clock.now();

        let request = typing(&page, "t1", ExpectedEffect::None);
        let result = r.engine.execute_action(&request);

        assert_clicked_but_not_typed(&r, &result);
        assert!(result.input_delivered(), "{case}: the click was delivered");
        assert!(
            result
                .error
                .as_ref()
                .is_some_and(|e| e.message.contains("no key was sent")),
            "{case}: {result:?}"
        );
        // The whole verification window was used on the injected clock: reads at 0, 50, ...
        // 500 ms, 11 in all, and nothing else ran after them.
        assert_eq!(
            r.clock.now().saturating_sub(started),
            VerifyParams::DEFAULT_TIMEOUT,
            "{case}"
        );
        let seen = phases(&r, &["click", "focus", "keys"]);
        assert_eq!(seen.first(), Some(&"click"), "{case}");
        assert_eq!(seen.iter().filter(|e| **e == "focus").count(), 11, "{case}");
        assert!(!seen.contains(&"keys"), "{case}");

        // Never retried: the same request replays the stored result with no new events,
        // and the spent snapshot refuses any other action.
        let events = r.world().events.len();
        assert_eq!(r.engine.execute_action(&request), result, "{case}");
        assert_eq!(r.world().events.len(), events, "{case}");
        let again = r
            .engine
            .execute_action(&typing(&page, "t2", ExpectedEffect::None));
        assert_eq!(error_code(&again), Some(ErrorCode::StaleSnapshot), "{case}");
        assert_eq!(r.world().clicks.len(), 1, "{case}");
        let m = r.engine.engine_metrics();
        assert_eq!((m.delivered_actions, m.rejected_actions), (1, 1), "{case}");
    }
}

#[test]
fn focus_on_another_element_is_not_accepted() {
    let mut r = typing_rig();
    let page = r.observe();
    // Keyboard focus sits in another input of the same window: right application, right
    // kind of control, wrong element.
    r.world().focused = Some(AccessibleElement {
        bbox: bx(100.0, 120.0, 300.0, 150.0),
        name: "Other".into(),
        ..name_field()
    });
    let result = r
        .engine
        .execute_action(&typing(&page, "t1", ExpectedEffect::None));
    assert_clicked_but_not_typed(&r, &result);
}

/// Finding 2 of the S4 UI Automation review: the focusing click's own redraw is not the
/// effect of typing.
#[test]
fn type_visual_change_is_measured_after_the_focusing_click() {
    let mut r = typing_rig();
    {
        let mut w = r.world();
        w.focused = Some(name_field());
        // The click repaints the screen (focus ring, caret); the keys change nothing.
        w.react_to_click = true;
    }
    let page = r.observe();
    r.world().events.clear();

    let result = r
        .engine
        .execute_action(&typing(&page, "t1", ExpectedEffect::VisualChange));

    // Before the fix this was `effect_observed`, an engine false success.
    assert_eq!(result.status, ActionStatus::Indeterminate, "{result:?}");
    assert_eq!(error_code(&result), Some(ErrorCode::IndeterminateEffect));
    assert!(result.input_delivered(), "the keys were delivered");
    assert_eq!(r.world().keys.len(), 1);
    // The reference frame is captured after the click and before the keys (the first
    // capture is the freshness check).
    let seen = phases(&r, &["capture", "click", "keys"]);
    assert_eq!(&seen[..4], ["capture", "click", "capture", "keys"]);
}

#[test]
fn type_visual_change_after_the_keys_is_an_observed_effect() {
    let mut r = typing_rig();
    {
        let mut w = r.world();
        w.focused = Some(name_field());
        w.react_to_click = true;
        w.react_to_input = true;
    }
    let page = r.observe();

    let result = r
        .engine
        .execute_action(&typing(&page, "t1", ExpectedEffect::VisualChange));

    assert_eq!(result.status, ActionStatus::EffectObserved, "{result:?}");
    assert_eq!(result.observed_effect, Some(ObservedEffect::VisualChange));
    assert!(result.input_delivered() && result.verified());
}

#[test]
fn a_click_redraw_alone_is_not_claimed_when_no_effect_is_expected() {
    let mut r = typing_rig();
    {
        let mut w = r.world();
        w.focused = Some(name_field());
        w.react_to_click = true;
    }
    let page = r.observe();
    let result = r
        .engine
        .execute_action(&typing(&page, "t1", ExpectedEffect::None));
    assert_eq!(result.status, ActionStatus::InputDelivered);
    assert_eq!(result.observed_effect, None);
}

#[test]
fn without_accessibility_focus_type_keeps_the_window_level_behaviour() {
    // Provider off, actuator able to split: the actuator's own focus check is all there is.
    let mut r = typing_rig();
    r.world().reads_focus = false;
    let page = r.observe();
    r.world().events.clear();
    let result = r
        .engine
        .execute_action(&typing(&page, "t1", ExpectedEffect::None));
    assert_eq!(result.status, ActionStatus::InputDelivered);
    let w = r.world();
    assert_eq!(w.delivered.len(), 1, "one call, as before");
    assert!(w.clicks.is_empty() && w.keys.is_empty());
    assert!(!w.events.contains(&"focus"), "focus was never read");
    drop(w);

    // Provider on, actuator that cannot split (the interim `enigo` input, for one).
    let mut r = typing_rig();
    {
        let mut w = r.world();
        w.phased = false;
        w.focused = Some(name_field());
    }
    let page = r.observe();
    let result = r
        .engine
        .execute_action(&typing(&page, "t1", ExpectedEffect::None));
    assert_eq!(result.status, ActionStatus::InputDelivered);
    let w = r.world();
    assert_eq!(w.delivered.len(), 1);
    assert!(w.clicks.is_empty() && w.keys.is_empty());
}

#[test]
fn clicks_and_scrolls_never_use_the_phases_or_read_focus() {
    let mut r = typing_rig();
    let page = r.observe();
    r.world().events.clear();
    let mut click = Rig::action(&page, "Save", "r1", ActionKind::Click);
    click.dry_run = false;
    assert_eq!(
        r.engine.execute_action(&click).status,
        ActionStatus::InputDelivered
    );
    let w = r.world();
    assert_eq!(w.delivered.len(), 1);
    assert!(w.clicks.is_empty() && w.keys.is_empty());
    assert!(!w.events.contains(&"focus"));
}

#[test]
fn a_stop_or_a_foreground_change_after_the_click_sends_no_keys() {
    let stopped = LocalPolicy {
        stopped: true,
        ..live_policy()
    };
    let withdrawn = LocalPolicy {
        actuation_enabled: false,
        ..live_policy()
    };
    for (case, policy) in [("operator stop", stopped), ("consent withdrawn", withdrawn)] {
        let mut r = typing_rig();
        r.world().focused = Some(name_field());
        let page = r.observe();
        // Reads: the gate, the last look before the click, the last look before the keys.
        r.script_policy([live_policy(), live_policy(), policy]);
        let result = r
            .engine
            .execute_action(&typing(&page, "t1", ExpectedEffect::None));
        assert_clicked_but_not_typed(&r, &result);
        // The delivered click keeps its reported status: no rollback.
        assert!(result.input_delivered(), "{case}");
        assert_eq!(r.engine.engine_metrics().delivered_actions, 1, "{case}");
    }
    for thief in [Some("other-app".to_owned()), None] {
        let mut r = typing_rig();
        r.world().focused = Some(name_field());
        let page = r.observe();
        // Attested at the gate and before the click; another application before the keys.
        r.world().foreground_script =
            VecDeque::from([Some(APP.to_owned()), Some(APP.to_owned()), thief]);
        let result = r
            .engine
            .execute_action(&typing(&page, "t1", ExpectedEffect::None));
        assert_clicked_but_not_typed(&r, &result);
    }
}

#[test]
fn a_failed_click_phase_reads_no_focus_and_sends_no_keys() {
    let mut r = typing_rig();
    r.world().focused = Some(name_field());
    let page = r.observe();
    r.world().receipt = Err(());
    r.world().events.clear();
    let result = r
        .engine
        .execute_action(&typing(&page, "t1", ExpectedEffect::None));
    assert_eq!(result.status, ActionStatus::Indeterminate);
    assert!(!result.input_delivered(), "the click itself is uncertain");
    assert!(!r.world().events.contains(&"focus"));
    assert_eq!(r.world().keys.len(), 0);
}

#[test]
fn a_failed_key_phase_is_indeterminate_after_a_delivered_click() {
    let mut r = typing_rig();
    {
        let mut w = r.world();
        w.focused = Some(name_field());
        w.fail_keys = true;
    }
    let page = r.observe();
    let result = r
        .engine
        .execute_action(&typing(&page, "t1", ExpectedEffect::None));
    assert_eq!(result.status, ActionStatus::Indeterminate);
    assert!(result.input_delivered(), "the click was delivered");
    assert_eq!(error_code(&result), Some(ErrorCode::IndeterminateEffect));
    assert_eq!(r.world().keys.len(), 0);
}

#[test]
fn a_zero_verification_window_reads_focus_once() {
    let mut config = EngineConfig::new(SESSION);
    config.verify = VerifyParams {
        timeout: Duration::ZERO,
        poll: Duration::from_millis(50),
    };
    let mut r = rig_with(form(), config);
    {
        let mut w = r.world();
        w.live = true;
        w.phased = true;
        w.reads_focus = true;
    }
    r.set_policy(live_policy());
    let page = r.observe();
    r.world().events.clear();
    let result = r
        .engine
        .execute_action(&typing(&page, "t1", ExpectedEffect::None));
    assert_clicked_but_not_typed(&r, &result);
    assert_eq!(phases(&r, &["focus"]), ["focus"]);
}

// ---- the time of each stage of an action (S4 third round, finding 6) ----

/// Every action the gate ran reports where its time went; the stages it did not reach are
/// zero.
#[test]
fn an_action_reports_the_time_of_each_of_its_stages() {
    let mut r = typing_rig();
    let page = r.observe();
    assert_eq!(r.engine.last_action_timings(), None, "no action yet");
    let ms = Duration::from_millis;

    // Two-phase type: focus arrives on the third read (two polls of 50 ms).
    let save = AccessibleElement {
        bbox: bx(400.0, 250.0, 480.0, 280.0),
        role: Role::Button,
        name: "Save".into(),
        ..name_field()
    };
    r.world().focus_script = VecDeque::from([Some(save), None, Some(name_field())]);
    r.engine
        .execute_action(&typing(&page, "t1", ExpectedEffect::None));
    let t = r.engine.last_action_timings().expect("timings");
    assert_eq!(
        (
            t.gate,
            t.freshness,
            t.delivery,
            t.focus,
            t.verification,
            t.total
        ),
        (ms(0), ms(0), ms(0), ms(100), ms(0), ms(100))
    );

    // A live click that shows no effect waits out the verification window.
    let page = r.observe();
    r.world().phased = false;
    let click = ActionRequest {
        dry_run: false,
        expected_effect: ExpectedEffect::VisualChange,
        ..Rig::action(&page, "Save", "c1", ActionKind::Click)
    };
    let result = r.engine.execute_action(&click);
    assert_eq!(result.status, ActionStatus::Indeterminate);
    let t = r.engine.last_action_timings().expect("timings");
    assert_eq!(t.verification, VerifyParams::DEFAULT_TIMEOUT);
    assert_eq!((t.gate, t.focus), (ms(0), ms(0)));
    assert_eq!(t.total, VerifyParams::DEFAULT_TIMEOUT);

    // A dry run has a gate and nothing else; a replay and a refusal before the gate have
    // no timings of their own.
    let page = r.observe();
    let dry = Rig::action(&page, "Save", "d1", ActionKind::Click);
    r.engine.execute_action(&dry);
    assert_eq!(
        r.engine.last_action_timings(),
        Some(ActionTimings::default())
    );
    r.engine.execute_action(&dry);
    assert_eq!(r.engine.last_action_timings(), None, "a replay");
    let too_many = ActionRequest {
        request_id: String::new(),
        ..dry
    };
    r.engine.execute_action(&too_many);
    assert_eq!(
        r.engine.last_action_timings(),
        None,
        "refused before the gate"
    );
}
