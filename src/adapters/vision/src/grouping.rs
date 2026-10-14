//! Turns segment boxes into element boxes: drops boxes that repeat a bigger one and
//! groups glyphs into words.
//!
//! Both passes find neighbours through a [`Grid`] built for the pass, with 4 px cells
//! unless the boxes are sparse. The domain's `SpatialIndex` is sized for elements (64 px
//! buckets): on a screen of tiny dots each bucket holds hundreds of boxes, and every search
//! scanned them all.

use std::cmp::Reverse;

use ommatix_domain::Rect;

use crate::forest::Forest;

/// Preferred cell side, as a power of two (4 px: the size of the dots and specks that
/// pack dense screens). [`Grid::new`] coarsens it when the boxes are sparse.
const CELL_SHIFT: u32 = 2;

/// Drops every box that lies inside a bigger box with all four edges at most `inset`
/// pixels in: a fill inside its own border, the hole of an "o". The bigger box is kept.
pub(crate) fn drop_nested_duplicates(mut boxes: Vec<Rect>, inset: u32) -> Vec<Rect> {
    // Biggest first, so a box can only repeat the boxes after it. Equal keys are equal
    // boxes, so an unstable sort gives the same order as a stable one.
    boxes.sort_unstable_by_key(|r| (Reverse(r.area()), r.y, r.x, r.w, r.h));
    // A repeated box lies inside the box repeating it, and its centre is at most
    // ceil(inset / 2) px from that box's centre on each axis (each edge moves in by
    // 0..=inset): each kept box looks for the centres around its own and drops the boxes
    // it repeats.
    let centres: Vec<Rect> = boxes.iter().map(centre).collect();
    let grid = Grid::new(&centres, CELL_SHIFT);
    let reach = inset.div_ceil(2);
    let mut dropped = vec![false; boxes.len()];
    let mut kept = Vec::with_capacity(boxes.len());
    for (i, outer) in boxes.iter().enumerate() {
        if dropped[i] {
            continue;
        }
        kept.push(*outer);
        if let Some(around) = centre(outer).inflate(reach).intersection(outer) {
            grid.for_each_intersecting(&around, i + 1, |j| {
                if !dropped[j] && repeats(outer, &boxes[j], inset) {
                    dropped[j] = true;
                }
            });
        }
    }
    kept
}

/// The pixel at the centre of a non-empty box; an empty box stays empty (never indexed).
fn centre(r: &Rect) -> Rect {
    if r.is_empty() {
        *r
    } else {
        let c = r.center();
        Rect::new(c.x, c.y, 1, 1)
    }
}

fn repeats(outer: &Rect, inner: &Rect, inset: u32) -> bool {
    outer.contains_rect(inner)
        && inner.x - outer.x <= inset
        && inner.y - outer.y <= inset
        && outer.right() - inner.right() <= inset
        && outer.bottom() - inner.bottom() <= inset
}

/// Groups glyph-sized boxes (both sides at most `max_side`) that are at most `gap`
/// pixels apart and not nested in each other: glyphs into words, an icon with its label.
/// Larger boxes pass through unchanged.
pub(crate) fn merge_glyphs(boxes: Vec<Rect>, max_side: u32, gap: u32) -> Vec<Rect> {
    let (glyphs, mut out): (Vec<Rect>, Vec<Rect>) = boxes
        .into_iter()
        .partition(|r| r.w <= max_side && r.h <= max_side);
    let grid = Grid::new(&glyphs, CELL_SHIFT);
    let mut words = Forest::new(glyphs.len());
    for (i, g) in glyphs.iter().enumerate() {
        // Between non-empty boxes the test is symmetric, so each pair is tested from its
        // first box only. An empty box is not indexed: it must find its partners itself.
        let from = if g.is_empty() { 0 } else { i + 1 };
        // Inflating by gap + 1 finds every box whose Chebyshev gap is <= gap.
        grid.for_each_intersecting(&g.inflate(gap.saturating_add(1)), from, |j| {
            let other = &glyphs[j];
            let nested = g.contains_rect(other) || other.contains_rect(g);
            if g.gap(other) <= gap && !nested {
                words.union(i, j);
            }
        });
    }
    // One box per word, in order of its first glyph.
    let mut word: Vec<Option<Rect>> = vec![None; glyphs.len()];
    for (i, g) in glyphs.iter().enumerate() {
        let w = &mut word[words.find(i)];
        *w = Some(w.map_or(*g, |w| w.union(g)));
    }
    out.extend(word.into_iter().flatten());
    out
}

