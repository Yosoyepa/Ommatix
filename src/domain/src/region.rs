use std::collections::BTreeSet;

use crate::{CellCoord, GridSpec, Rect};

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
    fn regions_are_clipped_to_frame() {
        let g = GridSpec::new(16).unwrap();
        let bounds = Rect::new(0, 0, 20, 20);
        assert_eq!(
            merge_dirty_cells(&[cell(1, 1)], &g, &bounds),
            vec![Rect::new(16, 16, 4, 4)]
        );
    }
}
