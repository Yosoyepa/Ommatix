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
//!
//! The words that differ are also described ([`WordDetail`], O3d): where each lies against
//! the regions the observation re-read, how the two reads' differing words pair up, and
//! whether the words no pairing explains touch the dirty tiles. Counts, closed classes and
//! one distance in pixels; never a text and never a coordinate. A word that differs and lies
//! inside a region is the stated limit L3 of ADR-0045 (the same pixels recognised from a
//! crop); one that crosses a region's edge or lies outside every region is never L3: the
//! first would be a word a region cut, the second a word a regional observation missed.

use ommatix_domain::bbox::BBox;
use ommatix_domain::fusion::Word;

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
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PlanCheck {
    /// In the regional observation's evidence and not in the full read's.
    pub only_regional: CheckKinds,
    /// In the full read's evidence and not in the regional observation's.
    pub only_full: CheckKinds,
    /// The full read's accessibility walk ended on its budget: its absences prove nothing.
    pub full_truncated: bool,
    /// The differing words described by place and pairing (O3d).
    pub words: WordDetail,
}

/// Where differing words of one side lie against the regions the observation re-read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WordPlaces {
    /// Inside one region: the recogniser read these pixels from a crop. A difference here is
    /// the stated limit L3.
    pub inside_a_region: usize,
    /// Meets a region without lying inside any: never L3 (a word a region cut).
    pub crossing_a_region_edge: usize,
    /// Meets no region: never L3 (a word the regional observation held from an earlier read
    /// and a full read sees otherwise).
    pub outside_every_region: usize,
}

impl WordPlaces {
    /// The words that are never the limit L3.
    pub const fn crossing_or_outside(&self) -> usize {
        self.crossing_a_region_edge + self.outside_every_region
    }
}

/// Each differing word of the full read paired with the differing word of the regional
/// observation that overlaps it most and is not paired yet.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct WordPairs {
    /// The same text with another box.
    pub same_text_other_box: usize,
    /// The largest difference between corresponding edges of a same-text pair, in pixels;
    /// 0 when there is no such pair. A distance, not a coordinate.
    pub largest_edge_difference_px: f64,
    /// Another text with an overlapping box.
    pub other_text: usize,
    /// Differing words of the full read that overlap no unpaired word of the regional one.
    pub unpaired_only_full: usize,
    /// Differing words of the regional observation left after the pairing.
    pub unpaired_only_regional: usize,
}

/// The words of a pair with another text, and the unpaired words, of both sides, split by
/// whether each touches a dirty tile of the plan (a tile whose pixels changed since the
/// committed base, or one still pending from an earlier failure): where the screen changed,
/// for instance the caret.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Unexplained {
    pub at_dirty_tiles: usize,
    pub away_from_dirty_tiles: usize,
}

/// The differing words of one check, described (O3d). Counts only.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct WordDetail {
    pub only_full: WordPlaces,
    pub only_regional: WordPlaces,
    pub pairs: WordPairs,
    pub unexplained: Unexplained,
}

impl WordDetail {
    /// Differing words, of both sides, that cross a region's edge or lie outside every
    /// region: never the limit L3.
    pub const fn never_l3(&self) -> usize {
        self.only_full.crossing_or_outside() + self.only_regional.crossing_or_outside()
    }
}

/// Where `b` lies against `regions`.
fn place(b: &BBox, regions: &[BBox], out: &mut WordPlaces) {
    if regions.iter().any(|r| r.contains(b)) {
        out.inside_a_region += 1;
    } else if regions.iter().any(|r| r.overlaps(b)) {
        out.crossing_a_region_edge += 1;
    } else {
        out.outside_every_region += 1;
    }
}

/// The largest difference between corresponding edges of two boxes.
fn edge_difference(a: &BBox, b: &BBox) -> f64 {
    [
        (a.x0() - b.x0()).abs(),
        (a.y0() - b.y0()).abs(),
        (a.x1() - b.x1()).abs(),
        (a.y1() - b.y1()).abs(),
    ]
    .into_iter()
    .fold(0.0, f64::max)
}

