use ommatix_application::{ElementDetector, PortError};
use ommatix_domain::{Detection, ElementKind, Frame, Rect};

use crate::border::FrameBorder;
use crate::grouping::{drop_nested_duplicates, merge_glyphs};
use crate::segment::{Segment, segments};

const NAME: &str = "flat-region";

/// Tunables of [`FlatRegionDetector`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlatRegionConfig {
    /// Max per-channel difference between neighbouring pixels of one flat region.
    pub tolerance: u8,
    /// Regions with fewer pixels are noise (anti-aliasing specks, dust).
    pub min_pixels: u32,
    /// A box inside a bigger one with every edge at most this many pixels in is the same
    /// element (a border and its fill).
    pub nested_inset: u32,
    /// Glyph-sized boxes at most this many pixels apart are grouped (glyphs into words).
    pub merge_gap: u32,
    /// Largest side of a glyph-sized box.
    pub glyph_max_side: u32,
}

impl Default for FlatRegionConfig {
    fn default() -> Self {
        Self {
            tolerance: 24,
            min_pixels: 4,
            nested_inset: 3,
            merge_gap: 3,
            glyph_max_side: 48,
        }
    }
}

/// Finds elements as flat-colour regions, the way GUIs are drawn (bootstrap ADR B0010).
///
/// 1. **Segment**: grow 8-connected regions of pixels whose neighbours differ by at most
///    `tolerance` (union-find, near-linear in the area).
/// 2. **Background**: a region is background when it blends into the pixels just
///    outside the analysed area (it is the backdrop, or a container around the area), or
///    when it reaches the frame's own border on a flat stretch covering at least half of
///    a screen edge, or has the colour of such a stretch: the wallpaper, a maximised or
///    snapped window, a taskbar, a dock. Stretches are measured on the whole frame border,
///    so the answer does not depend on the analysed area.
/// 3. **Group**: drop noise, drop boxes that merely repeat a bigger box, and group
///    glyph-sized boxes into words.
///
/// Every remaining box is an element ([`ElementKind::Blob`]). No colour is assumed to be
/// the background, so light themes, dark themes and wallpapers that contrast with their
/// windows all work, and a region that is mostly one surface yields no phantom element.
#[derive(Debug, Clone, Default)]
pub struct FlatRegionDetector {
    config: FlatRegionConfig,
}

impl FlatRegionDetector {
    pub fn new(config: FlatRegionConfig) -> Self {
        Self { config }
    }

    pub fn config(&self) -> &FlatRegionConfig {
        &self.config
    }
}

impl ElementDetector for FlatRegionDetector {
    fn name(&self) -> &'static str {
        NAME
    }

    /// Glyphs up to `merge_gap` apart are grouped into one word.
    fn context_margin(&self) -> u32 {
        self.config.merge_gap
    }

    fn detect(&self, frame: &Frame, region: Rect) -> Result<Vec<Detection>, PortError> {
        let area = region.clip(&frame.bounds());
        if area.is_empty() {
            return Ok(Vec::new());
        }
        if area.area() > u64::from(u32::MAX) {
            return Err(PortError::new(
                NAME,
                format!("region of {} pixels is too large", area.area()),
            ));
        }
        let c = &self.config;
        let border = FrameBorder::new(frame, c.tolerance);
        let boxes = segments(frame, area, c.tolerance, &border)
            .iter()
            .filter(|s| s.pixels >= c.min_pixels && !is_background(s, &border, c.tolerance))
            .map(|s| s.bounds)
            .collect();
        let boxes = drop_nested_duplicates(boxes, c.nested_inset);
        let mut boxes = merge_glyphs(boxes, c.glyph_max_side, c.merge_gap);
        boxes.sort_by_key(|r| (r.y, r.x, r.w, r.h));
        Ok(boxes
            .into_iter()
            .map(|bounds| Detection {
                bounds,
                kind: ElementKind::Blob,
            })
            .collect())
    }
}

fn is_background(s: &Segment, border: &FrameBorder, tolerance: u8) -> bool {
    s.continues_outside
        || s.on_border_surface
        || (s.touches_frame_border && border.has_surface_colour(s.mean, tolerance))
}

#[cfg(test)]
mod tests {
    use super::*;

    const WHITE: [u8; 4] = [255, 255, 255, 255];
    const BLUE: [u8; 4] = [30, 90, 200, 255];
    const INK: [u8; 4] = [0, 0, 0, 255];

