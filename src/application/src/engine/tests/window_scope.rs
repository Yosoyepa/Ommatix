//! The operator's scope on windows (ADR-0020) at the action gate: an allowed application
//! that also owns the desktop and the taskbar (`explorer.exe`) is acted on only in the
//! windows the policy lists, checked at the gate and again right before delivery.

use super::typing::{name_field, typing, typing_rig};
use super::*;
use crate::ports::WindowPin;

/// The class of a File Explorer window, the one the operator lists for the application.
const CABINET: &str = "CabinetWClass";
const OPENED: u64 = 0x0A0B;

fn window(class: &str, handle: u64) -> TopLevelWindow {
    TopLevelWindow {
        class: class.to_owned(),
        handle,
    }
}

fn scoped_policy() -> LocalPolicy {
    LocalPolicy {
        window_classes: [(APP.to_owned(), vec![CABINET.to_owned()])].into(),
        ..live_policy()
    }
}

/// A live rig whose actuator attests `foreground` in front and `at_point` under the click.
fn rig_with_windows(
    policy: LocalPolicy,
    foreground: Option<TopLevelWindow>,
    at_point: Option<TopLevelWindow>,
) -> Rig {
    let r = rig(form());
    {
        let mut w = r.world();
        w.live = true;
        w.foreground_window = foreground;
        w.window_at_point = at_point;
    }
    r.set_policy(policy);
    r
}

fn click(r: &mut Rig, id: &str) -> ActionResult {
    let page = r.observe();
    r.engine
        .execute_action(&live_click(&page, id, ExpectedEffect::None))
}

const SCOPE_CLASS: &str = "a window is not of a class the local policy allows for this application";

#[test]
fn the_allowed_window_receives_the_click() {
    let cabinet = window(CABINET, OPENED);
    let mut r = rig_with_windows(scoped_policy(), Some(cabinet.clone()), Some(cabinet));
    let result = click(&mut r, "r1");
    assert_eq!(result.status, ActionStatus::InputDelivered, "{result:?}");
    assert_eq!(r.world().delivered.len(), 1);
}

#[test]
fn the_desktop_and_the_taskbar_get_nothing_even_though_their_process_is_allowed() {
    let cabinet = window(CABINET, OPENED);
    for (case, foreground, at_point) in [
        (
            "the desktop is in front",
            window("Progman", 1),
            window("Progman", 1),
        ),
        (
            "the desktop worker is in front",
            window("WorkerW", 2),
            window("WorkerW", 2),
        ),
        (
            "the taskbar is in front",
            window("Shell_TrayWnd", 3),
            window("Shell_TrayWnd", 3),
        ),
        (
            "the point is on the taskbar over an Explorer window",
            cabinet.clone(),
            window("Shell_TrayWnd", 3),
        ),
        (
            "the point is on a shell pop-up over an Explorer window",
            cabinet.clone(),
            window("#32768", 4),
        ),
    ] {
        let mut r = rig_with_windows(scoped_policy(), Some(foreground), Some(at_point));
        let result = click(&mut r, "r1");
        assert_policy_denied(&result, SCOPE_CLASS);
        assert!(r.world().delivered.is_empty(), "{case}");
        assert_eq!(r.engine.engine_metrics().delivered_actions, 0, "{case}");
    }
}

#[test]
fn another_window_of_the_listed_class_under_the_point_gets_nothing() {
    let mut r = rig_with_windows(
        scoped_policy(),
        Some(window(CABINET, OPENED)),
        Some(window(CABINET, OPENED + 1)),
    );
    let result = click(&mut r, "r1");
    assert_policy_denied(&result, "the target point is not in the foreground window");
    assert!(r.world().delivered.is_empty(), "nothing was delivered");
}

#[test]
fn windows_the_actuator_cannot_attest_get_nothing() {
    let cabinet = window(CABINET, OPENED);
    for (foreground, at_point) in [
        (None, Some(cabinet.clone())),
        (Some(cabinet.clone()), None),
        (None, None),
    ] {
        let mut r = rig_with_windows(scoped_policy(), foreground, at_point);
        let result = click(&mut r, "r1");
        assert_policy_denied(
            &result,
            "the window scope cannot be checked: the actuator cannot attest the windows",
        );
        assert!(r.world().delivered.is_empty(), "nothing was delivered");
    }
}

/// The scope is read again right before delivery: a window that takes the foreground
/// between the gate and the events sends nothing (the same race as the foreground
/// application, SEC-05).
#[test]
fn a_window_change_right_before_delivery_sends_nothing() {
    let cabinet = window(CABINET, OPENED);
    let mut r = rig_with_windows(
        scoped_policy(),
        Some(cabinet.clone()),
        Some(cabinet.clone()),
    );
    let page = r.observe();
    // Reads: the gate, then the last look before the events.
    r.world().foreground_window_script =
        VecDeque::from([Some(cabinet.clone()), Some(window("Progman", 1))]);
    let result = r
        .engine
        .execute_action(&live_click(&page, "r1", ExpectedEffect::None));
    assert_policy_denied(&result, SCOPE_CLASS);
    assert!(r.world().delivered.is_empty(), "nothing was delivered");
    // The operator narrowing the scope meanwhile is honoured too: the policy is read again.
    let page = r.observe();
    r.script_policy([live_policy(), scoped_policy()]);
    r.world().foreground_window = Some(window("Progman", 1));
    r.world().window_at_point = Some(window("Progman", 1));
    let result = r
        .engine
        .execute_action(&live_click(&page, "r2", ExpectedEffect::None));
    assert_policy_denied(&result, SCOPE_CLASS);
    assert!(r.world().delivered.is_empty(), "nothing was delivered");
}

