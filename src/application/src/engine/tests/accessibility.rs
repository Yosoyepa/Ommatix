//! Accessibility evidence in the engine (ADR-0017), over the [`Access`] double.

use super::*;

const INVOKE: AccessiblePatterns = AccessiblePatterns {
    invoke: true,
    toggle: false,
    expand_collapse: false,
    select: false,
    editable_value: false,
    scroll: false,
};

const EDITABLE: AccessiblePatterns = AccessiblePatterns {
    invoke: false,
    toggle: false,
    expand_collapse: false,
    select: false,
    editable_value: true,
    scroll: false,
};

fn element(bbox: BBox, role: Role, name: &str, patterns: AccessiblePatterns) -> AccessibleElement {
    AccessibleElement {
        bbox,
        role,
        name: name.into(),
        enabled: true,
        offscreen: false,
        password: false,
        patterns,
    }
}

fn unknown(bbox: BBox) -> Control {
    Control {
        bbox,
        role: Role::Unknown,
        affordance: Affordance {
            actionability: Actionability::Unknown,
            enabled: true,
            visible: true,
        },
    }
}

/// A single-class detector (roles unknown, nothing confirmed) over a form whose window
/// exposes its controls to the accessibility tree: an input named "Name", a password
/// field, a Save button named "Guardar" and a Help link the detector missed. A detector
/// box at the top right has no accessible counterpart.
fn accessible_form() -> World {
    let input = bx(100.0, 40.0, 300.0, 70.0);
    let password = bx(100.0, 90.0, 300.0, 120.0);
    let save = bx(400.0, 250.0, 480.0, 280.0);
    let help = bx(20.0, 280.0, 60.0, 300.0);
    World {
        controls: vec![
            unknown(bx(102.0, 42.0, 298.0, 68.0)),
            unknown(password),
            unknown(bx(404.0, 252.0, 476.0, 278.0)),
            unknown(bx(560.0, 20.0, 620.0, 50.0)),
        ],
        words: vec![
            Word::new("hello", bx(110.0, 48.0, 160.0, 62.0)),
            Word::new("secret", bx(110.0, 98.0, 170.0, 112.0)),
            Word::new("Save", bx(420.0, 258.0, 460.0, 272.0)),
            Word::new("Menu", bx(565.0, 28.0, 610.0, 42.0)),
        ],
        accessible: vec![
            element(input, Role::Input, "Name", EDITABLE),
            AccessibleElement {
                password: true,
                ..element(password, Role::Unknown, "Password", EDITABLE)
            },
            element(save, Role::Button, "Guardar", INVOKE),
            element(help, Role::Unknown, "Help", INVOKE),
        ],
        ..World::default()
    }
}

type Summary = (String, Role, LabelSource, Actionability);

fn summary(page: &ObservePage) -> Vec<Summary> {
    let mut seen: Vec<Summary> = page
        .elements
        .iter()
        .map(|e| (e.label.clone(), e.role, e.label_source, e.actionability))
        .collect();
    seen.sort_by(|a, b| a.0.cmp(&b.0));
    seen
}

#[test]
fn ut_44_accessibility_role_and_name_win_over_the_visual_evidence() {
    let mut r = rig(accessible_form());
    let page = r.observe();
    let row = |label: &str, role, source, actionability| -> Summary {
        (label.to_owned(), role, source, actionability)
    };
    // Detector boxes are absorbed, the OCR captions inside them are no text nodes, and the
    // unmatched box keeps its OCR label and unknown affordance.
    assert_eq!(
        summary(&page),
        vec![
            row(
                "Guardar",
                Role::Button,
                LabelSource::Accessibility,
                Actionability::Confirmed
            ),
            row(
                "Help",
                Role::Unknown,
                LabelSource::Accessibility,
                Actionability::Confirmed
            ),
            row(
                "Menu",
                Role::Unknown,
                LabelSource::Ocr,
                Actionability::Unknown
            ),
            row(
                "Name",
                Role::Input,
                LabelSource::Accessibility,
                Actionability::Confirmed
            ),
            row(
                "Password",
                Role::Unknown,
                LabelSource::Accessibility,
                Actionability::Unknown
            ),
        ]
    );
    // Delivered at the accessible rectangle's centre, not the detector box's.
    r.world().live = true;
    r.set_policy(live_policy());
    let mut click = Rig::action(&page, "Guardar", "r1", ActionKind::Click);
    click.dry_run = false;
    assert_eq!(
        r.engine.execute_action(&click).status,
        ActionStatus::InputDelivered
    );
    assert_eq!(r.world().delivered[0].point(), (440.0, 265.0));
}

#[test]
fn ut_44_type_is_allowed_on_an_accessibility_confirmed_input() {
    let mut r = rig(accessible_form());
    let page = r.observe();
    let text = || ActionKind::Type { text: "x".into() };
    let on_input = Rig::action(&page, "Name", "r1", text());
    assert_eq!(
        r.engine.execute_action(&on_input).status,
        ActionStatus::Simulated
    );
    for (label, id) in [("Guardar", "r2"), ("Menu", "r3"), ("Password", "r4")] {
        let result = r
            .engine
            .execute_action(&Rig::action(&page, label, id, text()));
        assert_eq!(
            error_code(&result),
            Some(ErrorCode::NotActionable),
            "{label}"
        );
    }
}

