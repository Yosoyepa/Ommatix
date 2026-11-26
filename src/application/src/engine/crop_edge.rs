//! The crop-edge defence (ADR-0045, rule R5; chapter 04 §4.2): a word or a detection read
//! from a region is never committed clipped at the edge of the crop it was read from.
//!
//! The closure closes a region over the boxes the last snapshot **holds**, not over the
//! pixels of what they stand for. A box held tighter than its pixels (an earlier crop read it
//! so) lets the next region end inside the word, and a recogniser that clips at the crop's
//! edge then returns a piece of it. Testing "lies on the region's edge" cannot catch this:
//! the closure sets region edges at held boxes, so a word read correctly lies on the edge as
//! often as a clipped one does (all 70 regional observations of the lab's L6b had a region
//! grown by the closure), and a recogniser may drop a cut glyph instead of returning it,
//! leaving the box short of the edge.
//!
//! So the detector and the recogniser read each region grown by [`CROP_MARGIN_PX`] (the
//! *crop*), and what they find is judged against the region, which the crop holds with room
//! to spare:
//!
//! - **(a) Crossing.** A word, or a detection that is not a surface, that meets a region and
//!   crosses its edge was seen whole by the crop: the region is widened to contain it. The
//!   widening goes to a fixed point, closing again over the held cut-prone boxes (I1) and
//!   over every fresh cut-prone item the widened region now meets, without reading again.
//! - **(b) Near the crop's edge.** One that meets a final region within [`EDGE_BAND_PX`] of a
//!   crop edge that is not the frame's edge (or a final region that left its crop) may have
//!   been clipped by the crop itself: the observation reads once more, with each crop grown
//!   from the widened regions. A second (b) is a full refresh (`closure_budget`).
//!
//! What it keeps: the fresh words and non-surface detections that meet a final region (each
//! then lies inside one), and the surfaces a final region contains (a surface the region only
//! meets is the held one, reported whole or not at all, R2). Accessibility is **not** grown
//! and not concerned: the provider returns every element that meets the regions it reads,
//! whole (R1), so its read regions are decided as before ([`super::Engine::observe`]).
//!
//! The invariant it ends with, when the observation stays regional: every committed fresh
//! word or non-surface detection lies wholly inside a final region and at least
//! [`EDGE_BAND_PX`] from every edge of the crop it was read from that is not the frame's
//! edge; no held cut-prone box is met and not contained by a final region.
//!
//! **Limit L4.** [`EDGE_BAND_PX`] is about one glyph of a 9-10 pt font at 150 % scale. A
//! recogniser that drops a cut glyph wider than the band (a larger scale or font) can still
//! return a word short of the band: the defence makes such a cut unlikely, not impossible.

use ommatix_domain::bbox::BBox;
use ommatix_domain::fusion::{Control, Word};
use ommatix_domain::incremental::{Closure, halo_close, merge_regions};

/// Pixels by which the detector's and the recogniser's crop grows on each side of a region:
/// room to see whole a word or a detection that crosses the region's edge. A constant of its
/// own (not the configured `halo_px`, whose default it happens to equal): twice the band, so
/// that a word lying on a region's edge is a band's width clear of the crop's edge.
pub(super) const CROP_MARGIN_PX: f64 = 32.0;

/// A fresh word or detection that comes within this many pixels of a crop edge that is not
/// the frame's edge may have been clipped by the crop (about one glyph at 150 %, L4).
pub(super) const EDGE_BAND_PX: f64 = 16.0;

/// The crops the detector and the recogniser read for `regions`: each region grown by
/// [`CROP_MARGIN_PX`] and clipped to the frame, overlapping crops merged into their union so
/// that no pixel is read twice and no item is returned twice from two crops.
pub(super) fn crops(regions: &[BBox], bounds: &BBox) -> Vec<BBox> {
    let grown: Vec<BBox> = regions
        .iter()
        .filter_map(|r| r.pad(CROP_MARGIN_PX).clip(bounds))
        .collect();
    merge_regions(&grown)
}

/// Fresh cut-prone items, by kind, as the trace counts them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CropEdgeKinds {
    pub words: usize,
    pub detections: usize,
}

/// What the defence did to one regional observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CropEdgeOutcome {
    /// The planned regions held everything read: nothing changed.
    #[default]
    None,
    /// (a) widened the regions; no second read.
    Widened,
    /// (b) read once more and the second read settled.
    Reread,
    /// A full refresh: a second (b), a closure that ran out of rounds, or the widened regions
    /// over a budget (the plan trace's `decided_by` says which).
    FullRefresh,
}

