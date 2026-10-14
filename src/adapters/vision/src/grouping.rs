//! Turns segment boxes into element boxes: drops boxes that repeat a bigger one and
//! groups glyphs into words.

use std::cmp::Reverse;
use std::collections::BTreeMap;
use std::num::NonZeroU32;

use ommatix_domain::{ElementId, Rect, SpatialIndex};

use crate::forest::Forest;

const BUCKET: NonZeroU32 = NonZeroU32::new(64).expect("non-zero bucket");

/// Drops every box that lies inside a bigger box with all four edges at most `inset`
/// pixels in: a fill inside its own border, the hole of an "o". The bigger box is kept.
pub(crate) fn drop_nested_duplicates(mut boxes: Vec<Rect>, inset: u32) -> Vec<Rect> {
    // Biggest first, so each box is only compared with bigger boxes already kept.
    boxes.sort_by_key(|r| (Reverse(r.area()), r.y, r.x, r.w, r.h));
    let mut kept: Vec<Rect> = Vec::with_capacity(boxes.len());
    let mut index = SpatialIndex::new(BUCKET);
    for b in boxes {
        let duplicate = index
            .query_point(b.center())
            .into_iter()
            .any(|id| repeats(&kept[id.0 as usize], &b, inset));
        if !duplicate {
            index.upsert(ElementId(kept.len() as u64), b);
            kept.push(b);
        }
    }
    kept
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
    let mut index = SpatialIndex::new(BUCKET);
    for (i, g) in glyphs.iter().enumerate() {
        index.upsert(ElementId(i as u64), *g);
    }
    let mut words = Forest::new(glyphs.len());
    for (i, g) in glyphs.iter().enumerate() {
        // Inflating by gap + 1 finds every box whose Chebyshev gap is <= gap.
        for id in index.query_rect(&g.inflate(gap.saturating_add(1))) {
            let other = &glyphs[id.0 as usize];
            let nested = g.contains_rect(other) || other.contains_rect(g);
            if g.gap(other) <= gap && !nested {
                words.union(i, id.0 as usize);
            }
        }
    }
    let mut groups: BTreeMap<usize, Rect> = BTreeMap::new();
    for (i, g) in glyphs.iter().enumerate() {
        groups
            .entry(words.find(i))
            .and_modify(|word| *word = word.union(g))
            .or_insert(*g);
    }
    out.extend(groups.into_values());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

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
