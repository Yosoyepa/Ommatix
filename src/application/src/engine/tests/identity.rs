//! Native identity through the whole observation (ADR-0026): the runtime ID of an
//! accessibility element is the discriminant of its track, so the same text in the same box
//! is not the same object. Each test runs fusion and tracking over the accessibility double,
//! and the pixels of a changed region are repainted so that the planned observation reads it
//! again, as it would on a real screen.

use ommatix_domain::identity::{Changes, Retirement, TargetId, Track};

use super::*;
use crate::ports::NativeIdentity;

/// The process and window of the scene's accessibility tree.
const PROCESS: u32 = 4321;
const WINDOW: u64 = 500;

/// A button named `name` at `bbox`, exposed by process `scope.0` in window `scope.1`, with
/// the runtime ID `[42, id]`.
fn native_button_in(bbox: BBox, name: &str, id: i32, scope: (u32, u64)) -> AccessibleElement {
    AccessibleElement {
        bbox,
        role: Role::Button,
        name: name.into(),
        enabled: true,
        offscreen: false,
        password: false,
        patterns: AccessiblePatterns {
            invoke: true,
            ..AccessiblePatterns::default()
        },
        native: Some(NativeIdentity {
            provider: "uia",
            process_id: scope.0,
            window: scope.1,
            runtime_id: vec![42, id],
        }),
    }
}

fn native_button(bbox: BBox, name: &str, id: i32) -> AccessibleElement {
    native_button_in(bbox, name, id, (PROCESS, WINDOW))
}

/// A window whose accessibility tree is `elements` and whose pixels show nothing else.
fn window(elements: Vec<AccessibleElement>) -> Rig {
    rig(World {
        accessible: elements,
        ..World::default()
    })
}

/// Repaints the region of `bbox` with `shade`: a different picture, so the planned
/// observation reads that region again. The planner compares colour channels only (UT-05),
/// so a picture is a change only when its shade differs from what the region showed: a
/// region with nothing painted is not repainted black.
fn picture(r: &Rig, bbox: BBox, shade: u8) {
    let mut w = r.world();
    w.textures.retain(|(b, _)| *b != bbox);
    w.textures.push((bbox, vec![shade; 72]));
}

/// Removes the picture of `bbox`: the region changes and nothing is painted there.
fn erase(r: &Rig, bbox: BBox) {
    r.world().textures.retain(|(b, _)| *b != bbox);
}

/// The ID of the element of the published snapshot that stands at `bbox`.
fn id_at(r: &Rig, bbox: BBox) -> TargetId {
    published_tracks(r)
        .find(|t| t.observation.bbox == bbox)
        .unwrap_or_else(|| panic!("no element at {bbox:?}"))
        .target_id
}

/// The ID of the element whose discriminant ends with `suffix` (its runtime ID).
fn id_of_runtime_id(r: &Rig, suffix: &str) -> TargetId {
    published_tracks(r)
        .find(|t| {
            t.observation
                .discriminant
                .as_deref()
                .is_some_and(|d| d.ends_with(suffix))
        })
        .unwrap_or_else(|| panic!("no element with runtime id {suffix}"))
        .target_id
}

fn published_tracks(r: &Rig) -> impl Iterator<Item = &Track> {
    r.engine
        .store
        .published()
        .expect("an observation was published")
        .registry
        .tracks()
}

fn is_live(r: &Rig, id: TargetId) -> bool {
    published_tracks(r).any(|t| t.target_id == id)
}

fn identity_changes(r: &Rig) -> Changes {
    r.engine
        .last_identity_changes()
        .expect("an observation was published")
        .clone()
}

/// ADR-0026, BDD-15 (UT-37) through the real pipeline: a substitution with the same text and
/// box is another object, so the ID does not pass to it, and the old one records why.
#[test]
fn ut_37_a_substituted_element_with_the_same_text_and_box_gets_a_new_id() {
    let save = bx(400.0, 250.0, 480.0, 280.0);
    let mut r = window(vec![native_button(save, "Guardar", 7)]);
    r.observe();
    let old = id_at(&r, save);

    // Another object with the same text and box now stands there; its picture changed.
    r.world().accessible = vec![native_button(save, "Guardar", 8)];
    picture(&r, save, 255);
    r.observe();

    let new = id_at(&r, save);
    assert_ne!(new, old, "the substitute must not inherit the ID");
    assert!(!is_live(&r, old));
    let changes = identity_changes(&r);
    assert_eq!(changes.retired, vec![old]);
    assert_eq!(changes.reasons, vec![(old, Retirement::Substituted)]);
}