impl CropEdgeOutcome {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Widened => "widened",
            Self::Reread => "reread",
            Self::FullRefresh => "full_refresh",
        }
    }
}

/// The crop-edge defence of one regional observation (`demo --plan-trace`, object
/// `crop_edge`). Counts and areas only.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct CropEdgeTrace {
    /// [`CROP_MARGIN_PX`].
    pub margin_px: f64,
    /// [`EDGE_BAND_PX`].
    pub band_px: f64,
    /// The planned regions' area (a sum; they do not overlap).
    pub planned_area_px: f64,
    /// The pixels the detector and the recogniser each read, over every read of the
    /// observation: the crops' areas (a sum per read; a full refresh adds the frame).
    pub crop_area_px: f64,
    /// The final regions' area when the observation stayed regional; the frame's otherwise.
    pub final_area_px: f64,
    /// Items of the first read that met a planned region and crossed its edge: (a).
    pub crossing: CropEdgeKinds,
    /// Items of the first read within the band of a non-frame crop edge, plus final regions
    /// that left their crop (counted as detections): (b).
    pub near_crop_edge: CropEdgeKinds,
    /// Reads after the first: 0 or 1.
    pub rereads: usize,
    pub outcome: CropEdgeOutcome,
}

impl CropEdgeTrace {
    pub(super) fn new(planned: &[BBox]) -> Self {
        Self {
            margin_px: CROP_MARGIN_PX,
            band_px: EDGE_BAND_PX,
            planned_area_px: planned.iter().fold(0.0, |t, r| t + r.area()),
            ..Self::default()
        }
    }
}

/// What one read settles to.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Settled {
    /// The final regions: the invariant holds for this read.
    Regions(Vec<BBox>),
    /// (b): read again over these regions (already closed over what came near the edge).
    ReadAgain(Vec<BBox>),
    /// The closure ran out of rounds.
    Exhausted,
}

/// What a read returned that the defence judges, and what the last snapshot holds.
pub(super) struct ReadItems<'a> {
    pub words: &'a [Word],
    pub controls: &'a [Control],
    /// The detector's surfaces of this read (not cut-prone).
    pub surfaces: &'a [BBox],
    /// Held words and non-surface detections.
    pub held_cut_prone: &'a [BBox],
    /// Held relations between cut-prone boxes.
    pub held_relations: &'a [(BBox, BBox)],
}

impl ReadItems<'_> {
    fn is_surface(&self, b: &BBox) -> bool {
        self.surfaces.contains(b)
    }

    /// Fresh cut-prone boxes with their kind (`true` for a word).
    fn fresh(&self) -> impl Iterator<Item = (BBox, bool)> + '_ {
        self.words.iter().map(|w| (w.bbox, true)).chain(
            self.controls
                .iter()
                .filter(|c| !self.is_surface(&c.bbox))
                .map(|c| (c.bbox, false)),
        )
    }
}

fn count(kinds: &mut CropEdgeKinds, word: bool) {
    if word {
        kinds.words += 1;
    } else {
        kinds.detections += 1;
    }
}

/// Whether `b` comes within [`EDGE_BAND_PX`] of an edge of `crop` that is not an edge of
/// the frame `bounds`.
pub(super) fn near_crop_edge(b: &BBox, crop: &BBox, bounds: &BBox) -> bool {
    (crop.x0() > bounds.x0() && b.x0() - crop.x0() < EDGE_BAND_PX)
        || (crop.y0() > bounds.y0() && b.y0() - crop.y0() < EDGE_BAND_PX)
        || (crop.x1() < bounds.x1() && crop.x1() - b.x1() < EDGE_BAND_PX)
        || (crop.y1() < bounds.y1() && crop.y1() - b.y1() < EDGE_BAND_PX)
}

