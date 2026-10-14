use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU32;

use crate::{Detection, DomainError, Element, ElementId, Point, Rect, SpatialIndex, merge_nearby};

/// What changed in the scene after applying one frame's detections.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SceneDelta {
    pub added: Vec<Element>,
    /// Elements whose area was re-analysed and that kept their identity. Their bounds may
    /// or may not have changed; their pixels did (or were adjacent to a change).
    pub updated: Vec<Element>,
    pub removed: Vec<ElementId>,
}

impl SceneDelta {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.updated.is_empty() && self.removed.is_empty()
    }

    /// The net change of applying `self` and then `later`: an element added and then
    /// removed disappears, an element added and then updated is reported as added with its
    /// latest bounds. Ids are never reused, which keeps the composition well defined.
    #[must_use]
    pub fn then(self, later: SceneDelta) -> SceneDelta {
        let mut added: BTreeMap<ElementId, Element> =
            self.added.into_iter().map(|e| (e.id, e)).collect();
        let mut updated: BTreeMap<ElementId, Element> =
            self.updated.into_iter().map(|e| (e.id, e)).collect();
        let mut removed: BTreeSet<ElementId> = self.removed.into_iter().collect();

        for e in later.added {
            added.insert(e.id, e);
        }
        for e in later.updated {
            if let Some(slot) = added.get_mut(&e.id) {
                *slot = e;
            } else {
                updated.insert(e.id, e);
            }
        }
        for id in later.removed {
            if added.remove(&id).is_none() {
                updated.remove(&id);
                removed.insert(id);
            }
        }

        let reading_order = |e: &Element| (e.bounds.y, e.bounds.x, e.id);
        let mut added: Vec<Element> = added.into_values().collect();
        added.sort_by_key(reading_order);
        let mut updated: Vec<Element> = updated.into_values().collect();
        updated.sort_by_key(reading_order);
        SceneDelta {
            added,
            updated,
            removed: removed.into_iter().collect(),
        }
    }
}

/// The current set of perceived elements plus their spatial index.
///
/// The scene is only ever touched inside re-analysed regions: elements outside them are,
/// by construction, visually unchanged and keep their identity for free.
#[derive(Debug, Clone)]
pub struct Scene {
    elements: BTreeMap<ElementId, Element>,
    index: SpatialIndex,
    bucket_size: NonZeroU32,
    next_id: u64,
    identity_iou: f32,
}

impl Scene {
    /// `bucket_size`: spatial index bucket edge in pixels.
    /// `identity_iou`: minimum IoU, in `(0, 1]`, for a new detection to inherit an old
    /// element's id.
    pub fn new(bucket_size: NonZeroU32, identity_iou: f32) -> Result<Self, DomainError> {
        if !(identity_iou > 0.0 && identity_iou <= 1.0) {
            return Err(DomainError::InvalidIdentityIou(identity_iou));
        }
        Ok(Self {
            elements: BTreeMap::new(),
            index: SpatialIndex::new(bucket_size),
            bucket_size,
            next_id: 1,
            identity_iou,
        })
    }

    pub fn len(&self) -> usize {
        self.elements.len()
    }

