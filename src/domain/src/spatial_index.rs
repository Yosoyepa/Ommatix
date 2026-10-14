use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU32;

use crate::{ElementId, Point, Rect};

/// Uniform-grid spatial hash mapping screen buckets to the elements overlapping them.
///
/// A uniform grid is used instead of an R-tree because GUI elements have bounded,
/// similar sizes and the scene is updated incrementally every tick: inserts and removals
/// are O(buckets covered), point queries touch one bucket (see bootstrap ADR B0005).
/// Rectangle queries never cost more than the number of occupied buckets, whatever the
/// query size.
#[derive(Debug, Clone)]
pub struct SpatialIndex {
    bucket_size: u32,
    buckets: BTreeMap<(u32, u32), BTreeSet<ElementId>>,
    bounds: BTreeMap<ElementId, Rect>,
}

impl SpatialIndex {
    pub fn new(bucket_size: NonZeroU32) -> Self {
        Self {
            bucket_size: bucket_size.get(),
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

    pub fn get(&self, id: ElementId) -> Option<Rect> {
        self.bounds.get(&id).copied()
    }

    /// Inserts or moves `id` to `rect`. Empty rectangles are not indexed.
    pub fn upsert(&mut self, id: ElementId, rect: Rect) {
        self.remove(id);
        if rect.is_empty() {
            return;
        }
        let (x0, y0, x1, y1) = self.bucket_range(&rect);
        for y in y0..=y1 {
            for x in x0..=x1 {
                self.buckets.entry((x, y)).or_default().insert(id);
            }
        }
        self.bounds.insert(id, rect);
    }

    pub fn remove(&mut self, id: ElementId) -> Option<Rect> {
        let rect = self.bounds.remove(&id)?;
        let (x0, y0, x1, y1) = self.bucket_range(&rect);
        for y in y0..=y1 {
            for x in x0..=x1 {
                if let Some(set) = self.buckets.get_mut(&(x, y)) {
                    set.remove(&id);
                    if set.is_empty() {
                        self.buckets.remove(&(x, y));
                    }
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
        if rect.is_empty() || self.is_empty() {
            return Vec::new();
        }
        let (x0, y0, x1, y1) = self.bucket_range(rect);
        let span = (u64::from(x1 - x0) + 1).saturating_mul(u64::from(y1 - y0) + 1);
        let mut candidates = BTreeSet::new();
        if span > self.buckets.len() as u64 {
            // Huge query: walking the occupied buckets is cheaper than walking the range.
            for ((bx, by), ids) in &self.buckets {
                if (x0..=x1).contains(bx) && (y0..=y1).contains(by) {
                    candidates.extend(ids.iter().copied());
                }
            }
        } else {
            for y in y0..=y1 {
                for x in x0..=x1 {
                    if let Some(ids) = self.buckets.get(&(x, y)) {
                        candidates.extend(ids.iter().copied());
                    }
                }
            }
        }
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

    /// Inclusive bucket coordinates covered by a non-empty rectangle.
    fn bucket_range(&self, rect: &Rect) -> (u32, u32, u32, u32) {
        let s = self.bucket_size;
        (
            rect.x / s,
            rect.y / s,
            (rect.right() - 1) / s,
            (rect.bottom() - 1) / s,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index(bucket: u32) -> SpatialIndex {
        SpatialIndex::new(NonZeroU32::new(bucket).unwrap())
    }

    #[test]
    fn point_query_prefers_smallest_element() {
        let mut idx = index(32);
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
        let mut idx = index(10);
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
        let mut idx = index(16);
        idx.upsert(ElementId(1), Rect::new(60, 10, 10, 10));
        idx.upsert(ElementId(2), Rect::new(10, 10, 10, 10));
        idx.upsert(ElementId(3), Rect::new(10, 40, 10, 10));
        idx.upsert(ElementId(4), Rect::new(300, 300, 10, 10));
        assert_eq!(
            idx.query_rect(&Rect::new(0, 0, 100, 100)),
            vec![ElementId(2), ElementId(1), ElementId(3)]
        );
    }

    #[test]
    fn maximal_queries_are_cheap_and_never_overflow() {
        let mut idx = index(1);
        idx.upsert(ElementId(1), Rect::new(5, 5, 2, 2));
        let all = Rect::new(0, 0, u32::MAX, u32::MAX);
        assert_eq!(idx.query_rect(&all), vec![ElementId(1)]);
        let corner = Rect::new(u32::MAX, u32::MAX, u32::MAX, u32::MAX);
        assert_eq!(idx.query_rect(&corner).len(), 0);
        assert_eq!(idx.query_point(Point::new(u32::MAX, u32::MAX)).len(), 0);
    }

    /// The index must agree with a brute-force scan on arbitrary inputs.
    #[test]
    fn agrees_with_brute_force() {
        let mut rng = 0x2545_f491_4f6c_dd1du64;
        let mut next = |m: u32| {
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            (rng % u64::from(m)) as u32
        };
        let mut idx = index(24);
        let mut all = Vec::new();
        for i in 0..300 {
            let r = Rect::new(next(500), next(500), 1 + next(80), 1 + next(80));
            idx.upsert(ElementId(i), r);
            all.push((ElementId(i), r));
        }
        for _ in 0..300 {
            let q = Rect::new(next(520), next(520), 1 + next(120), 1 + next(120));
            let mut expected: Vec<(u32, u32, ElementId)> = all
                .iter()
                .filter(|(_, r)| r.intersects(&q))
                .map(|(id, r)| (r.y, r.x, *id))
                .collect();
            expected.sort_unstable();
            let expected: Vec<ElementId> = expected.into_iter().map(|t| t.2).collect();
            assert_eq!(idx.query_rect(&q), expected);

            let p = Point::new(next(520), next(520));
            let mut at: Vec<(u64, ElementId)> = all
                .iter()
                .filter(|(_, r)| r.contains(p))
                .map(|(id, r)| (r.area(), *id))
                .collect();
            at.sort_unstable();
            let at: Vec<ElementId> = at.into_iter().map(|t| t.1).collect();
            assert_eq!(idx.query_point(p), at);
        }
    }
}
