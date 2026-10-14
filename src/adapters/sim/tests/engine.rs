//! The engine end to end over the simulated desktop: perceive, act (dry run), let the
//! user change the screen, perceive again.

use std::sync::Arc;

use ommatix_adapter_sim::{SimulatedDesktop, layout};
use ommatix_application::contract::{
    ActionKind, ActionRequest, ExpectedEffect, ObservePage, ObserveRequest,
};
use ommatix_application::{Engine, EngineConfig, LocalPolicy, ManualClock, PolicyStore, Ports};
use ommatix_domain::fusion::Role;
use ommatix_domain::identity::ActionStatus;
use ommatix_domain::{Action, MouseButton};

struct DenyAll;

impl PolicyStore for DenyAll {
    fn current(&self) -> LocalPolicy {
        LocalPolicy::default()
    }
}

fn engine(desktop: &SimulatedDesktop, clock: Arc<ManualClock>) -> Engine {
    Engine::new(
        EngineConfig::new("sim-session"),
        Ports {
            capturer: Box::new(desktop.capturer()),
            detector: Box::new(desktop.detector()),
            recognizer: Box::new(desktop.recognizer()),
            accessibility: Box::new(ommatix_application::NoAccessibility),
            actuator: Box::new(desktop.actuator()),
            policy: Arc::new(DenyAll),
            clock,
        },
    )
}

fn labels(page: &ObservePage) -> Vec<(String, Role, String, bool)> {
    page.elements
        .iter()
        .map(|e| {
            (
                e.target_id.to_string(),
                e.role,
                e.label.clone(),
                e.enabled.unwrap_or(false),
            )
        })
        .collect()
}

#[test]
fn perceive_act_and_perceive_again() {
    let clock = Arc::new(ManualClock::new());
    let desktop = SimulatedDesktop::new().with_clock(clock.clone());
    let mut engine = engine(&desktop, clock);
    let page = engine.observe_screen(&ObserveRequest::default()).unwrap();
    let s = |v: &str| v.to_owned();
    assert_eq!(
        labels(&page),
        vec![
            (s("e1"), Role::Input, s("Name"), true),
            (s("e2"), Role::Button, s("Cancel"), true),
            (s("e3"), Role::Button, s("Save"), true),
        ]
    );

    let result = engine.execute_action(&ActionRequest {
        request_id: "r1".into(),
        session_id: page.session_id.clone(),
        snapshot_id: page.snapshot_id.clone(),
        geometry_epoch: page.geometry_epoch,
        target_id: "e1".into(),
        expected_revision: 1,
        action: ActionKind::Type {
            text: "hello".into(),
        },
        dry_run: true,
        expected_effect: ExpectedEffect::None,
    });
    assert_eq!(result.status, ActionStatus::Simulated);
    assert!(
        desktop.handed_actions().is_empty(),
        "a dry run hands nothing"
    );

    // The user opens the dialog: the modal disables the form and adds OK.
    desktop.perform(&Action::Click {
        at: layout::SAVE_BUTTON.center(),
        button: MouseButton::Left,
    });
    let page = engine.observe_screen(&ObserveRequest::default()).unwrap();
    assert_eq!(page.snapshot_id, "s2");
    assert_eq!(
        labels(&page),
        vec![
            (s("e1"), Role::Input, s("Name"), false),
            (s("e2"), Role::Button, s("Cancel"), false),
            (s("e3"), Role::Button, s("Save"), false),
            (s("e4"), Role::TextNode, s("Confirm"), false),
            (s("e5"), Role::Button, s("OK"), true),
        ]
    );
    assert_eq!(page.elements[0].revision, 2, "disabled is a new revision");
}