/// ADR-0026: the same object that moves a pixel keeps its ID; only its revision moves.
#[test]
fn the_same_object_moved_by_a_pixel_keeps_its_id() {
    let at = bx(400.0, 250.0, 480.0, 280.0);
    let moved = bx(401.0, 250.0, 481.0, 280.0);
    let mut r = window(vec![native_button(at, "Guardar", 7)]);
    picture(&r, at, 128);
    r.observe();
    let id = id_at(&r, at);

    r.world().accessible = vec![native_button(moved, "Guardar", 7)];
    erase(&r, at);
    picture(&r, moved, 128);
    r.observe();

    assert_eq!(id_at(&r, moved), id);
    let changes = identity_changes(&r);
    assert_eq!(changes.created, vec![]);
    assert_eq!(changes.retired, vec![]);
    assert_eq!(changes.updated, vec![id]);
}

/// BDD-15 through the whole pipeline: a list of identical rows, one of whose places is now
/// taken by another row object. The rows that stayed keep their IDs and the place that
/// changed hands its old ID to no one.
#[test]
fn a_virtualised_row_that_now_shows_another_object_keeps_no_id() {
    let row = |i: u32| {
        bx(
            0.0,
            100.0 + 20.0 * f64::from(i),
            300.0,
            118.0 + 20.0 * f64::from(i),
        )
    };
    let mut r = window(vec![
        native_button(row(0), "Abrir", 1),
        native_button(row(1), "Abrir", 2),
        native_button(row(2), "Abrir", 3),
    ]);
    r.observe();
    let (first, second, third) = (id_at(&r, row(0)), id_at(&r, row(1)), id_at(&r, row(2)));

    r.world().accessible[0] = native_button(row(0), "Abrir", 9);
    picture(&r, row(0), 255);
    r.observe();

    assert_ne!(id_at(&r, row(0)), first);
    assert_eq!(id_at(&r, row(1)), second);
    assert_eq!(id_at(&r, row(2)), third);
    assert_eq!(
        identity_changes(&r).reasons,
        vec![(first, Retirement::Substituted)]
    );
}

/// ADR-0026: two identical buttons that swap places keep neither ID. Each now stands where
/// the other was, and neither takes the other's ID, though that one is the nearest.
#[test]
fn twin_controls_that_swap_places_never_exchange_their_ids() {
    let left = bx(100.0, 40.0, 180.0, 70.0);
    let right = bx(300.0, 40.0, 380.0, 70.0);
    let mut r = window(vec![
        native_button(left, "OK", 1),
        native_button(right, "OK", 2),
    ]);
    picture(&r, left, 128);
    picture(&r, right, 128);
    r.observe();
    let (old_left, old_right) = (id_at(&r, left), id_at(&r, right));

    r.world().accessible = vec![native_button(right, "OK", 1), native_button(left, "OK", 2)];
    picture(&r, left, 255);
    picture(&r, right, 64);
    r.observe();

    let (new_left, new_right) = (id_at(&r, left), id_at(&r, right));
    assert!(
        ![old_left, old_right].contains(&new_left),
        "left: {new_left}"
    );
    assert!(
        ![old_left, old_right].contains(&new_right),
        "right: {new_right}"
    );
    assert_ne!(new_left, new_right);
    assert!(!is_live(&r, old_left) && !is_live(&r, old_right));
}

