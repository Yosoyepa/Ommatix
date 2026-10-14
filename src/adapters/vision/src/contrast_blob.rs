use std::collections::BTreeMap;

use ommatix_application::ElementDetector;
use ommatix_domain::{Detection, ElementKind, Frame, Rect};

/// Tunables of [`ContrastBlobDetector`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContrastBlobConfig {
    /// Max per-channel RGB difference still considered background.
    pub tolerance: u8,
    /// Components with fewer foreground pixels are discarded as noise.
    pub min_pixels: u32,
    /// Boxes closer than this many pixels are merged (glyphs into words, icon + label).
    pub merge_gap: u32,
}

impl Default for ContrastBlobConfig {
    fn default() -> Self {
        Self {
            tolerance: 24,
            min_pixels: 4,
            merge_gap: 3,
        }
    }
}

/// Finds elements as groups of pixels that contrast with the region's dominant colour.
///
/// 1. Background = most frequent colour in the region (ties broken by colour value).
/// 2. Foreground mask = pixels whose max RGB channel distance exceeds `tolerance`.
/// 3. 8-connected components → bounding boxes, small ones dropped.
/// 4. Nearby, non-nested boxes merged to a fixpoint.
///
/// Known MVP limitation: an element that fills its whole region becomes the background
/// and is not reported.
#[derive(Debug, Clone, Default)]
pub struct ContrastBlobDetector {
    config: ContrastBlobConfig,
}

impl ContrastBlobDetector {
    pub fn new(config: ContrastBlobConfig) -> Self {
        Self { config }
    }
}

impl ElementDetector for ContrastBlobDetector {
    fn name(&self) -> &'static str {
        "contrast-blob"
    }

    fn detect(&self, frame: &Frame, region: Rect) -> Vec<Detection> {
        let region = region.clip(&frame.bounds());
        if region.is_empty() {
            return Vec::new();
        }
        let bg = dominant_colour(frame, region);
        let (w, h) = (region.w as usize, region.h as usize);
        let mask: Vec<bool> = (0..h)
            .flat_map(|dy| (0..w).map(move |dx| (dx, dy)))
            .map(|(dx, dy)| {
                let px = frame.pixel(region.x + dx as u32, region.y + dy as u32);
                distance(px, bg) > self.config.tolerance
            })
            .collect();

        let boxes = components(&mask, w, h)
            .into_iter()
            .filter(|(_, n)| *n >= self.config.min_pixels)
            .map(|(r, _)| Rect::new(r.x + region.x, r.y + region.y, r.w, r.h))
            .collect();

        let mut merged = merge_close(boxes, self.config.merge_gap);
        merged.sort_by_key(|r| (r.y, r.x, r.w, r.h));
        merged
            .into_iter()
            .map(|bounds| Detection {
                bounds,
                kind: ElementKind::Blob,
            })
            .collect()
    }
}

fn dominant_colour(frame: &Frame, region: Rect) -> [u8; 4] {
    let mut counts: BTreeMap<[u8; 4], u32> = BTreeMap::new();
    for y in region.y..region.bottom() {
        for px in frame.row_slice(region.x, y, region.w).as_chunks::<4>().0 {
            *counts.entry(*px).or_default() += 1;
        }
    }
    // BTreeMap iterates in colour order, so `max_by_key` keeps the last max:
    // deterministic tie-break by highest colour value.
    counts
        .into_iter()
        .max_by_key(|(_, n)| *n)
        .map_or([0; 4], |(c, _)| c)
}

fn distance(a: [u8; 4], b: [u8; 4]) -> u8 {
    (0..3).map(|i| a[i].abs_diff(b[i])).max().unwrap_or(0)
}

