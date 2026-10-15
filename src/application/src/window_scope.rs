//! The operator's scope on windows (ADR-0020, chapter 08): which top-level windows of an
//! allowed application live input may go to.
//!
//! `[safety] allowed_applications` names a process, and a process can own more than the
//! window the operator means: `explorer.exe` owns File Explorer windows, but also the
//! desktop, the taskbar and the shell's pop-ups. This pure decision narrows the allowlist
//! to windows, from what the actuator attests about the foreground window and the window
//! under the action's point. It reads no OS state itself and fails closed: whatever the
//! actuator cannot attest is outside the scope.

use crate::ports::{LocalPolicy, TopLevelWindow, WindowPin};

/// Whether the operator set a scope on windows that applies to `application`: a class list
/// for it, or a pin (which applies to every application).
pub fn window_scope_applies(policy: &LocalPolicy, application: &str) -> bool {
    policy.window_pin.is_some() || policy.window_classes.contains_key(application)
}

/// Why live input to `application` must not go to the foreground window and the window at
/// the action's point, or `None` when the operator's window scope allows it (or sets none
/// for this application).
///
/// With a class list for `application`, both windows must have a listed class, and the
/// point must be in the foreground window itself: another window of the same class that
/// lies under the point is not the window the operator allowed. With a pin, both windows
/// must be the pinned one, and an unset pin allows nothing. The messages are fixed text:
/// they never carry a class, a handle or a title.
pub fn window_scope_problem(
    policy: &LocalPolicy,
    application: &str,
    foreground: Option<&TopLevelWindow>,
    at_point: Option<&TopLevelWindow>,
) -> Option<&'static str> {
    if !window_scope_applies(policy, application) {
        return None;
    }
    let classes = policy.window_classes.get(application);
    let (Some(foreground), Some(at_point)) = (foreground, at_point) else {
        return Some("the window scope cannot be checked: the actuator cannot attest the windows");
    };
    if let Some(classes) = classes {
        let listed = |window: &TopLevelWindow| classes.contains(&window.class);
        if !listed(foreground) || !listed(at_point) {
            return Some("a window is not of a class the local policy allows for this application");
        }
        if at_point.handle != foreground.handle {
            return Some("the target point is not in the foreground window");
        }
    }
    match policy.window_pin {
        None => None,
        Some(WindowPin::Unset) => Some("the local policy pins a window and none is pinned now"),
        Some(WindowPin::Window(pinned)) => (foreground.handle != pinned
            || at_point.handle != pinned)
            .then_some("the foreground window is not the window the local policy pins"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXPLORER: &str = "explorer.exe";
    const OPENED: u64 = 0x0A0B;

    fn window(class: &str, handle: u64) -> TopLevelWindow {
        TopLevelWindow {
            class: class.to_owned(),
            handle,
        }
    }

    fn explorer_policy() -> LocalPolicy {
        LocalPolicy {
            actuation_enabled: true,
            allowed_applications: vec![EXPLORER.to_owned()],
            window_classes: [(EXPLORER.to_owned(), vec!["CabinetWClass".to_owned()])].into(),
            ..LocalPolicy::default()
        }
    }

    fn pinned_policy(pin: WindowPin) -> LocalPolicy {
        LocalPolicy {
            window_pin: Some(pin),
            ..explorer_policy()
        }
    }

    fn check(policy: &LocalPolicy, foreground: &TopLevelWindow, at: &TopLevelWindow) -> bool {
        window_scope_problem(policy, EXPLORER, Some(foreground), Some(at)).is_none()
    }

    #[test]
    fn without_a_scope_nothing_changes() {
        let policy = LocalPolicy {
            allowed_applications: vec![EXPLORER.to_owned()],
            ..LocalPolicy::default()
        };
        // Even a window the actuator cannot attest is not asked about.
        assert_eq!(window_scope_problem(&policy, EXPLORER, None, None), None);
        let desktop = window("Progman", 7);
        assert!(check(&policy, &desktop, &desktop));
    }

    #[test]
    fn an_application_without_an_entry_is_not_restricted_by_class() {
        let policy = explorer_policy();
        let any = window("Notepad", 9);
        assert_eq!(
            window_scope_problem(&policy, "notepad.exe", Some(&any), Some(&any)),
            None
        );
        assert_eq!(
            window_scope_problem(&policy, "notepad.exe", None, None),
            None
        );
    }

    #[test]
    fn the_explorer_window_of_the_listed_class_is_allowed() {
        let cabinet = window("CabinetWClass", OPENED);
        assert!(check(&explorer_policy(), &cabinet, &cabinet));
    }

    #[test]
    fn the_desktop_the_taskbar_and_shell_pop_ups_are_refused() {
        let policy = explorer_policy();
        for class in [
            "Progman",
            "WorkerW",
            "Shell_TrayWnd",
            "Shell_SecondaryTrayWnd",
            "#32768",
            "ApplicationFrameWindow",
        ] {
            let other = window(class, 1);
            assert!(!check(&policy, &other, &other), "{class} in the foreground");
            let cabinet = window("CabinetWClass", OPENED);
            assert!(
                !check(&policy, &cabinet, &other),
                "{class} under the point while an Explorer window is in front"
            );
            assert!(
                !check(&policy, &other, &cabinet),
                "{class} in front of the Explorer window"
            );
        }
    }

    #[test]
    fn a_class_must_match_exactly() {
        let policy = explorer_policy();
        for class in ["cabinetwclass", "CabinetWClass2", "CabinetW", ""] {
            let near = window(class, OPENED);
            assert!(!check(&policy, &near, &near), "{class:?}");
        }
    }

    #[test]
    fn another_window_of_the_listed_class_under_the_point_is_refused() {
        // Two Explorer windows overlap: the one in front is allowed, a click into the
        // other one is not the window the operator allowed.
        let front = window("CabinetWClass", OPENED);
        let behind = window("CabinetWClass", OPENED + 1);
        assert!(!check(&explorer_policy(), &front, &behind));
    }

    #[test]
    fn windows_the_actuator_cannot_attest_are_refused() {
        let policy = explorer_policy();
        let cabinet = window("CabinetWClass", OPENED);
        for (foreground, at) in [(None, Some(&cabinet)), (Some(&cabinet), None), (None, None)] {
            let problem = window_scope_problem(&policy, EXPLORER, foreground, at);
            assert!(
                problem.is_some_and(|p| p.contains("cannot attest")),
                "{problem:?}"
            );
        }
    }

    #[test]
    fn the_pin_allows_only_the_recorded_window() {
        let pinned = pinned_policy(WindowPin::Window(OPENED));
        let recorded = window("CabinetWClass", OPENED);
        assert!(check(&pinned, &recorded, &recorded));
        // A non-recorded Explorer window: the right class, another handle.
        let other = window("CabinetWClass", OPENED + 1);
        assert!(!check(&pinned, &other, &other));
        assert!(!check(&pinned, &recorded, &other));
        assert!(!check(&pinned, &other, &recorded));
    }

    #[test]
    fn a_pin_without_a_window_allows_nothing() {
        let recorded = window("CabinetWClass", OPENED);
        let problem = window_scope_problem(
            &pinned_policy(WindowPin::Unset),
            EXPLORER,
            Some(&recorded),
            Some(&recorded),
        );
        assert!(
            problem.is_some_and(|p| p.contains("none is pinned")),
            "{problem:?}"
        );
    }

    #[test]
    fn a_pin_applies_to_every_application_even_without_classes() {
        let policy = LocalPolicy {
            window_pin: Some(WindowPin::Window(OPENED)),
            ..LocalPolicy::default()
        };
        let recorded = window("Notepad", OPENED);
        let other = window("Notepad", OPENED + 1);
        assert_eq!(
            window_scope_problem(&policy, "notepad.exe", Some(&recorded), Some(&recorded)),
            None
        );
        assert!(window_scope_problem(&policy, "notepad.exe", Some(&other), Some(&other)).is_some());
        assert!(window_scope_problem(&policy, "notepad.exe", None, None).is_some());
    }

    #[test]
    fn the_messages_never_carry_a_class_a_handle_or_a_title() {
        let secret = window("SecretClass", 0xDEAD_BEEF);
        for policy in [explorer_policy(), pinned_policy(WindowPin::Window(OPENED))] {
            let message = window_scope_problem(&policy, EXPLORER, Some(&secret), Some(&secret))
                .unwrap_or_default();
            assert!(!message.is_empty(), "a refusal has a message");
            for leak in [
                "SecretClass",
                "DEADBEEF",
                "deadbeef",
                "3735928559",
                "CabinetWClass",
            ] {
                assert!(!message.contains(leak), "{message}");
            }
        }
    }
}