    pub fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }

    pub fn get(&self, id: ElementId) -> Option<&Element> {
        self.elements.get(&id)
    }

    /// Forgets every element (the screen geometry changed) and returns their ids in
    /// ascending order. Ids keep increasing afterwards: they are never reused.
    pub fn clear(&mut self) -> Vec<ElementId> {
        self.index = SpatialIndex::new(self.bucket_size);
        std::mem::take(&mut self.elements).into_keys().collect()
    }

    /// All elements in reading order.
    pub fn elements(&self) -> Vec<Element> {
        let mut all: Vec<Element> = self.elements.values().copied().collect();
        all.sort_by_key(|e| (e.bounds.y, e.bounds.x, e.id));
        all
    }

    pub fn elements_at(&self, p: Point) -> Vec<Element> {
        self.resolve(self.index.query_point(p))
    }

    pub fn elements_in(&self, rect: &Rect) -> Vec<Element> {
        self.resolve(self.index.query_rect(rect))
    }

    /// Turns changed areas into the regions that must be re-analysed.
    ///
    /// `margin` is how far the detector's grouping reaches (its context margin, declared
    /// by the application's `ElementDetector` port): an element at most `margin` pixels
    /// from a region (touching it, at 0) could be reported together with what is inside
    /// it, so it is re-analysed too.
    ///
    /// Every element within reach of a region is absorbed whole, so detectors see complete
    /// elements, **except** elements that enclose the region, i.e. hold it in their interior
    /// without sharing an edge (a window around a changed button): their outline did not
    /// change, so neither did their bounds. Growing and merging regions within `margin` of
    /// each other repeat until nothing changes, which guarantees that every element within
    /// `margin` of an output region is either inside it or encloses it. Output is sorted.
    pub fn expand_regions(&self, regions: &[Rect], margin: u32) -> Vec<Rect> {
        let reach = margin.saturating_add(1);
        let mut out = merge_nearby(regions.to_vec(), margin);
        loop {
            let mut grown = false;
            for region in &mut out {
                loop {
                    let absorb: Vec<Rect> = self
                        .index
                        .query_rect(&region.inflate(reach))
                        .into_iter()
                        .map(|id| self.elements[&id].bounds)
                        .filter(|b| {
                            b.gap(region) <= margin
                                && !region.contains_rect(b)
                                && !encloses(b, region)
                        })
                        .collect();
                    if absorb.is_empty() {
                        break;
                    }
                    for b in absorb {
                        *region = region.union(&b);
                    }
                    grown = true;
                }
            }
            let count = out.len();
            out = merge_nearby(out, margin);
            if !grown && out.len() == count {
                return out;
            }
        }
    }

    /// Replaces everything inside `regions` (as returned by [`Scene::expand_regions`])
    /// with `detections` observed in `frame_no`.
    ///
    /// Elements lying inside a region are invalidated. Detections and invalidated elements
    /// of the same kind are then paired globally, best IoU first (ties: older id first),
    /// and a pair with IoU >= `identity_iou` keeps the old id. Unpaired detections get new
    /// ids; unpaired old elements are removed.
    pub fn apply(
        &mut self,
        frame_no: u64,
        regions: &[Rect],
        detections: &[Detection],
    ) -> SceneDelta {
        let mut stale: BTreeMap<ElementId, Element> = BTreeMap::new();
        for region in regions {
            for id in self.index.query_rect(region) {
                let e = self.elements[&id];
                if region.contains_rect(&e.bounds) {
                    stale.insert(id, e);
                }
            }
        }
        for id in stale.keys() {
            self.elements.remove(id);
            self.index.remove(*id);
        }

        let mut detections: Vec<Detection> = detections
            .iter()
            .filter(|d| !d.bounds.is_empty())
            .copied()
            .collect();
        detections.sort_by_key(|d| (d.bounds.y, d.bounds.x, d.bounds.w, d.bounds.h, d.kind));
        let matches = self.match_identities(&stale, &detections);

        let mut delta = SceneDelta::default();
        for (det, matched) in detections.iter().zip(&matches) {
            let element = if let Some(id) = matched {
                let e = Element {
                    bounds: det.bounds,
                    last_changed: frame_no,
                    ..stale[id]
                };
                delta.updated.push(e);
                e
            } else {
                let e = Element {
                    id: ElementId(self.next_id),
                    bounds: det.bounds,
                    kind: det.kind,
                    first_seen: frame_no,
                    last_changed: frame_no,
                };
                self.next_id += 1;
                delta.added.push(e);
                e
            };
            self.index.upsert(element.id, element.bounds);
            self.elements.insert(element.id, element);
        }

        let kept: BTreeSet<ElementId> = matches.into_iter().flatten().collect();
        delta.removed = stale.into_keys().filter(|id| !kept.contains(id)).collect();
        delta
    }

    /// For each detection, the stale element whose id it inherits (global greedy matching
    /// on IoU, so the best pair always wins regardless of detection order).
    fn match_identities(
        &self,
        stale: &BTreeMap<ElementId, Element>,
        detections: &[Detection],
    ) -> Vec<Option<ElementId>> {
        let mut stale_index = SpatialIndex::new(self.bucket_size);
        for (id, e) in stale {
            stale_index.upsert(*id, e.bounds);
        }
        let mut pairs: Vec<(f32, ElementId, usize)> = Vec::new();
        for (i, det) in detections.iter().enumerate() {
            for id in stale_index.query_rect(&det.bounds) {
                let old = &stale[&id];
                let iou = old.bounds.iou(&det.bounds);
                if old.kind == det.kind && iou >= self.identity_iou {
                    pairs.push((iou, id, i));
                }
            }
        }
        pairs.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));

        let mut matches = vec![None; detections.len()];
        let mut taken = BTreeSet::new();
        for (_, id, i) in pairs {
            if matches[i].is_none() && taken.insert(id) {
                matches[i] = Some(id);
            }
        }
        matches
    }

    fn resolve(&self, ids: Vec<ElementId>) -> Vec<Element> {
        ids.into_iter().map(|id| self.elements[&id]).collect()
    }
}

