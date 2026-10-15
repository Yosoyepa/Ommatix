//! The candidate index inside the engine (ADR-0027, IDX-09, IDX-10, IDX-14). An engine that
//! runs with `candidate_search = rstar` publishes exactly what the scan publishes, across a new
//! geometry revision and a failed observation, and it keeps no index from one observation to
//! the next: each observation builds its own from the reconciled controls of that observation.

use ommatix_domain::candidates::CandidateSearch;

use super::*;

/// The form of the engine tests, with a panel behind it, so that owners and corridors compete.
fn scene() -> World {
    let mut w = form();
    // First, so that the form is painted over it (`render` paints in order).
    w.controls.insert(
        0,
        Control {
            bbox: bx(20.0, 20.0, 620.0, 300.0),
            role: Role::Unknown,
            affordance: Affordance {
                actionability: Actionability::Unknown,
                enabled: true,
                visible: true,
            },
        },
    );
    w
}

fn rig_for(search: CandidateSearch, world: World) -> Rig {
    let mut config = EngineConfig::new(SESSION);
    config.fusion.candidate_search = search;
    rig_with(world, config)
}

/// The evidence, relations and gaps of the snapshot the engine committed last.
fn evidence_of(r: &Rig) -> String {
    r.engine
        .store
        .published()
        .map(|p| {
            format!(
                "{:?}|{:?}|{:?}",
                p.evidence, p.relations, p.accessibility_gaps
            )
        })
        .unwrap_or_default()
}

/// One observation and what it published that does not depend on its run: the page without
/// its timings, and the evidence of the snapshot it committed.
fn observe_published(r: &mut Rig) -> (ObservePage, String) {
    let mut page = r.observe();
    page.timings = StageTimings::default();
    (page, evidence_of(r))
}

/// IDX-14 (ADR-0027): the same scripted session, through the whole observation (detection,
/// text, fusion, identity, commit), publishes the same for both enumerators.
#[test]
fn idx_14_an_engine_session_publishes_the_same_for_both_enumerators() {
    let session = |search| {
        let mut r = rig_for(search, scene());
        let mut out = vec![observe_published(&mut r)];
        {
            let mut w = r.world();
            w.controls[3].bbox = bx(502.0, 250.0, 582.0, 280.0);
            w.words[3].bbox = bx(517.0, 258.0, 567.0, 272.0);
        }
        out.push(observe_published(&mut r));
        out.push(observe_published(&mut r));
        out
    };
    let scan = session(CandidateSearch::Scan);
    let tree = session(CandidateSearch::RStar);
    assert_eq!(tree, scan);
    assert!(
        scan[0].0.elements.len() >= 4,
        "the form publishes its controls"
    );
}

/// IDX-09 (ADR-0027, RF-10): a new geometry revision with as many controls as before. The
/// "hello" text is now covered by a smaller button, so its owner changes: the second
/// observation must follow the new boxes, not an index built for the earlier ones.
#[test]
fn idx_09_a_new_revision_with_as_many_boxes_rebuilds_the_candidates() {
    let session = |search| {
        let mut r = rig_for(search, scene());
        let first = observe_published(&mut r);
        {
            let mut w = r.world();
            w.epoch = 2;
            w.controls[3].bbox = bx(105.0, 45.0, 165.0, 65.0);
        }
        let second = observe_published(&mut r);
        (first, second)
    };
    let (scan_first, scan_second) = session(CandidateSearch::Scan);
    let (tree_first, tree_second) = session(CandidateSearch::RStar);
    assert_eq!(tree_first, scan_first);
    assert_eq!(tree_second, scan_second);
    assert_ne!(
        scan_first.0.geometry_epoch, scan_second.0.geometry_epoch,
        "the second observation is a new revision"
    );
    assert_ne!(
        scan_first.0.elements, scan_second.0.elements,
        "the moved boxes change what the page publishes"
    );
}

/// IDX-10 (ADR-0027, RF-04): an observation that fails before its commit publishes nothing:
/// the committed revision and evidence stay as they were, and the next observation publishes
/// what the scan publishes.
#[test]
fn idx_10_a_failed_observation_publishes_nothing_and_the_next_one_matches_the_scan() {
    let session = |search| {
        let mut r = rig_for(search, scene());
        let first = observe_published(&mut r);
        let (revision, evidence) = (r.engine.store.revision(), evidence_of(&r));
        {
            // Pixels must change, or the observation has no region for the detector to fail on.
            let mut w = r.world();
            w.controls[3].bbox = bx(502.0, 250.0, 582.0, 280.0);
            w.fail_detect = true;
        }
        let failed = r.observe_with(&ObserveRequest::default());
        assert!(
            failed.is_err(),
            "the detector failure fails the observation"
        );
        assert_eq!(r.engine.store.revision(), revision, "nothing was committed");
        assert_eq!(
            evidence_of(&r),
            evidence,
            "the committed evidence is unchanged"
        );
        r.world().fail_detect = false;
        let recovered = observe_published(&mut r);
        (first, recovered)
    };
    let scan = session(CandidateSearch::Scan);
    let tree = session(CandidateSearch::RStar);
    assert_eq!(tree, scan);
}
