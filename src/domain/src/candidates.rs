//! Candidate enumeration for Algorithm 2 (ADR-0027, INC-05 of the next-cut plan).
//!
//! Algorithm 2 asks the controls of one fusion two questions: which of them can own a word
//! (their IoC over the word reaches the threshold), and which of them block the corridor
//! between a word and a control. This module answers with *candidates*: positions into the
//! controls' slice, visited in ascending order and at most once each. The fusion then applies
//! the exact predicate of the reference to every candidate, so an enumeration may be larger
//! than the answer, but it must never omit an element of it.
//!
//! [`CandidateSearch::Scan`] is the reference: every position is a candidate, in source order,
//! and no structure is built. [`CandidateSearch::RStar`] is the experimental index: an R*-tree
//! built once per fusion over the same boxes. Its completeness is argued only where the query
//! window is a box that Algorithm 2 compares: an owner shares positive area with the word, and
//! an obstacle shares positive area with the corridor, so both meet the closed window of that
//! box. At an IoC threshold of zero every control qualifies, so the index visits every position
//! instead (ADR-0027).

use rstar::{AABB, RTree, RTreeObject};

use crate::bbox::BBox;

/// How Algorithm 2 enumerates the candidate controls of a fusion (ADR-0027).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CandidateSearch {
    /// Every control, in source order. The reference (ADR-0003) and the default.
    #[default]
    Scan,
    /// An R*-tree built once per fusion over the same controls. Experimental and off by
    /// default: its results are the reference's, its cost is not shown to pay (ADR-0027).
    RStar,
}

/// A control's entry in the tree: its position in the fusion's slice and its envelope.
#[derive(Debug, Clone, Copy)]
struct Entry {
    position: usize,
    envelope: AABB<[f64; 2]>,
}

impl RTreeObject for Entry {
    type Envelope = AABB<[f64; 2]>;

    fn envelope(&self) -> Self::Envelope {
        self.envelope
    }
}

/// The box as the tree stores it: the same corners, with no rounding.
fn envelope(b: &BBox) -> AABB<[f64; 2]> {
    AABB::from_corners([b.x0(), b.y0()], [b.x1(), b.y1()])
}

#[derive(Debug)]
enum Index {
    Scan,
    Tree(RTree<Entry>),
}

/// The controls of one fusion, as their enumeration sees them. Built once over the reconciled
/// controls and borrowed for the whole fusion, so every position refers to the slice it was
/// built from.
#[derive(Debug)]
pub struct ControlSearch<'a> {
    boxes: &'a [BBox],
    index: Index,
}

impl<'a> ControlSearch<'a> {
    /// Prepares the enumeration of `boxes`. The R*-tree is bulk-loaded here, once; `Scan`
    /// builds nothing.
    pub fn new(boxes: &'a [BBox], search: CandidateSearch) -> Self {
        let index = match search {
            CandidateSearch::Scan => Index::Scan,
            CandidateSearch::RStar => Index::Tree(RTree::bulk_load(
                boxes
                    .iter()
                    .enumerate()
                    .map(|(position, b)| Entry {
                        position,
                        envelope: envelope(b),
                    })
                    .collect(),
            )),
        };
        Self { boxes, index }
    }

