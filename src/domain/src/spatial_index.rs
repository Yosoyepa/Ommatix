use std::collections::{BTreeMap, BTreeSet};

use crate::{ElementId, Point, Rect};

/// Uniform-grid spatial hash mapping screen buckets to the elements overlapping them.
///
/// A uniform grid is used instead of an R-tree because GUI elements have bounded,
/// similar sizes and the scene is updated incrementally every tick: inserts and removals
/// are O(buckets covered), queries are O(1) buckets for points (see ADR-0005).
#[derive(Debug, Clone)]
pub struct SpatialIndex {
    bucket_size: u32,
    buckets: BTreeMap<(u32, u32), BTreeSet<ElementId>>,
    bounds: BTreeMap<ElementId, Rect>,
}

impl SpatialIndex {
    pub fn new(bucket_size: u32) -> Self {
        Self {
            bucket_size: bucket_size.max(1),
            buckets: BTreeMap::new(),
            bounds: BTreeMap::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.bounds.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bounds.is_empty()
    }

    /// Inserts or moves `id` to `rect`.
    pub fn upsert(&mut self, id: ElementId, rect: Rect) {
        self.remove(id);
        for key in self.bucket_keys(&rect) {
            self.buckets.entry(key).or_default().insert(id);
        }
        self.bounds.insert(id, rect);
    }

    pub fn remove(&mut self, id: ElementId) -> Option<Rect> {
        let rect = self.bounds.remove(&id)?;
        for key in self.bucket_keys(&rect) {
            if let Some(set) = self.buckets.get_mut(&key) {
                set.remove(&id);
                if set.is_empty() {
                    self.buckets.remove(&key);
                }
            }
        }
        Some(rect)
    }

    /// Elements containing `p`, most specific (smallest area) first, ties by id.
    pub fn query_point(&self, p: Point) -> Vec<ElementId> {
        let key = (p.x / self.bucket_size, p.y / self.bucket_size);
        let mut hits: Vec<(u64, ElementId)> = self
            .buckets
            .get(&key)
            .into_iter()
            .flatten()
            .filter_map(|id| {
                let r = self.bounds[id];
                r.contains(p).then_some((r.area(), *id))
            })
            .collect();
        hits.sort_unstable();
        hits.into_iter().map(|(_, id)| id).collect()
    }

    /// Elements intersecting `rect`, in reading order (top-to-bottom, left-to-right).
    pub fn query_rect(&self, rect: &Rect) -> Vec<ElementId> {
        let candidates: BTreeSet<ElementId> = self
            .bucket_keys(rect)
            .filter_map(|k| self.buckets.get(&k))
            .flatten()
            .copied()
            .collect();
        let mut hits: Vec<(u32, u32, ElementId)> = candidates
            .into_iter()
            .filter_map(|id| {
                let r = self.bounds[&id];
                r.intersects(rect).then_some((r.y, r.x, id))
            })
            .collect();
        hits.sort_unstable();
        hits.into_iter().map(|(_, _, id)| id).collect()
    }

    fn bucket_keys(&self, rect: &Rect) -> impl Iterator<Item = (u32, u32)> + use<> {
        let s = self.bucket_size;
        let (x0, y0) = (rect.x / s, rect.y / s);
        let (x1, y1) = if rect.is_empty() {
            (x0, y0)
        } else {
            ((rect.right() - 1) / s, (rect.bottom() - 1) / s)
        };
        let empty = rect.is_empty();
        (y0..=y1)
            .flat_map(move |y| (x0..=x1).map(move |x| (x, y)))
            .filter(move |_| !empty)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_query_prefers_smallest_element() {
        let mut idx = SpatialIndex::new(32);
        idx.upsert(ElementId(1), Rect::new(0, 0, 200, 200)); // window
        idx.upsert(ElementId(2), Rect::new(40, 40, 30, 10)); // button inside it
        assert_eq!(
            idx.query_point(Point::new(50, 45)),
            vec![ElementId(2), ElementId(1)]
        );
        assert_eq!(idx.query_point(Point::new(150, 150)), vec![ElementId(1)]);
        assert_eq!(idx.query_point(Point::new(500, 500)).len(), 0);
    }

    #[test]
    fn upsert_moves_and_remove_cleans_buckets() {
        let mut idx = SpatialIndex::new(10);
        idx.upsert(ElementId(7), Rect::new(0, 0, 5, 5));
        idx.upsert(ElementId(7), Rect::new(50, 50, 5, 5));
        assert_eq!(idx.query_point(Point::new(1, 1)).len(), 0);
        assert_eq!(idx.query_point(Point::new(51, 51)), vec![ElementId(7)]);
        assert_eq!(idx.remove(ElementId(7)), Some(Rect::new(50, 50, 5, 5)));
        assert!(idx.is_empty());
        assert!(idx.buckets.is_empty());
    }

    #[test]
    fn rect_query_returns_reading_order() {
        let mut idx = SpatialIndex::new(16);
        idx.upsert(ElementId(1), Rect::new(60, 10, 10, 10));
        idx.upsert(ElementId(2), Rect::new(10, 10, 10, 10));
        idx.upsert(ElementId(3), Rect::new(10, 40, 10, 10));
        idx.upsert(ElementId(4), Rect::new(300, 300, 10, 10));
        assert_eq!(
            idx.query_rect(&Rect::new(0, 0, 100, 100)),
            vec![ElementId(2), ElementId(1), ElementId(3)]
        );
    }
}