/// `inner` lies in the interior of `outer`: inside it and sharing none of its edges, so
/// `outer`'s outermost rows and columns are not part of `inner`.
fn encloses(outer: &Rect, inner: &Rect) -> bool {
    outer.x < inner.x
        && outer.y < inner.y
        && inner.right() < outer.right()
        && inner.bottom() < outer.bottom()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ElementKind;

    fn scene() -> Scene {
        Scene::new(NonZeroU32::new(32).unwrap(), 0.5).unwrap()
    }

    fn blob(x: u32, y: u32, w: u32, h: u32) -> Detection {
        Detection {
            bounds: Rect::new(x, y, w, h),
            kind: ElementKind::Blob,
        }
    }

    #[test]
    fn rejects_invalid_identity_threshold() {
        let bucket = NonZeroU32::new(8).unwrap();
        for bad in [0.0, -1.0, 1.5, f32::NAN] {
            assert!(Scene::new(bucket, bad).is_err(), "{bad}");
        }
        assert!(Scene::new(bucket, 1.0).is_ok());
    }

    #[test]
    fn first_apply_adds_everything() {
        let mut s = scene();
        let d = s.apply(
            1,
            &[Rect::new(0, 0, 100, 100)],
            &[blob(10, 10, 20, 10), blob(50, 50, 5, 5)],
        );
        assert_eq!(d.added.len(), 2);
        assert_eq!(s.elements_at(Point::new(15, 15))[0].id, ElementId(1));
    }

    #[test]
    fn identity_survives_small_moves_and_untouched_elements_are_kept() {
        let mut s = scene();
        s.apply(
            1,
            &[Rect::new(0, 0, 200, 200)],
            &[blob(10, 10, 20, 10), blob(150, 150, 10, 10)],
        );
        // Only the top-left area changed; the button shifted by one pixel.
        let d = s.apply(2, &[Rect::new(0, 0, 64, 64)], &[blob(11, 10, 20, 10)]);
        assert!(d.added.is_empty() && d.removed.is_empty());
        assert_eq!(d.updated[0].id, ElementId(1));
        assert_eq!(d.updated[0].first_seen, 1);
        assert_eq!(s.len(), 2, "element outside the region is untouched");
    }

    #[test]
    fn best_overlap_wins_identity_regardless_of_order() {
        let mut s = scene();
        s.apply(1, &[Rect::new(0, 0, 200, 200)], &[blob(10, 10, 100, 30)]);
        // A focus ring (sorted first) and the exact button come back together.
        let d = s.apply(
            2,
            &[Rect::new(0, 0, 200, 200)],
            &[blob(8, 8, 104, 34), blob(10, 10, 100, 30)],
        );
        assert_eq!(d.updated.len(), 1);
        assert_eq!(d.updated[0].id, ElementId(1));
        assert_eq!(d.updated[0].bounds, Rect::new(10, 10, 100, 30));
        assert_eq!(d.added[0].bounds, Rect::new(8, 8, 104, 34));
    }

    #[test]
    fn disappeared_elements_are_removed() {
        let mut s = scene();
        s.apply(1, &[Rect::new(0, 0, 100, 100)], &[blob(10, 10, 20, 10)]);
        let d = s.apply(2, &[Rect::new(0, 0, 32, 32)], &[]);
        assert_eq!(d.removed, vec![ElementId(1)]);
        assert!(s.is_empty());
    }

    #[test]
    fn expand_regions_covers_touched_elements_and_merges() {
        let mut s = scene();
        s.apply(1, &[Rect::new(0, 0, 200, 200)], &[blob(10, 10, 100, 20)]);
        let out = s.expand_regions(&[Rect::new(0, 0, 16, 16), Rect::new(96, 16, 16, 16)], 0);
        assert_eq!(out, vec![Rect::new(0, 0, 112, 32)]);
    }

    #[test]
    fn expansion_repeats_until_no_element_is_cut() {
        let mut s = scene();
        let a = blob(2, 2, 38, 38);
        let c = blob(2, 50, 18, 40);
        s.apply(1, &[Rect::new(0, 0, 200, 200)], &[a, c]);
        // A small change inside A; growing to A would cut C if expansion stopped there.
        let out = s.expand_regions(&[Rect::new(2, 2, 62, 62)], 0);
        assert_eq!(out, vec![Rect::new(2, 2, 62, 88)]);
        assert!(out[0].contains_rect(&c.bounds));
    }

    #[test]
    fn containers_of_a_change_are_neither_expanded_nor_invalidated() {
        let mut s = scene();
        let window = blob(10, 10, 1000, 740);
        let button = blob(100, 100, 80, 30);
        s.apply(1, &[Rect::new(0, 0, 1100, 800)], &[window, button]);

        let regions = s.expand_regions(&[Rect::new(96, 96, 32, 32)], 3);
        assert_eq!(
            regions,
            vec![Rect::new(96, 96, 84, 34)],
            "button, not window"
        );

        let d = s.apply(2, &regions, &[blob(100, 100, 80, 30)]);
        assert_eq!(d.updated.len(), 1, "only the button is re-observed");
        assert_eq!(d.removed.len(), 0);
        assert_eq!(s.len(), 2);
    }

    /// Verification N2: a container whose edge lies in the changed area may have moved that
    /// edge, so it is re-analysed; only a container holding the change in its interior is
    /// left alone.
    #[test]
    fn containers_sharing_an_edge_with_the_change_are_re_detected() {
        let mut s = scene();
        let panel = blob(64, 64, 384, 256);
        s.apply(1, &[Rect::new(0, 0, 640, 480)], &[panel]);
        // The panel's bottom half turned into background: the change lies inside its old
        // bounds, but on its left, right and bottom edges.
        assert_eq!(
            s.expand_regions(&[Rect::new(64, 192, 384, 128)], 0),
            vec![panel.bounds]
        );
        let inside = Rect::new(96, 96, 32, 32);
        assert_eq!(s.expand_regions(&[inside], 0), vec![inside]);
    }

    /// Verification N5: an element the detector could group with something in the region
    /// (a glyph 2 px away) is re-analysed with it, and so is an element touching it.
    #[test]
    fn elements_within_the_detector_margin_are_absorbed() {
        let mut s = scene();
        let near = blob(88, 200, 6, 12); // ends 2 px before the changed cell
        let touching = blob(128, 200, 6, 12); // starts on the cell's right edge
        s.apply(1, &[Rect::new(0, 0, 320, 320)], &[near, touching]);
        let cell = Rect::new(96, 192, 32, 32);
        assert_eq!(
            s.expand_regions(&[cell], 0),
            vec![Rect::new(96, 192, 38, 32)],
            "touching only"
        );
        assert_eq!(
            s.expand_regions(&[cell], 3),
            vec![Rect::new(88, 192, 46, 32)]
        );
    }

    /// After expansion, every element within the margin of a region is inside it or
    /// encloses it, and regions are farther apart than the margin, on arbitrary scenes.
    #[test]
    fn expansion_invariant_holds_on_random_scenes() {
        let mut rng = 0x9e37_79b9_7f4a_7c15u64;
        let mut next = |m: u32| {
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            (rng % u64::from(m)) as u32
        };
        for round in 0..50 {
            let mut s = scene();
            let detections: Vec<Detection> = (0..40)
                .map(|_| blob(next(400), next(400), 1 + next(120), 1 + next(120)))
                .collect();
            s.apply(1, &[Rect::new(0, 0, 600, 600)], &detections);
            let changes: Vec<Rect> = (0..5)
                .map(|_| Rect::new(next(500), next(500), 1 + next(64), 1 + next(64)))
                .collect();
            let margin = round % 4;
            let regions = s.expand_regions(&changes, margin);
            for r in &regions {
                for e in s.elements() {
                    if e.bounds.gap(r) <= margin {
                        assert!(
                            r.contains_rect(&e.bounds) || encloses(&e.bounds, r),
                            "round {round}: {e:?} cut by {r:?}"
                        );
                    }
                }
            }
            for (i, a) in regions.iter().enumerate() {
                for b in &regions[i + 1..] {
                    assert!(a.gap(b) > margin, "regions within the margin");
                }
            }
            for c in &changes {
                assert!(regions.iter().any(|r| r.contains_rect(c)), "change lost");
            }
        }
    }

    #[test]
    fn deltas_compose_into_the_net_change() {
        let e = |id: u64, x: u32| Element {
            id: ElementId(id),
            bounds: Rect::new(x, 0, 5, 5),
            kind: ElementKind::Blob,
            first_seen: 1,
            last_changed: 1,
        };
        let first = SceneDelta {
            added: vec![e(5, 0), e(6, 10)],
            updated: vec![e(1, 20)],
            removed: vec![ElementId(2)],
        };
        let second = SceneDelta {
            added: vec![e(7, 30)],
            updated: vec![e(5, 1), e(3, 40)],
            removed: vec![ElementId(6), ElementId(1)],
        };
        let net = first.then(second);
        assert_eq!(net.added, vec![e(5, 1), e(7, 30)]);
        assert_eq!(net.updated, vec![e(3, 40)]);
        assert_eq!(net.removed, vec![ElementId(1), ElementId(2)]);
    }

    #[test]
    fn clear_forgets_elements_but_never_reuses_ids() {
        let mut s = scene();
        let first = s.apply(1, &[Rect::new(0, 0, 100, 100)], &[blob(10, 10, 20, 10)]);
        assert_eq!(s.clear(), vec![first.added[0].id]);
        assert!(s.is_empty());
        assert_eq!(s.elements_at(Point::new(15, 15)).len(), 0);
        let again = s.apply(2, &[Rect::new(0, 0, 100, 100)], &[blob(10, 10, 20, 10)]);
        assert!(again.added[0].id > first.added[0].id);
    }
}
