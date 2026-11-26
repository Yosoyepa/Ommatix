//! The operator's check of a regional observation against a full read of the same frame
//! (`demo --plan-check`, ADR-0045). Off unless the composition root asks; no MCP client can
//! reach it.
//!
//! After a regional observation is committed, the three perception stages run again on the
//! whole frame, the result is normalised as the observation's own evidence is, and the two
//! are compared box by box. Only counts are kept: the boxes found by one read and not by the
//! other, per kind, split by whether they overlap a box the support-aware closure let a
//! region meet without containing (a spanned surface, or an accessibility box, which R1
//! leaves out of the closure). Its time is never part of the observation's timings.

use ommatix_domain::bbox::BBox;

/// Boxes found by one read and not by the other, of one kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CheckCount {
    /// Overlapping a spanned surface, or an accessibility box, that a region met: where
    /// ADR-0045 changed what a regional observation keeps.
    pub at_spanned_or_whole_on_read: usize,
    /// Everywhere else (for instance OCR, whose locality is approximate: L3).
    pub elsewhere: usize,
}

/// [`CheckCount`] for each kind of evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CheckKinds {
    pub controls: CheckCount,
    pub words: CheckCount,
    pub accessibility: CheckCount,
}

/// The comparison of one regional observation with a full read of its frame. Counts only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PlanCheck {
    /// In the regional observation's evidence and not in the full read's.
    pub only_regional: CheckKinds,
    /// In the full read's evidence and not in the regional observation's.
    pub only_full: CheckKinds,
    /// The full read's accessibility walk ended on its budget: its absences prove nothing.
    pub full_truncated: bool,
}

/// The items of `a` that `b` lacks, as multisets, by equality.
pub(super) fn missing<'a, T: PartialEq>(a: &'a [T], b: &[T]) -> Vec<&'a T> {
    let mut used = vec![false; b.len()];
    a.iter()
        .filter(|x| {
            if let Some(i) = (0..b.len()).find(|&i| !used[i] && b[i] == **x) {
                used[i] = true;
                false
            } else {
                true
            }
        })
        .collect()
}

/// Counts `boxes` by whether each overlaps one of `met`.
pub(super) fn count(boxes: impl Iterator<Item = BBox>, met: &[BBox]) -> CheckCount {
    let mut out = CheckCount::default();
    for b in boxes {
        if met.iter().any(|m| m.overlaps(&b)) {
            out.at_spanned_or_whole_on_read += 1;
        } else {
            out.elsewhere += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bx(x0: f64, y0: f64, x1: f64, y1: f64) -> BBox {
        BBox::new(x0, y0, x1, y1).unwrap()
    }

    #[test]
    fn missing_is_a_multiset_difference() {
        assert_eq!(missing(&[1, 1, 2, 3], &[1, 2, 2]), vec![&1, &3]);
        assert_eq!(missing::<u8>(&[], &[1]), Vec::<&u8>::new());
    }

    #[test]
    fn counts_split_by_the_met_boxes() {
        let met = [bx(0.0, 0.0, 10.0, 10.0)];
        let c = count(
            [bx(5.0, 5.0, 6.0, 6.0), bx(20.0, 20.0, 30.0, 30.0)].into_iter(),
            &met,
        );
        assert_eq!(
            c,
            CheckCount {
                at_spanned_or_whole_on_read: 1,
                elsewhere: 1
            }
        );
    }
}
