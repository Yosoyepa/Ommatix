//! Pure geometry of the OCR adapters: OCR engines report word rectangles relative to the
//! image they were given (a tile of a region's crop); the engine needs them in frame
//! coordinates, clipped to the region, in a deterministic order.

use ommatix_domain::bbox::BBox;
use ommatix_domain::fusion::Word;

use crate::crop::PixelRect;

/// A word rectangle relative to `tile` (`x, y, width, height`, as OCR engines report it),
/// in frame coordinates and clipped to `clip`. `None` for a non-finite or empty rectangle,
/// or one with no area inside `clip`.
pub fn word_box(rect: [f64; 4], tile: PixelRect, clip: &BBox) -> Option<BBox> {
    let [x, y, w, h] = rect;
    let (ox, oy) = (f64::from(tile.x), f64::from(tile.y));
    BBox::new(ox + x, oy + y, ox + x + w, oy + y + h)
        .ok()?
        .clip(clip)
}

/// Sorts words by box, then text, and drops exact duplicates (a word seen through two
/// overlapping regions).
pub fn sort_words(words: &mut Vec<Word>) {
    words.sort_by(|a, b| {
        a.bbox
            .cmp_corners(&b.bbox)
            .then_with(|| a.text.cmp(&b.text))
    });
    words.dedup();
}

#[cfg(test)]
mod tests {
    use super::*;

    const TILE: PixelRect = PixelRect {
        x: 100,
        y: 40,
        width: 50,
        height: 20,
    };

    fn clip() -> BBox {
        BBox::new(100.5, 40.0, 150.0, 60.0).unwrap()
    }

    #[test]
    fn word_rectangles_are_offset_by_the_tile_origin() {
        let b = word_box([2.0, 3.0, 10.0, 8.0], TILE, &clip()).unwrap();
        assert_eq!(b, BBox::new(102.0, 43.0, 112.0, 51.0).unwrap());
    }

    #[test]
    fn word_rectangles_are_clipped_to_the_region() {
        let b = word_box([0.0, 15.0, 60.0, 10.0], TILE, &clip()).unwrap();
        assert_eq!(b, BBox::new(100.5, 55.0, 150.0, 60.0).unwrap());
    }

    #[test]
    fn degenerate_rectangles_are_dropped() {
        for rect in [
            [0.0, 0.0, 0.0, 5.0],
            [0.0, 0.0, 5.0, -1.0],
            [f64::NAN, 0.0, 5.0, 5.0],
            [0.0, 0.0, f64::INFINITY, 5.0],
            [60.0, 0.0, 5.0, 5.0], // outside the clip
        ] {
            assert_eq!(word_box(rect, TILE, &clip()), None, "{rect:?}");
        }
    }

    #[test]
    fn words_are_sorted_and_deduplicated() {
        let w = |t: &str, x: f64| Word::new(t, BBox::new(x, 0.0, x + 5.0, 5.0).unwrap());
        let mut words = vec![w("b", 10.0), w("a", 0.0), w("b", 10.0), w("a", 10.0)];
        sort_words(&mut words);
        assert_eq!(words, vec![w("a", 0.0), w("a", 10.0), w("b", 10.0)]);
    }
}