    /// The boxes the positions refer to.
    pub const fn boxes(&self) -> &'a [BBox] {
        self.boxes
    }

    /// Number of controls, and so of positions.
    pub const fn len(&self) -> usize {
        self.boxes.len()
    }

    pub const fn is_empty(&self) -> bool {
        self.boxes.is_empty()
    }

    /// The enumerator in use.
    pub const fn strategy(&self) -> CandidateSearch {
        match self.index {
            Index::Scan => CandidateSearch::Scan,
            Index::Tree(_) => CandidateSearch::RStar,
        }
    }

    /// Calls `visit` once for every position that may own a word with box `word` at
    /// `threshold`, in ascending order. With a threshold above zero an owner shares positive
    /// area with the word, so the word's own box is a complete window. At zero, and always for
    /// [`CandidateSearch::Scan`], every position is visited: completeness is not proven there.
    pub fn owner_candidates(&self, word: &BBox, threshold: f64, visit: impl FnMut(usize)) {
        match &self.index {
            Index::Tree(tree) if threshold > 0.0 => {
                let mut positions: Vec<usize> = tree
                    .locate_in_envelope_intersecting(envelope(word))
                    .map(|entry| entry.position)
                    .collect();
                positions.sort_unstable();
                positions.into_iter().for_each(visit);
            }
            _ => (0..self.boxes.len()).for_each(visit),
        }
    }

    /// Whether `pred` holds for some position whose box may overlap `gap`, the corridor
    /// between a word and a control. Every box that overlaps `gap` with positive area meets
    /// its closed window, so the tree never hides an obstacle that the predicate would accept.
    /// The calls happen in no defined order: the answer is a single boolean.
    pub fn any_overlapping(&self, gap: &BBox, mut pred: impl FnMut(usize) -> bool) -> bool {
        match &self.index {
            Index::Tree(tree) => tree
                .locate_in_envelope_intersecting(envelope(gap))
                .any(|entry| pred(entry.position)),
            Index::Scan => (0..self.boxes.len()).any(pred),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bx(x0: f64, y0: f64, x1: f64, y1: f64) -> BBox {
        BBox::new(x0, y0, x1, y1).unwrap()
    }

    fn visited(search: &ControlSearch<'_>, word: &BBox, threshold: f64) -> Vec<usize> {
        let mut out = Vec::new();
        search.owner_candidates(word, threshold, |i| out.push(i));
        out
    }

    /// IDX-01 (Algorithm 2, RF-06): a control that only touches the word's edge or corner is
    /// a candidate, because the window is closed; the exact filter then keeps only positive
    /// area, so the candidate is rejected there and nowhere else.
    #[test]
    fn idx_01_edge_and_corner_contact_are_candidates_and_the_exact_filter_drops_them() {
        let word = bx(10.0, 10.0, 20.0, 20.0);
        let boxes = [
            bx(20.0, 10.0, 40.0, 20.0),
            bx(20.0, 20.0, 40.0, 40.0),
            bx(0.0, 0.0, 10.0, 10.0),
            bx(15.0, 15.0, 25.0, 25.0),
        ];
        for strategy in [CandidateSearch::Scan, CandidateSearch::RStar] {
            let search = ControlSearch::new(&boxes, strategy);
            let candidates = visited(&search, &word, 0.5);
            assert!(candidates.contains(&0), "{strategy:?}: edge contact");
            assert!(candidates.contains(&1), "{strategy:?}: corner contact");
            assert!(candidates.contains(&2), "{strategy:?}: corner contact");
            assert!(candidates.contains(&3), "{strategy:?}: overlap");
            let exact: Vec<usize> = candidates
                .iter()
                .copied()
                .filter(|&i| boxes[i].overlaps(&word))
                .collect();
            assert_eq!(exact, vec![3], "{strategy:?}: positive area only");
        }
    }

    /// IDX-02 (ADR-0027): at an IoC threshold of zero the index cannot prove completeness, so
    /// it visits every position, including the controls that do not touch the word.
    #[test]
    fn idx_02_zero_threshold_visits_every_position() {
        let boxes = [bx(0.0, 0.0, 5.0, 5.0), bx(100.0, 100.0, 110.0, 110.0)];
        let word = bx(10.0, 10.0, 20.0, 20.0);
        let search = ControlSearch::new(&boxes, CandidateSearch::RStar);
        assert_eq!(visited(&search, &word, 0.0), vec![0, 1]);
        assert_eq!(visited(&search, &word, 0.75), Vec::<usize>::new());
        let scan = ControlSearch::new(&boxes, CandidateSearch::Scan);
        assert_eq!(visited(&scan, &word, 0.75), vec![0, 1]);
    }

    /// IDX-07 (RF-06): identical boxes are distinct controls. Each position is visited once,
    /// in ascending order, whatever the tree's own order is.
    #[test]
    fn idx_07_identical_boxes_keep_each_position_once_in_ascending_order() {
        let same = bx(0.0, 0.0, 100.0, 100.0);
        let boxes = [same; 40];
        let word = bx(10.0, 10.0, 20.0, 20.0);
        let search = ControlSearch::new(&boxes, CandidateSearch::RStar);
        let positions = visited(&search, &word, 0.75);
        assert_eq!(positions, (0..40).collect::<Vec<_>>());
    }

    /// IDX-08 (Algorithm 2, RF-06): a word taller than its control is still found, because the
    /// query is the word's own box, not a window derived from the control's height. Here the
    /// control covers 87.5 % of the word, so it owns the word at 0.75 and must be a candidate.
    #[test]
    fn idx_08_a_word_taller_than_its_control_is_found_through_its_own_box() {
        let control = bx(100.0, 0.0, 200.0, 350.0);
        let tall_word = bx(120.0, 0.0, 180.0, 400.0);
        assert!(control.ioc(&tall_word) >= 0.75);
        let boxes = [control];
        for strategy in [CandidateSearch::Scan, CandidateSearch::RStar] {
            let search = ControlSearch::new(&boxes, strategy);
            assert_eq!(visited(&search, &tall_word, 0.75), vec![0], "{strategy:?}");
        }
    }

    /// IDX-13 (RF-06): when every control overlaps the query, the index keeps every
    /// candidate; the order and the abstention are then the reference's.
    #[test]
    fn idx_13_total_overlap_keeps_every_candidate() {
        let boxes = vec![bx(0.0, 0.0, 100.0, 100.0); 500];
        let word = bx(1.0, 1.0, 2.0, 2.0);
        let search = ControlSearch::new(&boxes, CandidateSearch::RStar);
        assert_eq!(visited(&search, &word, 0.75).len(), 500);
    }

    /// The obstacle query returns every box that overlaps the gap with positive area, and
    /// the scan returns the same answer for the same predicate.
    #[test]
    fn any_overlapping_agrees_with_the_scan_for_positive_area() {
        let boxes = [
            bx(0.0, 0.0, 10.0, 10.0),
            bx(10.0, 0.0, 20.0, 10.0),
            bx(30.0, 0.0, 40.0, 10.0),
        ];
        let gap = bx(5.0, 2.0, 15.0, 8.0);
        for strategy in [CandidateSearch::Scan, CandidateSearch::RStar] {
            let search = ControlSearch::new(&boxes, strategy);
            let hits = search.any_overlapping(&gap, |i| boxes[i].overlaps(&gap));
            assert!(hits, "{strategy:?}");
            let touching_only = bx(20.0, 0.0, 22.0, 10.0);
            assert!(!search.any_overlapping(&touching_only, |i| boxes[i].overlaps(&touching_only)));
        }
        let empty: [BBox; 0] = [];
        let search = ControlSearch::new(&empty, CandidateSearch::RStar);
        assert!(search.is_empty());
        assert!(!search.any_overlapping(&gap, |_| true));
    }
}
