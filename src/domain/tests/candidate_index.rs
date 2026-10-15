//! Differential and property tests of the candidate index (ADR-0027, IDX-01 and IDX-14) on
//! generated scenes of the families the protocol names: the R*-tree enumerator and the scan
//! must give the same fusion, and every candidate the scan would accept must be one the index
//! visits. Every scene comes from a fixed seed, so a failure can be reproduced.
//!
//! The default run uses scenes of 32 and 128 controls. Scenes of 512 to 8192 controls are
//! checked by an ignored test in release and by the release benchmark
//! (`src/acceptance/examples/candidate_index.rs`): the scan reference is quadratic and slow
//! without optimisation.

#[path = "support/families.rs"]
mod families;

use families::{FAMILIES, Family, Scene, scene};
use ommatix_domain::bbox::BBox;
use ommatix_domain::candidates::{CandidateSearch, ControlSearch};
use ommatix_domain::fusion::{FusionParams, corridor, fuse, internal_owners_with};

/// Owners of every word, by brute force: the IoC predicate of the reference, applied to all.
fn brute_owner_candidates(boxes: &[BBox], word: &BBox, threshold: f64) -> Vec<usize> {
    (0..boxes.len())
        .filter(|&i| boxes[i].ioc(word) >= threshold)
        .collect()
}

fn assert_same_fusion(name: &str, scene: &Scene, params: FusionParams) {
    let scan = fuse(
        &scene.controls,
        &scene.words,
        FusionParams {
            candidate_search: CandidateSearch::Scan,
            ..params
        },
    )
    .expect("valid parameters");
    let tree = fuse(
        &scene.controls,
        &scene.words,
        FusionParams {
            candidate_search: CandidateSearch::RStar,
            ..params
        },
    )
    .expect("valid parameters");
    assert_eq!(
        tree, scan,
        "{name}: the index changed the fusion (threshold {}, margin {})",
        params.ioc_threshold, params.label_margin
    );
}

fn check_family_fuses_identically(family: Family, sizes: &[usize]) {
    for (k, &n) in sizes.iter().enumerate() {
        let s = scene(family, n, 0x5EED_0026 + k as u64);
        for threshold in [0.0, 0.5, 0.75, 1.0] {
            for margin in [0.0, 0.15] {
                let params = FusionParams {
                    ioc_threshold: threshold,
                    label_margin: margin,
                    ..FusionParams::default()
                };
                assert_same_fusion(&format!("{family:?} n={n}"), &s, params);
            }
        }
        // The owners alone, in the order of the controls, for both enumerators.
        let boxes: Vec<BBox> = s.controls.iter().map(|c| c.bbox).collect();
        let scan = internal_owners_with(&boxes, &s.words, 0.75, CandidateSearch::Scan).unwrap();
        let tree = internal_owners_with(&boxes, &s.words, 0.75, CandidateSearch::RStar).unwrap();
        assert_eq!(tree, scan, "{family:?} n={n}: owners");
    }
}

/// IDX-14 (RF-06): on every family, for small scenes, the integrated fusion is the same for
/// both enumerators: owners, labels, orphans and the order of every list, over the thresholds
/// (zero included) and the label margins of the reference.
#[test]
fn idx_14_generated_families_fuse_identically_up_to_128() {
    for family in FAMILIES {
        check_family_fuses_identically(family, &[32, 128]);
    }
}

/// IDX-14 at the sizes of the protocol, 512 to 8192 controls. Ignored without optimisations:
/// the scan reference is quadratic and takes minutes in a debug build. Run it with
/// `cargo test --release -p ommatix-domain --test candidate_index -- --ignored`; the release
/// benchmark checks the same scenes again.
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "512 to 8192 controls: run with --release -- --ignored"
)]
fn idx_14_generated_families_fuse_identically_from_512_to_8192() {
    for family in FAMILIES {
        check_family_fuses_identically(family, &[512, 2048, 8192]);
    }
}

/// IDX-01 (RF-06): the enumeration is complete. For every word and threshold above zero, every
/// control the reference accepts is visited; the visits are ascending and never repeat; and
/// every obstacle of every corridor is visited too.
#[test]
fn idx_01_generated_candidates_contain_every_owner_and_blocker() {
    for family in FAMILIES {
        for (k, &n) in [32_usize, 128].iter().enumerate() {
            let s = scene(family, n, 0xC0FF_EE00 + k as u64);
            let boxes: Vec<BBox> = s.controls.iter().map(|c| c.bbox).collect();
            let search = ControlSearch::new(&boxes, CandidateSearch::RStar);
            for w in &s.words {
                for threshold in [0.25, 0.75] {
                    let mut visited = Vec::new();
                    search.owner_candidates(&w.bbox, threshold, |i| visited.push(i));
                    assert!(
                        visited.windows(2).all(|p| p[0] < p[1]),
                        "{family:?}: ascending and unique"
                    );
                    for owner in brute_owner_candidates(&boxes, &w.bbox, threshold) {
                        assert!(
                            visited.contains(&owner),
                            "{family:?} n={n}: owner {owner} missed"
                        );
                    }
                }
                for (i, c) in s.controls.iter().enumerate().take(16) {
                    let Some(gap) = corridor(&w.bbox, &c.bbox) else {
                        continue;
                    };
                    let blockers: Vec<usize> = (0..boxes.len())
                        .filter(|&j| {
                            boxes[j] != c.bbox
                                && !boxes[j].contains(&c.bbox)
                                && boxes[j].overlaps(&gap)
                        })
                        .collect();
                    let mut hit = Vec::new();
                    search.any_overlapping(&gap, |j| {
                        hit.push(j);
                        false
                    });
                    for j in blockers {
                        assert!(
                            hit.contains(&j),
                            "{family:?} n={n}: blocker {j} of control {i} missed"
                        );
                    }
                }
            }
        }
    }
}

/// Scan and index agree on each owner query, including the threshold-zero fallback and the
/// tie cases of identical boxes (IDX-02, IDX-04, IDX-07, IDX-13 on generated scenes).
#[test]
fn idx_02_generated_owner_queries_agree_for_every_threshold() {
    for family in FAMILIES {
        let s = scene(family, 128, 0xA11_CE00);
        let boxes: Vec<BBox> = s.controls.iter().map(|c| c.bbox).collect();
        for threshold in [0.0, 0.01, 0.5, 0.75, 1.0] {
            let scan =
                internal_owners_with(&boxes, &s.words, threshold, CandidateSearch::Scan).unwrap();
            let tree =
                internal_owners_with(&boxes, &s.words, threshold, CandidateSearch::RStar).unwrap();
            assert_eq!(tree, scan, "{family:?} threshold {threshold}");
        }
    }
}