#[test]
fn without_a_scope_the_actuator_is_not_asked_about_windows() {
    let mut r = rig_with_windows(live_policy(), None, None);
    let result = click(&mut r, "r1");
    assert_eq!(result.status, ActionStatus::InputDelivered, "{result:?}");
    let asked = r
        .world()
        .events
        .iter()
        .any(|e| matches!(*e, "foreground_window" | "window_at"));
    assert!(!asked, "the default policy costs no window query");
}

#[test]
fn a_scope_on_another_application_does_not_restrict_this_one() {
    let policy = LocalPolicy {
        window_classes: [("explorer.exe".to_owned(), vec![CABINET.to_owned()])].into(),
        ..live_policy()
    };
    let mut r = rig_with_windows(policy, None, None);
    let result = click(&mut r, "r1");
    assert_eq!(result.status, ActionStatus::InputDelivered, "{result:?}");
}

#[test]
fn the_pin_lets_only_the_recorded_window_receive_input() {
    let pinned = LocalPolicy {
        window_pin: Some(WindowPin::Window(OPENED)),
        ..scoped_policy()
    };
    let recorded = window(CABINET, OPENED);
    let mut r = rig_with_windows(pinned.clone(), Some(recorded.clone()), Some(recorded));
    let result = click(&mut r, "r1");
    assert_eq!(result.status, ActionStatus::InputDelivered, "{result:?}");

    // A non-recorded Explorer window: the right class, another handle.
    let other = window(CABINET, OPENED + 1);
    let mut r = rig_with_windows(pinned, Some(other.clone()), Some(other));
    let result = click(&mut r, "r1");
    assert_policy_denied(
        &result,
        "the foreground window is not the window the local policy pins",
    );
    assert!(r.world().delivered.is_empty(), "nothing was delivered");
}

#[test]
fn a_configured_pin_with_no_window_allows_no_input() {
    let unset = LocalPolicy {
        window_pin: Some(WindowPin::Unset),
        ..live_policy()
    };
    let cabinet = window(CABINET, OPENED);
    let mut r = rig_with_windows(unset, Some(cabinet.clone()), Some(cabinet));
    let result = click(&mut r, "r1");
    assert_policy_denied(
        &result,
        "the local policy pins a window and none is pinned now",
    );
    assert!(r.world().delivered.is_empty(), "nothing was delivered");
}

/// `type` in two phases: the scope is looked at again after the focusing click and before
/// the keys. A window change in between leaves the click delivered and no key sent.
#[test]
fn a_window_change_after_the_focusing_click_sends_no_key() {
    let cabinet = window(CABINET, OPENED);
    let r = typing_rig_with(
        scoped_policy(),
        Some(cabinet.clone()),
        Some(cabinet.clone()),
    );
    let mut r = r;
    let page = r.observe();
    r.world().focused = Some(name_field());
    // Reads: the gate, the last look before the click, and the last look before the keys.
    r.world().foreground_window_script = VecDeque::from([
        Some(cabinet.clone()),
        Some(cabinet),
        Some(window("Shell_TrayWnd", 3)),
    ]);
    let result = r
        .engine
        .execute_action(&typing(&page, "t1", ExpectedEffect::None));
    assert_eq!(result.status, ActionStatus::Indeterminate, "{result:?}");
    assert!(result.input_delivered(), "the click was delivered");
    assert_eq!(r.world().clicks.len(), 1);
    assert!(r.world().keys.is_empty(), "no key was sent");
}

/// The same `type` with every window in scope sends its keys.
#[test]
fn type_in_the_allowed_window_sends_its_keys() {
    let cabinet = window(CABINET, OPENED);
    let mut r = typing_rig_with(scoped_policy(), Some(cabinet.clone()), Some(cabinet));
    let page = r.observe();
    r.world().focused = Some(name_field());
    let result = r
        .engine
        .execute_action(&typing(&page, "t1", ExpectedEffect::None));
    assert_eq!(result.status, ActionStatus::InputDelivered, "{result:?}");
    assert_eq!(r.world().keys.len(), 1);
}

/// A live rig for `type` in phases with the given window scope.
fn typing_rig_with(
    policy: LocalPolicy,
    foreground: Option<TopLevelWindow>,
    at_point: Option<TopLevelWindow>,
) -> Rig {
    let r = typing_rig();
    {
        let mut w = r.world();
        w.foreground_window = foreground;
        w.window_at_point = at_point;
    }
    r.set_policy(policy);
    r
}