/// Describes the differing words: `only_full` (in the full read, not in the regional
/// observation) and `only_regional` (the converse), against the plan's `regions` and the
/// boxes of its `dirty` tiles. Pairing is greedy in the order of `only_full`; on a tie of
/// overlap the first regional word wins.
pub(super) fn word_detail(
    only_full: &[&Word],
    only_regional: &[&Word],
    regions: &[BBox],
    dirty: &[BBox],
) -> WordDetail {
    let mut out = WordDetail::default();
    for w in only_full {
        place(&w.bbox, regions, &mut out.only_full);
    }
    for w in only_regional {
        place(&w.bbox, regions, &mut out.only_regional);
    }
    let mut unexplained: Vec<&BBox> = Vec::new();
    let mut used = vec![false; only_regional.len()];
    for f in only_full {
        let mut best: Option<(usize, f64)> = None;
        for (i, r) in only_regional.iter().enumerate() {
            let a = f.bbox.intersection_area(&r.bbox);
            if !used[i] && a > 0.0 && best.is_none_or(|(_, b)| a > b) {
                best = Some((i, a));
            }
        }
        if let Some((i, _)) = best {
            used[i] = true;
            let r = only_regional[i];
            if r.text == f.text {
                out.pairs.same_text_other_box += 1;
                out.pairs.largest_edge_difference_px = out
                    .pairs
                    .largest_edge_difference_px
                    .max(edge_difference(&f.bbox, &r.bbox));
            } else {
                out.pairs.other_text += 1;
                unexplained.extend([&f.bbox, &r.bbox]);
            }
        } else {
            out.pairs.unpaired_only_full += 1;
            unexplained.push(&f.bbox);
        }
    }
    for (i, r) in only_regional.iter().enumerate() {
        if !used[i] {
            out.pairs.unpaired_only_regional += 1;
            unexplained.push(&r.bbox);
        }
    }
    for b in unexplained {
        if dirty.iter().any(|d| d.overlaps(b)) {
            out.unexplained.at_dirty_tiles += 1;
        } else {
            out.unexplained.away_from_dirty_tiles += 1;
        }
    }
    out
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

    /// One region and one dirty tile inside it, as in the lab's caret scene.
    fn regions() -> Vec<BBox> {
        vec![bx(100.0, 100.0, 300.0, 200.0)]
    }

    fn dirty() -> Vec<BBox> {
        vec![bx(128.0, 128.0, 192.0, 192.0)]
    }

    fn detail(full: &[Word], regional: &[Word]) -> WordDetail {
        let f: Vec<&Word> = full.iter().collect();
        let r: Vec<&Word> = regional.iter().collect();
        word_detail(&f, &r, &regions(), &dirty())
    }

    /// Inside a region, the same text, another box: L3.
    #[test]
    fn o3d_a_word_inside_a_region_with_another_box_is_l3() {
        let d = detail(
            &[Word::new("dos,", bx(210.0, 150.0, 250.0, 170.0))],
            &[Word::new("dos,", bx(211.0, 150.0, 250.5, 171.0))],
        );
        assert_eq!(
            d.only_full,
            WordPlaces {
                inside_a_region: 1,
                ..WordPlaces::default()
            }
        );
        assert_eq!(d.only_regional, d.only_full);
        assert_eq!(d.pairs.same_text_other_box, 1);
        assert!((d.pairs.largest_edge_difference_px - 1.0).abs() < 1e-9);
        assert_eq!(
            (
                d.pairs.other_text,
                d.pairs.unpaired_only_full,
                d.pairs.unpaired_only_regional
            ),
            (0, 0, 0)
        );
        assert_eq!(d.unexplained, Unexplained::default());
        assert_eq!(d.never_l3(), 0);
    }

    /// Inside a region, another text over the caret's tile: L3, and the caret is where.
    #[test]
    fn o3d_a_word_inside_a_region_with_another_text_is_l3_at_the_dirty_tiles() {
        let d = detail(
            &[Word::new("2026.", bx(140.0, 150.0, 180.0, 170.0))],
            &[Word::new("2026.|", bx(140.0, 150.0, 184.0, 170.0))],
        );
        assert_eq!(d.only_full.inside_a_region, 1);
        assert_eq!(d.only_regional.inside_a_region, 1);
        assert_eq!(d.pairs.other_text, 1);
        assert_eq!(d.pairs.same_text_other_box, 0);
        assert!(d.pairs.largest_edge_difference_px.abs() < 1e-9);
        assert_eq!(
            d.unexplained,
            Unexplained {
                at_dirty_tiles: 2,
                away_from_dirty_tiles: 0
            }
        );
        assert_eq!(d.never_l3(), 0);
    }

    /// A full read's word crossing the region's edge, read clipped by the crop: a cut.
    #[test]
    fn o3d_a_word_crossing_a_region_edge_is_never_l3() {
        let d = detail(
            &[Word::new("Linea", bx(280.0, 110.0, 320.0, 130.0))],
            &[Word::new("Line", bx(280.0, 110.0, 300.0, 130.0))],
        );
        assert_eq!(
            d.only_full,
            WordPlaces {
                crossing_a_region_edge: 1,
                ..WordPlaces::default()
            }
        );
        assert_eq!(d.only_regional.inside_a_region, 1);
        assert_eq!(d.pairs.other_text, 1);
        assert_eq!(
            d.unexplained,
            Unexplained {
                at_dirty_tiles: 0,
                away_from_dirty_tiles: 2
            }
        );
        assert_eq!(d.never_l3(), 1);
    }

    /// Words outside every region that differ: held from an earlier read, seen otherwise by
    /// the full read. Never L3; unpaired on both sides when their boxes do not meet.
    #[test]
    fn o3d_a_word_outside_every_region_is_never_l3() {
        let d = detail(
            &[Word::new("tres:", bx(400.0, 300.0, 440.0, 320.0))],
            &[Word::new("tres", bx(500.0, 300.0, 530.0, 320.0))],
        );
        assert_eq!(
            d.only_full,
            WordPlaces {
                outside_every_region: 1,
                ..WordPlaces::default()
            }
        );
        assert_eq!(d.only_regional, d.only_full);
        assert_eq!(
            (d.pairs.unpaired_only_full, d.pairs.unpaired_only_regional),
            (1, 1)
        );
        assert_eq!(d.unexplained.away_from_dirty_tiles, 2);
        assert_eq!(d.never_l3(), 2);
    }

    /// The pairing takes the largest overlap and never pairs one regional word twice.
    #[test]
    fn o3d_the_pairing_takes_the_largest_overlap_once() {
        let d = detail(
            &[
                Word::new("a", bx(110.0, 110.0, 130.0, 130.0)),
                Word::new("b", bx(112.0, 110.0, 132.0, 130.0)),
            ],
            &[
                Word::new("x", bx(120.0, 110.0, 140.0, 130.0)),
                Word::new("a", bx(111.0, 110.0, 131.0, 130.0)),
            ],
        );
        assert_eq!(d.pairs.same_text_other_box, 1);
        assert_eq!(d.pairs.other_text, 1);
        assert_eq!(
            (d.pairs.unpaired_only_full, d.pairs.unpaired_only_regional),
            (0, 0)
        );
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
