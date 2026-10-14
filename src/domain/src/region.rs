use std::collections::BTreeSet;
use std::num::NonZeroU32;

use crate::{CellCoord, ElementId, GridSpec, Rect, SpatialIndex};

/// Bucket edge used to find nearby rectangles.
const MERGE_BUCKET: NonZeroU32 = NonZeroU32::new(64).expect("non-zero");

/// Groups 4-connected dirty cells and returns the pixel bounding box of each group,
/// clipped to `frame_bounds`. Output is sorted (top-to-bottom, left-to-right) so the
/// result is deterministic regardless of input order.
pub fn merge_dirty_cells(cells: &[CellCoord], grid: &GridSpec, frame_bounds: &Rect) -> Vec<Rect> {
    let mut pending: BTreeSet<(u32, u32)> = cells.iter().map(|c| (c.row, c.col)).collect();
    let mut regions = Vec::new();

    while let Some(seed) = pending.pop_first() {
        let mut stack = vec![seed];
        let (mut min_r, mut min_c, mut max_r, mut max_c) = (seed.0, seed.1, seed.0, seed.1);
        while let Some((r, c)) = stack.pop() {
            min_r = min_r.min(r);
            min_c = min_c.min(c);
            max_r = max_r.max(r);
            max_c = max_c.max(c);
            let neighbours = [
                r.checked_sub(1).map(|r| (r, c)),
                Some((r + 1, c)),
                c.checked_sub(1).map(|c| (r, c)),
                Some((r, c + 1)),
            ];
            for n in neighbours.into_iter().flatten() {
                if pending.remove(&n) {
                    stack.push(n);
                }
            }
        }
        let top_left = grid.cell_rect(
            CellCoord {
                col: min_c,
                row: min_r,
            },
            frame_bounds,
        );
        let bottom_right = grid.cell_rect(
            CellCoord {
                col: max_c,
                row: max_r,
            },
            frame_bounds,
        );
        regions.push(top_left.union(&bottom_right));
    }

    regions.sort_by_key(|r| (r.y, r.x));
    regions
}