/// ADR-0026: twins stacked on one box keep their own IDs, because their identities tell
/// them apart where their boxes cannot.
#[test]
fn stacked_twins_keep_their_own_ids() {
    let same = bx(100.0, 40.0, 180.0, 70.0);
    let mut r = window(vec![
        native_button(same, "OK", 1),
        native_button(same, "OK", 2),
    ]);
    r.observe();
    let (one, two) = (id_of_runtime_id(&r, "42.1"), id_of_runtime_id(&r, "42.2"));
    assert_ne!(one, two);

    picture(&r, same, 128);
    r.observe();

    assert_eq!(id_of_runtime_id(&r, "42.1"), one);
    assert_eq!(id_of_runtime_id(&r, "42.2"), two);
    assert_eq!(identity_changes(&r), Changes::default());
}

/// ADR-0026: a navigation rebuilds its window. The same button, text, box and runtime ID in
/// another window (or another process) is another object.
#[test]
fn a_button_in_another_window_or_process_gets_a_new_id() {
    let next = bx(400.0, 250.0, 480.0, 280.0);
    let mut r = window(vec![native_button(next, "Siguiente", 1)]);
    r.observe();
    let old = id_at(&r, next);

    r.world().accessible = vec![native_button_in(
        next,
        "Siguiente",
        1,
        (PROCESS, WINDOW + 1),
    )];
    picture(&r, next, 255);
    r.observe();
    let in_other_window = id_at(&r, next);
    assert_ne!(in_other_window, old, "another window");

    r.world().accessible = vec![native_button_in(
        next,
        "Siguiente",
        1,
        (PROCESS + 1, WINDOW + 1),
    )];
    picture(&r, next, 0);
    r.observe();
    assert_ne!(id_at(&r, next), in_other_window, "another process");
}

/// ADR-0026: a container recycled to show another item keeps its runtime ID, but not its
/// content: the ID does not pass to the new item.
#[test]
fn a_recycled_container_with_other_content_gets_a_new_id() {
    let at = bx(100.0, 40.0, 300.0, 70.0);
    let mut r = window(vec![native_button(at, "Abrir", 5)]);
    r.observe();
    let old = id_at(&r, at);

    r.world().accessible = vec![native_button(at, "Eliminar", 5)];
    picture(&r, at, 255);
    r.observe();

    assert_ne!(id_at(&r, at), old);
    assert_eq!(
        identity_changes(&r).reasons,
        vec![(old, Retirement::Vanished)]
    );
}

/// ADR-0026: an element that leaves the window and comes back is not the element that left,
/// even with the same runtime ID: continuity holds only between consecutive observations.
#[test]
fn an_element_that_comes_back_after_a_lapse_gets_a_new_id() {
    let at = bx(400.0, 250.0, 480.0, 280.0);
    let mut r = window(vec![native_button(at, "Guardar", 3)]);
    picture(&r, at, 128);
    r.observe();
    let old = id_at(&r, at);

    r.world().accessible = Vec::new();
    erase(&r, at);
    r.observe();
    assert!(!is_live(&r, old));

    r.world().accessible = vec![native_button(at, "Guardar", 3)];
    picture(&r, at, 128);
    r.observe();
    assert_ne!(id_at(&r, at), old);
}

/// ADR-0026: an accessibility element carries its scoped runtime ID as its discriminant, and
/// a visual box carries none. Without that discriminant, the substitution above would keep
/// its ID: this test fails if the pipeline drops it again.
#[test]
fn a_native_identity_is_the_discriminant_of_its_observation() {
    let native_at = bx(400.0, 250.0, 480.0, 280.0);
    let visual_at = bx(100.0, 40.0, 300.0, 70.0);
    let mut r = rig(World {
        controls: vec![Control {
            bbox: visual_at,
            role: Role::Button,
            affordance: confirmed(),
        }],
        accessible: vec![native_button(native_at, "Guardar", 7)],
        ..World::default()
    });
    r.observe();

    let discriminant_at = |bbox: BBox| {
        published_tracks(&r)
            .find(|t| t.observation.bbox == bbox)
            .unwrap_or_else(|| panic!("no element at {bbox:?}"))
            .observation
            .discriminant
            .clone()
    };
    assert_eq!(
        discriminant_at(native_at).as_deref(),
        Some("uia:4321:500:42.7")
    );
    assert_eq!(discriminant_at(visual_at), None);
}