/// 8-connected components of `mask`; returns (bounding box, pixel count) in scan order.
fn components(mask: &[bool], w: usize, h: usize) -> Vec<(Rect, u32)> {
    let mut seen = vec![false; mask.len()];
    let mut out = Vec::new();
    let mut stack = Vec::new();
    for start in 0..mask.len() {
        if !mask[start] || seen[start] {
            continue;
        }
        seen[start] = true;
        stack.push(start);
        let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
        let mut count = 0u32;
        while let Some(i) = stack.pop() {
            let (x, y) = (i % w, i / w);
            count += 1;
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
            for ny in y.saturating_sub(1)..=(y + 1).min(h - 1) {
                for nx in x.saturating_sub(1)..=(x + 1).min(w - 1) {
                    let j = ny * w + nx;
                    if mask[j] && !seen[j] {
                        seen[j] = true;
                        stack.push(j);
                    }
                }
            }
        }
        let rect = Rect::new(
            x0 as u32,
            y0 as u32,
            (x1 - x0 + 1) as u32,
            (y1 - y0 + 1) as u32,
        );
        out.push((rect, count));
    }
    out
}

fn merge_close(mut boxes: Vec<Rect>, gap: u32) -> Vec<Rect> {
    loop {
        let mut merged = false;
        'outer: for i in 0..boxes.len() {
            for j in (i + 1)..boxes.len() {
                let (a, b) = (boxes[i], boxes[j]);
                // Nested boxes (a button inside a dialog frame) stay separate elements.
                let nested = a.union(&b) == a || a.union(&b) == b;
                if !nested && a.gap(&b) <= gap {
                    boxes[i] = boxes[i].union(&boxes[j]);
                    boxes.swap_remove(j);
                    merged = true;
                    break 'outer;
                }
            }
        }
        if !merged {
            return boxes;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WHITE: [u8; 4] = [255, 255, 255, 255];
    const BLUE: [u8; 4] = [30, 90, 200, 255];

    fn detect(frame: &Frame, region: Rect) -> Vec<Rect> {
        ContrastBlobDetector::default()
            .detect(frame, region)
            .into_iter()
            .map(|d| d.bounds)
            .collect()
    }

    #[test]
    fn finds_separate_buttons() {
        let mut f = Frame::solid(100, 50, WHITE).unwrap();
        f.fill_rect(Rect::new(10, 10, 20, 10), BLUE);
        f.fill_rect(Rect::new(60, 10, 20, 10), BLUE);
        assert_eq!(
            detect(&f, f.bounds()),
            vec![Rect::new(10, 10, 20, 10), Rect::new(60, 10, 20, 10)]
        );
    }

    #[test]
    fn merges_glyph_like_blobs_and_drops_noise() {
        let mut f = Frame::solid(60, 20, WHITE).unwrap();
        for i in 0..4 {
            f.fill_rect(Rect::new(5 + i * 5, 5, 3, 7), [0, 0, 0, 255]); // "glyphs", 2px apart
        }
        f.fill_rect(Rect::new(50, 15, 1, 1), [0, 0, 0, 255]); // single-pixel noise
        assert_eq!(detect(&f, f.bounds()), vec![Rect::new(5, 5, 18, 7)]);
    }

    #[test]
    fn respects_region_and_is_deterministic() {
        let mut f = Frame::solid(100, 100, WHITE).unwrap();
        f.fill_rect(Rect::new(10, 10, 10, 10), BLUE);
        f.fill_rect(Rect::new(70, 70, 10, 10), BLUE);
        let region = Rect::new(50, 50, 50, 50);
        let a = detect(&f, region);
        assert_eq!(a, vec![Rect::new(70, 70, 10, 10)]);
        assert_eq!(a, detect(&f, region));
    }

    #[test]
    fn nested_elements_are_kept_apart() {
        let mut f = Frame::solid(100, 100, WHITE).unwrap();
        f.fill_rect(Rect::new(10, 10, 80, 80), BLUE); // dialog frame
        f.fill_rect(Rect::new(12, 12, 76, 76), WHITE);
        f.fill_rect(Rect::new(40, 60, 20, 10), BLUE); // OK button
        assert_eq!(
            detect(&f, f.bounds()),
            vec![Rect::new(10, 10, 80, 80), Rect::new(40, 60, 20, 10)]
        );
    }

    #[test]
    fn low_contrast_is_background() {
        let mut f = Frame::solid(20, 20, WHITE).unwrap();
        f.fill_rect(Rect::new(5, 5, 5, 5), [245, 245, 245, 255]);
        assert_eq!(detect(&f, f.bounds()).len(), 0);
    }
}
