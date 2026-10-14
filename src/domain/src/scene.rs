use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{Detection, Element, ElementId, Point, Rect, SpatialIndex};

/// What changed in the scene after applying one frame's detections.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneDelta {
    pub added: Vec<Element>,
    /// Elements that kept their identity but whose bounds or pixels changed.
    pub updated: Vec<Element>,
    pub removed: Vec<ElementId>,
}

impl SceneDelta {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.updated.is_empty() && self.removed.is_empty()
    }
}

/// The current set of perceived elements plus their spatial index.
///
/// The scene is only ever touched inside dirty regions: elements outside them are, by
/// construction, visually unchanged and keep their identity for free.
#[derive(Debug, Clone)]
pub struct Scene {
    elements: BTreeMap<ElementId, Element>,
    index: SpatialIndex,
    next_id: u64,
    identity_iou: f32,
}

impl Scene {
    /// `bucket_size`: spatial index bucket edge in pixels.
    /// `identity_iou`: minimum IoU for a new detection to inherit an old element's id.
    pub fn new(bucket_size: u32, identity_iou: f32) -> Self {
        Self {
            elements: BTreeMap::new(),
            index: SpatialIndex::new(bucket_size),
            next_id: 1,
            identity_iou,
        }
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

    /// Grows each dirty region to fully include any element it touches, so detectors
    /// re-observe whole elements instead of fragments. Overlapping results are merged
    /// to a fixpoint; output is sorted.
    pub fn expand_regions(&self, regions: &[Rect]) -> Vec<Rect> {
        let mut out: Vec<Rect> = regions
            .iter()
            .map(|r| {
                self.index
                    .query_rect(r)
                    .into_iter()
                    .fold(*r, |acc, id| acc.union(&self.elements[&id].bounds))
            })
            .collect();
        loop {
            let mut merged = false;
            'outer: for i in 0..out.len() {
                for j in (i + 1)..out.len() {
                    if out[i].intersects(&out[j]) {
                        out[i] = out[i].union(&out[j]);
                        out.swap_remove(j);
                        merged = true;
                        break 'outer;
                    }
                }
            }
            if !merged {
                break;
            }
        }
        out.sort_by_key(|r| (r.y, r.x));
        out
    }

    /// Replaces everything inside `regions` with `detections` observed in `frame_no`.
    ///
    /// Detections overlapping an invalidated element with IoU >= `identity_iou` (and the
    /// same kind) inherit its id; unmatched old elements are removed.
    pub fn apply(
        &mut self,
        frame_no: u64,
        regions: &[Rect],
        detections: &[Detection],
    ) -> SceneDelta {
        let mut invalidated: Vec<Element> = self
            .elements
            .values()
            .filter(|e| regions.iter().any(|r| r.intersects(&e.bounds)))
            .copied()
            .collect();
        for e in &invalidated {
            self.elements.remove(&e.id);
            self.index.remove(e.id);
        }

        let mut sorted: Vec<Detection> = detections.to_vec();
        sorted.sort_by_key(|d| (d.bounds.y, d.bounds.x, d.bounds.w, d.bounds.h, d.kind));

        let mut delta = SceneDelta::default();
        for det in sorted {
            let best = invalidated
                .iter()
                .enumerate()
                .filter(|(_, old)| old.kind == det.kind)
                .map(|(i, old)| (i, old.bounds.iou(&det.bounds)))
                .filter(|(_, iou)| *iou >= self.identity_iou)
                .max_by(|a, b| a.1.total_cmp(&b.1).then(b.0.cmp(&a.0)));

            let element = if let Some((i, _)) = best {
                let old = invalidated.swap_remove(i);
                let e = Element {
                    bounds: det.bounds,
                    last_changed: frame_no,
                    ..old
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

        delta.removed = invalidated.into_iter().map(|e| e.id).collect();
        delta.removed.sort_unstable();
        delta
    }

    fn resolve(&self, ids: Vec<ElementId>) -> Vec<Element> {
        ids.into_iter().map(|id| self.elements[&id]).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ElementKind;

    fn blob(x: u32, y: u32, w: u32, h: u32) -> Detection {
        Detection {
            bounds: Rect::new(x, y, w, h),
            kind: ElementKind::Blob,
        }
    }

    #[test]
    fn first_apply_adds_everything() {
        let mut s = Scene::new(32, 0.5);
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
        let mut s = Scene::new(32, 0.5);
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
        assert_eq!(s.len(), 2, "element outside the dirty region is untouched");
    }

    #[test]
    fn disappeared_elements_are_removed() {
        let mut s = Scene::new(32, 0.5);
        s.apply(1, &[Rect::new(0, 0, 100, 100)], &[blob(10, 10, 20, 10)]);
        let d = s.apply(2, &[Rect::new(0, 0, 32, 32)], &[]);
        assert_eq!(d.removed, vec![ElementId(1)]);
        assert!(s.is_empty());
    }

    #[test]
    fn expand_regions_covers_touched_elements_and_merges() {
        let mut s = Scene::new(32, 0.5);
        s.apply(1, &[Rect::new(0, 0, 200, 200)], &[blob(10, 10, 100, 20)]);
        let out = s.expand_regions(&[Rect::new(0, 0, 16, 16), Rect::new(96, 16, 16, 16)]);
        assert_eq!(out, vec![Rect::new(0, 0, 112, 32)]);
    }
}