/// Settles one read: (a) to a fixed point, then the test for (b). Counts into `trace` when
/// given (the first read).
pub(super) fn settle(
    regions: &[BBox],
    crops: &[BBox],
    items: &ReadItems<'_>,
    bounds: &BBox,
    max_rounds: usize,
    mut trace: Option<&mut CropEdgeTrace>,
) -> Settled {
    let fresh: Vec<(BBox, bool)> = items.fresh().collect();
    if let Some(t) = trace.as_deref_mut() {
        for (b, word) in &fresh {
            let meets = regions.iter().any(|r| r.overlaps(b));
            if meets && !regions.iter().any(|r| r.contains(b)) {
                count(&mut t.crossing, *word);
            }
        }
    }
    // (a): close over the held cut-prone boxes and the fresh ones, merge, until both hold.
    let supports: Vec<BBox> = items
        .held_cut_prone
        .iter()
        .copied()
        .chain(fresh.iter().map(|(b, _)| *b))
        .collect();
    let mut work = regions.to_vec();
    let finals = loop {
        let merged = merge_regions(&work);
        let mut closed = Vec::with_capacity(merged.len());
        for region in &merged {
            // Every box here lies in the frame (the evidence is clipped to it), so the only
            // error, a box outside the bounds, cannot occur; it is treated as exhaustion.
            match halo_close(
                *region,
                &supports,
                items.held_relations,
                *bounds,
                max_rounds,
            ) {
                Ok(Closure::Closed(r)) => closed.push(r),
                Ok(Closure::BudgetExhausted) | Err(_) => return Settled::Exhausted,
            }
        }
        if closed == merged {
            break merged;
        }
        work = closed;
    };
    // (b): what may have been clipped by the crop itself.
    let mut again = false;
    for region in &finals {
        if !crops.iter().any(|c| c.contains(region)) {
            again = true;
            if let Some(t) = trace.as_deref_mut() {
                t.near_crop_edge.detections += 1;
            }
        }
    }
    for (b, word) in &fresh {
        if !finals.iter().any(|r| r.overlaps(b)) {
            continue;
        }
        let clear = crops
            .iter()
            .find(|c| c.contains(b))
            .is_some_and(|c| !near_crop_edge(b, c, bounds));
        if !clear {
            again = true;
            if let Some(t) = trace.as_deref_mut() {
                count(&mut t.near_crop_edge, *word);
            }
        }
    }
    if again {
        Settled::ReadAgain(finals)
    } else {
        Settled::Regions(finals)
    }
}

