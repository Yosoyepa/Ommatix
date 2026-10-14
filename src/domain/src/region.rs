use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU32;

use crate::union_find::UnionFind;
use crate::{CellCoord, ElementId, GridSpec, Rect, SpatialIndex};

/// Bucket edge used to find overlapping rectangles.
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

/// Replaces every group of overlapping rectangles by their union, repeating until no two
/// rectangles overlap. Empty rectangles are dropped; output is sorted.
///
/// Each pass finds overlapping pairs through a spatial hash and joins them with a
/// union-find, so a pass is O(n · neighbours) instead of the O(n²) pair scan.
pub fn merge_overlapping(rects: Vec<Rect>) -> Vec<Rect> {
    let mut rects: Vec<Rect> = rects.into_iter().filter(|r| !r.is_empty()).collect();
    loop {
        if rects.len() < 2 {
            break;
        }
        let mut index = SpatialIndex::new(MERGE_BUCKET);
        for (i, r) in rects.iter().enumerate() {
            index.upsert(ElementId(i as u64), *r);
        }
        let mut groups = UnionFind::new(rects.len());
        let mut merged = false;
        for (i, r) in rects.iter().enumerate() {
            for other in index.query_rect(r) {
                merged |= groups.union(i, other.0 as usize);
            }
        }
        if !merged {
            break;
        }
        let mut unions: BTreeMap<usize, Rect> = BTreeMap::new();
        for (i, r) in rects.iter().enumerate() {
            let root = groups.find(i);
            unions
                .entry(root)
                .and_modify(|u| *u = u.union(r))
                .or_insert(*r);
        }
        rects = unions.into_values().collect();
    }
    rects.sort_by_key(|r| (r.y, r.x, r.w, r.h));
    rects
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
        let out = merge_overlapping(vec![
            Rect::new(0, 0, 10, 10),
            Rect::new(100, 100, 5, 5),
            Rect::new(8, 8, 10, 10),
            Rect::new(16, 0, 4, 10), // overlaps only the union of the first and third
            Rect::new(50, 50, 0, 0),
        ]);
        assert_eq!(
            out,
            vec![Rect::new(0, 0, 20, 18), Rect::new(100, 100, 5, 5)]
        );
    }

    #[test]
    fn merging_thousands_of_rects_is_fast() {
        // A checkerboard of 1-cell regions: nothing overlaps, so nothing merges.
        let rects: Vec<Rect> = (0..100u32)
            .flat_map(|y| (0..100u32).map(move |x| (x, y)))
            .filter(|(x, y)| (x + y) % 2 == 0)
            .map(|(x, y)| Rect::new(x * 32, y * 32, 32, 32))
            .collect();
        let n = rects.len();
        assert_eq!(merge_overlapping(rects).len(), n);
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