    fn detect(frame: &Frame, region: Rect) -> Vec<Rect> {
        FlatRegionDetector::default()
            .detect(frame, region)
            .unwrap()
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
    fn groups_glyphs_into_words_and_drops_noise() {
        let mut f = Frame::solid(60, 20, WHITE).unwrap();
        for i in 0..4 {
            f.fill_rect(Rect::new(5 + i * 5, 5, 3, 7), INK); // glyphs 2 px apart
        }
        f.fill_rect(Rect::new(50, 15, 1, 1), INK); // single-pixel noise
        assert_eq!(detect(&f, f.bounds()), vec![Rect::new(5, 5, 18, 7)]);
    }

    #[test]
    fn respects_the_region_and_is_deterministic() {
        let mut f = Frame::solid(100, 100, WHITE).unwrap();
        f.fill_rect(Rect::new(10, 10, 10, 10), BLUE);
        f.fill_rect(Rect::new(70, 70, 10, 10), BLUE);
        let region = Rect::new(50, 50, 50, 50);
        let a = detect(&f, region);
        assert_eq!(a, vec![Rect::new(70, 70, 10, 10)]);
        assert_eq!(a, detect(&f, region));
    }

    #[test]
    fn a_frame_and_its_contents_are_separate_elements() {
        let mut f = Frame::solid(100, 100, WHITE).unwrap();
        f.fill_rect(Rect::new(10, 10, 80, 80), BLUE); // dialog border
        f.fill_rect(Rect::new(12, 12, 76, 76), WHITE); // its fill: same element
        f.fill_rect(Rect::new(40, 60, 20, 10), BLUE); // OK button
        assert_eq!(
            detect(&f, f.bounds()),
            vec![Rect::new(10, 10, 80, 80), Rect::new(40, 60, 20, 10)]
        );
    }

    #[test]
    fn low_contrast_and_smooth_gradients_are_background() {
        let mut f = Frame::solid(20, 20, WHITE).unwrap();
        f.fill_rect(Rect::new(5, 5, 5, 5), [245, 245, 245, 255]);
        assert_eq!(detect(&f, f.bounds()).len(), 0);

        let gradient: Vec<u8> = (0..200u32)
            .flat_map(|_| (0..256u32).flat_map(|x| [x as u8, x as u8, 255, 255]))
            .collect();
        let g = Frame::from_rgba(256, 200, gradient).unwrap();
        assert_eq!(detect(&g, g.bounds()).len(), 0, "no phantom element");
    }

    #[test]
    fn a_window_dominating_a_contrasting_wallpaper_is_an_element() {
        let mut f = Frame::solid(640, 400, [120, 140, 170, 255]).unwrap();
        f.fill_rect(Rect::new(40, 40, 560, 320), WHITE); // covers most of the screen
        f.fill_rect(Rect::new(500, 300, 80, 32), BLUE);
        assert_eq!(
            detect(&f, f.bounds()),
            vec![Rect::new(40, 40, 560, 320), Rect::new(500, 300, 80, 32)]
        );
    }

    #[test]
    fn a_container_around_the_region_is_not_reported() {
        let mut f = Frame::solid(256, 256, WHITE).unwrap();
        f.fill_rect(Rect::new(32, 32, 192, 192), BLUE); // panel
        f.fill_rect(Rect::new(110, 110, 6, 12), WHITE); // a glyph on it
        assert_eq!(
            detect(&f, Rect::new(96, 96, 32, 32)),
            vec![Rect::new(110, 110, 6, 12)]
        );
    }

    #[test]
    fn dark_themes_work_like_light_ones() {
        let mut f = Frame::solid(200, 100, [24, 26, 31, 255]).unwrap();
        f.fill_rect(Rect::new(20, 20, 160, 60), [40, 42, 48, 255]); // window, low contrast
        for i in 0..3 {
            f.fill_rect(Rect::new(30 + i * 9, 30, 6, 12), [230, 230, 230, 255]);
        }
        f.fill_rect(Rect::new(120, 50, 50, 20), [94, 129, 172, 255]);
        assert_eq!(
            detect(&f, f.bounds()),
            vec![Rect::new(30, 30, 24, 12), Rect::new(120, 50, 50, 20)]
        );
    }

    #[test]
    fn an_empty_or_outside_region_finds_nothing() {
        let f = Frame::solid(10, 10, WHITE).unwrap();
        assert_eq!(detect(&f, Rect::new(50, 50, 5, 5)).len(), 0);
        assert_eq!(detect(&f, Rect::new(2, 2, 0, 5)).len(), 0);
    }
}