#[test]
fn a_password_field_never_publishes_its_text() {
    let mut r = rig(accessible_form());
    r.set_policy(LocalPolicy {
        include_values: true,
        ..LocalPolicy::default()
    });
    let asked = ObserveRequest {
        include_values: true,
        ..ObserveRequest::default()
    };
    let page = r.observe_with(&asked).unwrap();
    let password = page
        .elements
        .iter()
        .find(|e| e.label == "Password")
        .unwrap();
    assert_eq!(password.text, None);
    let input = page.elements.iter().find(|e| e.label == "Name").unwrap();
    assert_eq!(input.text, Some(("hello".into(), TextKind::Unknown)));
    // The password's OCR text is neither a label, a value nor a text node.
    let leaks = |page: &ObservePage, secret: &str| {
        page.elements.iter().any(|e| {
            e.label.contains(secret) || e.text.as_ref().is_some_and(|(t, _)| t.contains(secret))
        })
    };
    assert!(!leaks(&page, "secret"));

    // Without an accessible name, a password field still takes no OCR label.
    {
        let mut w = r.world();
        w.accessible[1].name = String::new();
        w.words[1] = Word::new("hunter2", bx(110.0, 98.0, 180.0, 112.0));
    }
    let page = r.observe_with(&asked).unwrap();
    assert!(!leaks(&page, "hunter2"));
}

#[test]
fn only_planned_regions_reach_the_accessibility_provider() {
    let mut r = rig(accessible_form());
    r.observe();
    let full = bx(0.0, 0.0, f64::from(W), f64::from(H));
    assert_eq!(r.world().access_calls, vec![vec![full]]);
    // Nothing changed: the tree is not read again.
    r.observe();
    assert_eq!(r.world().access_calls.len(), 1);
    // A change near the Save button: only that region is read, and the Help link's
    // evidence, outside it, is kept.
    r.world().words[2].bbox = bx(422.0, 258.0, 462.0, 272.0);
    let page = r.observe();
    let calls = r.world().access_calls.clone();
    assert_eq!(calls.len(), 2);
    assert!(calls[1].iter().all(|reg| *reg != full));
    assert!(
        !calls[1]
            .iter()
            .any(|reg| reg.overlaps(&bx(20.0, 280.0, 60.0, 300.0)))
    );
    assert_eq!(Rig::id_of(&page, "Help").1, 1);
    assert!(page.elements.iter().any(|e| e.label == "Guardar"));
}

#[test]
fn the_accessibility_stage_is_timed() {
    struct Slow(Arc<ManualClock>);
    impl AccessibilityProvider for Slow {
        fn name(&self) -> &'static str {
            "slow"
        }
        fn snapshot(&mut self, _: &[BBox]) -> Result<Vec<AccessibleElement>, PortError> {
            self.0.advance(Duration::from_millis(12));
            Ok(Vec::new())
        }
    }
    let mut r = rig(form());
    r.engine.ports.accessibility = Box::new(Slow(r.clock.clone()));
    r.observe();
    let t = r.engine.last_timings().unwrap();
    assert_eq!(t.accessibility, Duration::from_millis(12));
    assert_eq!(t.total, Duration::from_millis(12));
}

#[test]
fn an_accessibility_failure_publishes_nothing() {
    let mut r = rig(accessible_form());
    r.world().fail_access = true;
    let err = r.observe_with(&ObserveRequest::default()).unwrap_err();
    assert_eq!(err.code, ErrorCode::InferenceFailed);
    assert_eq!(r.engine.engine_metrics().observations, 0);
}

/// S4 finding 2: a detector box centred on another control (the Calculator's "Función"
/// box over x²) is no longer confirmed once the accessibility tree shows the control under
/// its centre; the accessible control is.
#[test]
fn a_visual_box_centred_on_an_accessible_control_is_not_confirmed() {
    let world = World {
        controls: vec![Control {
            bbox: bx(150.0, 80.0, 290.0, 150.0),
            role: Role::Unknown,
            affordance: confirmed(),
        }],
        words: vec![Word::new("Función", bx(160.0, 85.0, 280.0, 98.0))],
        accessible: vec![element(
            bx(200.0, 100.0, 240.0, 130.0),
            Role::Button,
            "Square",
            INVOKE,
        )],
        ..World::default()
    };
    let mut r = rig(world);
    let page = r.observe();
    let label = |l: &str| page.elements.iter().find(|e| e.label == l).unwrap().clone();
    assert_eq!(label("Función").actionability, Actionability::Unknown);
    assert_eq!(label("Square").actionability, Actionability::Confirmed);
    let click = Rig::action(&page, "Función", "r1", ActionKind::Click);
    assert_eq!(
        error_code(&r.engine.execute_action(&click)),
        Some(ErrorCode::NotActionable)
    );
}

#[test]
fn accessibility_evidence_is_deterministic_in_any_provider_order() {
    let mut a = rig(accessible_form());
    let mut reversed = accessible_form();
    reversed.accessible.reverse();
    let mut b = rig(reversed);
    assert_eq!(a.observe(), b.observe());
}