/// The fresh words and controls of a settled read that the observation keeps: what meets a
/// final region (cut-prone items, then inside one) and the surfaces a final region contains.
pub(super) fn kept(
    words: Vec<Word>,
    controls: Vec<Control>,
    surfaces: Vec<BBox>,
    finals: &[BBox],
) -> (Vec<Word>, Vec<Control>, Vec<BBox>) {
    let meets = |b: &BBox| finals.iter().any(|r| r.overlaps(b));
    let inside = |b: &BBox| finals.iter().any(|r| r.contains(b));
    let controls = controls
        .into_iter()
        .filter(|c| {
            if surfaces.contains(&c.bbox) {
                inside(&c.bbox)
            } else {
                meets(&c.bbox)
            }
        })
        .collect();
    let words = words.into_iter().filter(|w| meets(&w.bbox)).collect();
    let surfaces = surfaces.into_iter().filter(|s| inside(s)).collect();
    (words, controls, surfaces)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ommatix_domain::fusion::{Actionability, Affordance, Role};

    fn bx(x0: f64, y0: f64, x1: f64, y1: f64) -> BBox {
        BBox::new(x0, y0, x1, y1).unwrap()
    }

    fn frame() -> BBox {
        bx(0.0, 0.0, 640.0, 320.0)
    }

    fn control(b: BBox) -> Control {
        Control {
            bbox: b,
            role: Role::Unknown,
            affordance: Affordance {
                actionability: Actionability::Unknown,
                enabled: true,
                visible: true,
            },
        }
    }

    fn items<'a>(
        words: &'a [Word],
        controls: &'a [Control],
        surfaces: &'a [BBox],
        held: &'a [BBox],
    ) -> ReadItems<'a> {
        ReadItems {
            words,
            controls,
            surfaces,
            held_cut_prone: held,
            held_relations: &[],
        }
    }

    #[test]
    fn crops_grow_by_the_margin_clip_to_the_frame_and_merge() {
        let regions = [
            bx(100.0, 100.0, 200.0, 150.0),
            bx(250.0, 100.0, 300.0, 150.0),
        ];
        assert_eq!(
            crops(&regions, &frame()),
            vec![bx(68.0, 68.0, 332.0, 182.0)]
        );
        assert_eq!(
            crops(&[bx(0.0, 300.0, 50.0, 320.0)], &frame()),
            vec![bx(0.0, 268.0, 82.0, 320.0)]
        );
    }

    /// A word inside the region, clear of the crop's edge: nothing happens.
    #[test]
    fn a_word_inside_is_kept_as_planned() {
        let region = [bx(100.0, 100.0, 200.0, 150.0)];
        let words = [Word::new("w", bx(120.0, 110.0, 200.0, 130.0))];
        let mut t = CropEdgeTrace::new(&region);
        let s = settle(
            &region,
            &crops(&region, &frame()),
            &items(&words, &[], &[], &[]),
            &frame(),
            100,
            Some(&mut t),
        );
        assert_eq!(s, Settled::Regions(region.to_vec()));
        assert_eq!(t.crossing, CropEdgeKinds::default());
        assert_eq!(t.near_crop_edge, CropEdgeKinds::default());
    }

    /// (a): a word crossing the region's edge, seen whole by the crop, widens the region.
    #[test]
    fn a_crossing_word_widens_the_region_without_a_second_read() {
        let region = [bx(100.0, 100.0, 200.0, 150.0)];
        let words = [Word::new("w", bx(180.0, 110.0, 210.0, 130.0))];
        let mut t = CropEdgeTrace::new(&region);
        let s = settle(
            &region,
            &crops(&region, &frame()),
            &items(&words, &[], &[], &[]),
            &frame(),
            100,
            Some(&mut t),
        );
        assert_eq!(s, Settled::Regions(vec![bx(100.0, 100.0, 210.0, 150.0)]));
        assert_eq!(t.crossing.words, 1);
        assert_eq!(t.near_crop_edge, CropEdgeKinds::default());
    }

    /// (a) to a fixed point: the widened region meets a held word and another fresh one.
    #[test]
    fn the_widening_closes_again_over_held_and_fresh_boxes() {
        let region = [bx(100.0, 100.0, 200.0, 150.0)];
        let words = [
            Word::new("a", bx(190.0, 110.0, 205.0, 130.0)),
            Word::new("b", bx(204.0, 140.0, 215.0, 160.0)),
        ];
        let held = [bx(214.0, 120.0, 220.0, 158.0)];
        let s = settle(
            &region,
            &crops(&region, &frame()),
            &items(&words, &[], &[], &held),
            &frame(),
            100,
            None,
        );
        assert_eq!(s, Settled::Regions(vec![bx(100.0, 100.0, 220.0, 160.0)]));
    }

    /// (b): a word that meets the region and reaches the crop's band is read again.
    #[test]
    fn a_word_near_the_crops_edge_is_read_again() {
        let region = [bx(100.0, 100.0, 200.0, 150.0)];
        // The crop ends at x = 232; the word reaches 230, inside the 16 px band.
        let words = [Word::new("w", bx(190.0, 110.0, 230.0, 130.0))];
        let mut t = CropEdgeTrace::new(&region);
        let s = settle(
            &region,
            &crops(&region, &frame()),
            &items(&words, &[], &[], &[]),
            &frame(),
            100,
            Some(&mut t),
        );
        assert_eq!(s, Settled::ReadAgain(vec![bx(100.0, 100.0, 230.0, 150.0)]));
        assert_eq!(t.near_crop_edge.words, 1);
    }

    /// A word on the frame's edge is not near a crop edge: the frame's edges are exempt.
    #[test]
    fn the_frames_edge_is_not_a_crop_edge() {
        let region = [bx(560.0, 100.0, 640.0, 150.0)];
        let words = [Word::new("w", bx(600.0, 110.0, 640.0, 130.0))];
        let s = settle(
            &region,
            &crops(&region, &frame()),
            &items(&words, &[], &[], &[]),
            &frame(),
            100,
            None,
        );
        assert_eq!(s, Settled::Regions(region.to_vec()));
    }

    /// A surface that a region only meets triggers nothing and is not kept; one inside is.
    #[test]
    fn surfaces_never_trigger_and_are_kept_only_inside() {
        let region = [bx(100.0, 100.0, 200.0, 150.0)];
        let spanned = bx(150.0, 60.0, 225.0, 170.0);
        let inner = bx(110.0, 110.0, 140.0, 140.0);
        let controls = [control(spanned), control(inner)];
        let surfaces = [spanned, inner];
        let s = settle(
            &region,
            &crops(&region, &frame()),
            &items(&[], &controls, &surfaces, &[]),
            &frame(),
            100,
            None,
        );
        assert_eq!(s, Settled::Regions(region.to_vec()));
        let (_, kept_controls, kept_surfaces) =
            kept(Vec::new(), controls.to_vec(), surfaces.to_vec(), &region);
        assert_eq!(kept_surfaces, vec![inner]);
        assert_eq!(kept_controls, vec![control(inner)]);
    }

    /// What meets no final region is dropped: the held evidence there stays.
    #[test]
    fn what_only_the_margin_saw_is_dropped() {
        let region = [bx(100.0, 100.0, 200.0, 150.0)];
        let words = vec![
            Word::new("in", bx(120.0, 110.0, 150.0, 130.0)),
            Word::new("margin", bx(205.0, 110.0, 225.0, 130.0)),
        ];
        let (w, _, _) = kept(words, Vec::new(), Vec::new(), &region);
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].text, "in");
    }
}