/// A uniform grid over the boxes of one pass: every non-empty box is listed in each cell
/// it overlaps, in index order. Cells are stored as compressed rows, so building costs
/// O(boxes + cells) and a query only visits the cells it covers.
#[derive(Debug)]
struct Grid<'a> {
    boxes: &'a [Rect],
    /// Bounds of the non-empty boxes: `[x0, x1) x [y0, y1)`.
    x0: u32,
    y0: u32,
    x1: u32,
    y1: u32,
    /// Cell side is `1 << shift`.
    shift: u32,
    cols: usize,
    /// The boxes in cell `c` (row-major) are `items[start[c]..start[c + 1]]`.
    start: Vec<usize>,
    items: Vec<u32>,
}

impl<'a> Grid<'a> {
    /// Indexes `boxes` (at most `u32::MAX` of them, like every per-box index in this
    /// crate) with cells of side `1 << shift`, doubled while there are more than a few
    /// cells per box.
    fn new(boxes: &'a [Rect], mut shift: u32) -> Self {
        let count = u32::try_from(boxes.len()).expect("box count fits in u32");
        let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
        for b in boxes.iter().filter(|b| !b.is_empty()) {
            x0 = x0.min(b.x);
            y0 = y0.min(b.y);
            x1 = x1.max(b.right());
            y1 = y1.max(b.bottom());
        }
        let mut grid = Self {
            boxes,
            x0,
            y0,
            x1,
            y1,
            shift,
            cols: 0,
            start: vec![0],
            items: Vec::new(),
        };
        if x0 >= x1 {
            return grid; // nothing to index
        }
        // At shift 31 the grid is at most 2x2 cells, under any budget.
        let budget = 4 * u64::from(count) + 256;
        let side = |extent: u32, shift: u32| u64::from((extent - 1) >> shift) + 1;
        while side(x1 - x0, shift) * side(y1 - y0, shift) > budget {
            shift += 1;
        }
        grid.shift = shift;
        grid.cols = side(x1 - x0, shift) as usize;
        let cells = grid.cols * side(y1 - y0, shift) as usize;

        // Count the boxes of each cell, turn counts into cell ends, then fill each cell
        // back to front so that it lists its boxes in index order.
        let mut start = vec![0usize; cells + 1];
        for b in boxes {
            grid.for_each_cell(b, |c| start[c] += 1);
        }
        for c in 1..=cells {
            start[c] += start[c - 1];
        }
        let mut items = vec![0u32; start[cells]];
        for (i, b) in boxes.iter().enumerate().rev() {
            grid.for_each_cell(b, |c| {
                start[c] -= 1;
                items[start[c]] = i as u32;
            });
        }
        grid.start = start;
        grid.items = items;
        grid
    }

    /// Cells covered by the part of `r` inside the indexed bounds, as inclusive column and
    /// row ranges.
    fn cell_range(&self, r: &Rect) -> Option<((usize, usize), (usize, usize))> {
        let (left, right) = (r.x.max(self.x0), r.right().min(self.x1));
        let (top, bottom) = (r.y.max(self.y0), r.bottom().min(self.y1));
        if left >= right || top >= bottom {
            return None;
        }
        let col = |x: u32| ((x - self.x0) >> self.shift) as usize;
        let row = |y: u32| ((y - self.y0) >> self.shift) as usize;
        Some(((col(left), col(right - 1)), (row(top), row(bottom - 1))))
    }