/// Replaces every group of nearby rectangles by their union until no two rectangles are
/// at most `gap` pixels apart (Chebyshev gap: overlapping and touching rectangles are 0
/// apart). Empty rectangles are dropped; output is sorted.
///
/// Each rectangle in turn absorbs its neighbours, found through a spatial hash, until
/// none is left within reach; absorbed rectangles leave the hash for good. Every query
/// either absorbs something or ends a rectangle's turn, so the work is linear in the
/// number of rectangles times the cost of a query, whatever the layout (a staircase in
/// which each rectangle only reaches the union of the previous ones included).
pub fn merge_nearby(rects: Vec<Rect>, gap: u32) -> Vec<Rect> {
    let mut rects: Vec<Rect> = rects.into_iter().filter(|r| !r.is_empty()).collect();
    rects.sort_by_key(|r| (r.y, r.x, r.w, r.h));
    let mut index = SpatialIndex::new(MERGE_BUCKET);
    for (i, r) in rects.iter().enumerate() {
        index.upsert(ElementId(i as u64), *r);
    }
    let mut alive = vec![true; rects.len()];
    for i in 0..rects.len() {
        if !alive[i] {
            continue;
        }
        index.remove(ElementId(i as u64));
        let mut grown = rects[i];
        loop {
            let near: Vec<ElementId> = index
                .query_rect(&grown.inflate(gap.saturating_add(1)))
                .into_iter()
                .filter(|id| rects[id.0 as usize].gap(&grown) <= gap)
                .collect();
            if near.is_empty() {
                break;
            }
            for id in near {
                grown = grown.union(&rects[id.0 as usize]);
                alive[id.0 as usize] = false;
                index.remove(id);
            }
        }
        rects[i] = grown;
        index.upsert(ElementId(i as u64), grown);
    }
    let mut out: Vec<Rect> = rects
        .into_iter()
        .zip(alive)
        .filter_map(|(r, alive)| alive.then_some(r))
        .collect();
    out.sort_by_key(|r| (r.y, r.x, r.w, r.h));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell(col: u32, row: u32) -> CellCoord {
        CellCoord { col, row }
    }

    #[test]
    fn merges_connected_cells_and_separates_islands() {
        let g = GridSpec::new(10).unwrap();
        let bounds = Rect::new(0, 0, 100, 100);
        let cells = [cell(0, 0), cell(1, 0), cell(1, 1), cell(5, 5)];
        assert_eq!(
            merge_dirty_cells(&cells, &g, &bounds),
            vec![Rect::new(0, 0, 20, 20), Rect::new(50, 50, 10, 10)]
        );
    }

    #[test]
    fn diagonal_cells_are_not_connected() {
        let g = GridSpec::new(10).unwrap();
        let bounds = Rect::new(0, 0, 100, 100);
        assert_eq!(
            merge_dirty_cells(&[cell(0, 0), cell(1, 1)], &g, &bounds).len(),
            2
        );
    }

    #[test]
    fn overlapping_rects_merge_transitively_and_disjoint_ones_stay() {
        let out = merge_nearby(
            vec![
                Rect::new(0, 0, 10, 10),
                Rect::new(100, 100, 5, 5),
                Rect::new(8, 8, 10, 10),
                Rect::new(16, 0, 4, 10), // overlaps only the union of the first and third
                Rect::new(50, 50, 0, 0),
            ],
            0,
        );
        assert_eq!(
            out,
            vec![Rect::new(0, 0, 20, 18), Rect::new(100, 100, 5, 5)]
        );
    }

    #[test]
    fn touching_and_nearby_rects_merge_within_the_gap() {
        let a = Rect::new(0, 0, 10, 10);
        let corner = Rect::new(10, 10, 5, 5); // touches a's corner
        let near = Rect::new(18, 0, 5, 5); // 3 px right of the union
        assert_eq!(
            merge_nearby(vec![a, corner], 0),
            vec![Rect::new(0, 0, 15, 15)]
        );
        assert_eq!(merge_nearby(vec![a, near], 2).len(), 2);
        assert_eq!(
            merge_nearby(vec![a, corner, near], 3),
            vec![Rect::new(0, 0, 23, 15)]
        );
    }

    #[test]
    fn merging_thousands_of_rects_is_fast() {
        // A grid of 1-cell regions one cell apart: nothing is near, so nothing merges.
        let rects: Vec<Rect> = (0..100u32)
            .flat_map(|y| (0..100u32).map(move |x| Rect::new(x * 64, y * 64, 32, 32)))
            .collect();
        let n = rects.len();
        assert_eq!(merge_nearby(rects, 3).len(), n);
    }

    /// Verification N11: each rectangle only overlaps the union of the previous ones, which
    /// took one pass over all rectangles per merge (quadratic) before.
    #[test]
    fn a_staircase_merges_in_one_sweep() {
        let n = 20_000u32;
        let mut rects = vec![Rect::new(0, 0, 2, 2), Rect::new(1, 1, 2, 2)];
        for k in 2..n {
            rects.push(if k % 2 == 0 {
                Rect::new(k, 0, 2, 1)
            } else {
                Rect::new(k, k.div_ceil(2), 2, 2)
            });
        }
        let expected = rects.iter().fold(rects[0], |u, r| u.union(r));
        assert_eq!(merge_nearby(rects, 0), vec![expected]);
    }

    /// Same result as merging pairs until nothing changes, on random rectangles.
    #[test]
    fn matches_the_naive_fixpoint_on_random_rects() {
        let mut rng = 0x2545_f491_4f6c_dd1du64;
        let mut next = |m: u32| {
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            (rng % u64::from(m)) as u32
        };
        for gap in [0, 3] {
            for _ in 0..200 {
                let rects: Vec<Rect> = (0..12)
                    .map(|_| Rect::new(next(200), next(200), next(40), next(40)))
                    .collect();
                assert_eq!(merge_nearby(rects.clone(), gap), naive(rects, gap));
            }
        }
    }

    fn naive(rects: Vec<Rect>, gap: u32) -> Vec<Rect> {
        let mut rects: Vec<Rect> = rects.into_iter().filter(|r| !r.is_empty()).collect();
        'again: loop {
            for i in 0..rects.len() {
                for j in i + 1..rects.len() {
                    if rects[i].gap(&rects[j]) <= gap {
                        let other = rects.remove(j);
                        rects[i] = rects[i].union(&other);
                        continue 'again;
                    }
                }
            }
            break;
        }
        rects.sort_by_key(|r| (r.y, r.x, r.w, r.h));
        rects
    }

    #[test]
    fn regions_are_clipped_to_frame() {
        let g = GridSpec::new(16).unwrap();
        let bounds = Rect::new(0, 0, 20, 20);
        assert_eq!(
            merge_dirty_cells(&[cell(1, 1)], &g, &bounds),
            vec![Rect::new(16, 16, 4, 4)]
        );
    }
}