    fn for_each_cell(&self, r: &Rect, mut f: impl FnMut(usize)) {
        if r.is_empty() {
            return;
        }
        if let Some(((c0, c1), (r0, r1))) = self.cell_range(r) {
            for row in r0..=r1 {
                for col in c0..=c1 {
                    f(row * self.cols + col);
                }
            }
        }
    }

    /// Calls `f` once with the index of every box at index `from` or later that
    /// intersects `area`.
    fn for_each_intersecting(&self, area: &Rect, from: usize, mut f: impl FnMut(usize)) {
        let Some(((c0, c1), (r0, r1))) = self.cell_range(area) else {
            return;
        };
        for row in r0..=r1 {
            for col in c0..=c1 {
                let c = row * self.cols + col;
                // A cell lists its boxes in index order: walk it back down to `from`.
                for &j in self.items[self.start[c]..self.start[c + 1]].iter().rev() {
                    if (j as usize) < from {
                        break;
                    }
                    let b = &self.boxes[j as usize];
                    // A box listed in several cells is reported from the cell holding the
                    // top-left corner of its overlap with `area` only.
                    if b.intersects(area)
                        && ((area.x.max(b.x) - self.x0) >> self.shift) as usize == col
                        && ((area.y.max(b.y) - self.y0) >> self.shift) as usize == row
                    {
                        f(j as usize);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    /// xorshift64: deterministic pseudo-random numbers in `0..m`.
    fn rng(mut state: u64) -> impl FnMut(u32) -> u32 {
        move |m| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state % u64::from(m)) as u32
        }
    }

    /// `drop_nested_duplicates` by its definition: every box, biggest first, unless a box
    /// kept before it repeats it.
    fn drop_by_definition(mut boxes: Vec<Rect>, inset: u32) -> Vec<Rect> {
        boxes.sort_by_key(|r| (Reverse(r.area()), r.y, r.x, r.w, r.h));
        let mut kept: Vec<Rect> = Vec::new();
        for b in boxes {
            if !kept.iter().any(|k| repeats(k, &b, inset)) {
                kept.push(b);
            }
        }
        kept
    }

    /// `merge_glyphs` by its definition, comparing every pair of glyphs. An empty box,
    /// which no search finds, only joins boxes that are not empty.
    fn merge_by_definition(boxes: Vec<Rect>, max_side: u32, gap: u32) -> Vec<Rect> {
        let (glyphs, mut out): (Vec<Rect>, Vec<Rect>) = boxes
            .into_iter()
            .partition(|r| r.w <= max_side && r.h <= max_side);
        let mut words = Forest::new(glyphs.len());
        for (i, a) in glyphs.iter().enumerate() {
            for (j, b) in glyphs.iter().enumerate() {
                let nested = a.contains_rect(b) || b.contains_rect(a);
                if !(a.is_empty() && b.is_empty()) && a.gap(b) <= gap && !nested {
                    words.union(i, j);
                }
            }
        }
        let mut by_root: BTreeMap<usize, Rect> = BTreeMap::new();
        for (i, g) in glyphs.iter().enumerate() {
            by_root
                .entry(words.find(i))
                .and_modify(|word| *word = word.union(g))
                .or_insert(*g);
        }
        out.extend(by_root.into_values());
        out
    }

    /// Both passes give exactly what their definitions say, in the same order, whatever
    /// the boxes: dense or sparse, nested, repeated, empty, or at the end of the
    /// coordinate range (where geometry saturates).
    #[test]
    fn agrees_with_the_definitions_on_random_boxes() {
        let mut next = rng(0x2545_f491_4f6c_dd1d);
        for round in 0..600 {
            let inset = [0, 1, 3, 7, u32::MAX][round % 5];
            let gap = [0, 1, 3, 10, u32::MAX][round / 5 % 5];
            let max_side = [8, 48, u32::MAX][round / 25 % 3];
            let span = [12, 60, 400, 5000][round % 4];
            let size = [3, 12, 60][round / 4 % 3];
            let mut boxes: Vec<Rect> = Vec::new();
            for _ in 0..=next(80) {
                let pick = next(boxes.len().max(1) as u32) as usize;
                let b = match next(10) {
                    0 if !boxes.is_empty() => boxes[pick],
                    1 | 2 if !boxes.is_empty() => {
                        // Moves each edge in by up to one pixel more than the inset.
                        let mut edge = || next(inset.min(8) + 2);
                        let (l, t, r, d) = (edge(), edge(), edge(), edge());
                        let o = boxes[pick];
                        Rect::new(
                            o.x.saturating_add(l),
                            o.y.saturating_add(t),
                            o.w.saturating_sub(l + r),
                            o.h.saturating_sub(t + d),
                        )
                    }
                    3 => Rect::new(u32::MAX - next(90), next(span), next(size), next(size)),
                    4 => Rect::new(next(span), next(span), next(3), next(3)),
                    _ => Rect::new(next(span), next(span), 1 + next(size), 1 + next(size)),
                };
                boxes.push(b);
            }
            assert_eq!(
                drop_nested_duplicates(boxes.clone(), inset),
                drop_by_definition(boxes.clone(), inset),
                "inset {inset}: {boxes:?}"
            );
            assert_eq!(
                merge_glyphs(boxes.clone(), max_side, gap),
                merge_by_definition(boxes.clone(), max_side, gap),
                "max side {max_side}, gap {gap}: {boxes:?}"
            );
        }
    }

    #[test]
    fn the_grid_reports_each_box_meeting_an_area_once() {
        let mut next = rng(0x9e37_79b9_7f4a_7c15);
        for round in 0..300 {
            let span = [20, 300, 4000, u32::MAX][round % 4];
            let boxes: Vec<Rect> = (0..=next(100))
                .map(|_| Rect::new(next(span), next(span), next(40), next(40)))
                .collect();
            let grid = Grid::new(&boxes, CELL_SHIFT);
            for _ in 0..20 {
                let area = Rect::new(next(span), next(span), next(300), next(300));
                let from = next(boxes.len() as u32 + 1) as usize;
                let mut found = Vec::new();
                grid.for_each_intersecting(&area, from, |j| found.push(j));
                found.sort_unstable();
                let expected: Vec<usize> = (from..boxes.len())
                    .filter(|&j| boxes[j].intersects(&area))
                    .collect();
                assert_eq!(found, expected, "{area:?} from {from}: {boxes:?}");
            }
        }
    }

    #[test]
    fn a_border_and_its_fill_are_one_box() {
        let border = Rect::new(10, 10, 100, 30);
        let fill = Rect::new(11, 11, 98, 28);
        let label = Rect::new(20, 18, 30, 12);
        let thick = Rect::new(15, 15, 90, 20); // 5 px in: a separate element
        let kept = drop_nested_duplicates(vec![fill, label, border, thick], 3);
        assert_eq!(kept, vec![border, thick, label]);
    }

    #[test]
    fn glyphs_become_words_but_nested_and_large_boxes_stay_apart() {
        let word: Vec<Rect> = (0..4).map(|i| Rect::new(10 + i * 9, 10, 6, 12)).collect();
        let far = Rect::new(100, 10, 6, 12);
        let icon = Rect::new(200, 5, 24, 24);
        let dot_in_icon = Rect::new(208, 12, 6, 6);
        let button = Rect::new(10, 50, 80, 30);
        let mut boxes = word.clone();
        boxes.extend([far, icon, dot_in_icon, button]);
        let mut out = merge_glyphs(boxes, 48, 3);
        out.sort_by_key(|r| (r.y, r.x));
        assert_eq!(
            out,
            vec![icon, Rect::new(10, 10, 33, 12), far, dot_in_icon, button]
        );
    }
}
