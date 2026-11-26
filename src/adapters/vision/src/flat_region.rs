use ommatix_application::{PortError, VisionDetector};
use ommatix_domain::bbox::BBox;
use ommatix_domain::fusion::{Actionability, Affordance, Control, Role};
use ommatix_domain::incremental::BgraFrame;
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
    /// The surfaces of the last [`VisionDetector::detect`] call (ADR-0045).
    last_surfaces: Vec<BBox>,
}

impl FlatRegionDetector {
    pub fn new(config: FlatRegionConfig) -> Self {
        Self {
            config,
            last_surfaces: Vec::new(),
        }
    }

    pub fn config(&self) -> &FlatRegionConfig {
        &self.config
    }
}

impl FlatRegionDetector {
    /// How far, in pixels, grouping reaches: glyphs up to `merge_gap` apart are one word.
    pub fn context_margin(&self) -> u32 {
        self.config.merge_gap
    }

    /// Flat-colour blobs inside `region` of an RGBA frame (the prototype representation).
    pub fn detect_blobs(&self, frame: &Frame, region: Rect) -> Result<Vec<Detection>, PortError> {
        Ok(self
            .detect_parts(frame, region)?
            .boxes
            .into_iter()
            .map(|bounds| Detection {
                bounds,
                kind: ElementKind::Blob,
            })
            .collect())
    }

    /// The band of this detector's surfaces (ADR-0045): a duplicate it drops lies within
    /// `nested_inset` pixels of the box it repeats, so a change one pixel further in or out
    /// cannot touch the box or its duplicates.
    pub fn surface_band(&self) -> u32 {
        self.config.nested_inset.saturating_add(1)
    }

    /// The boxes of `region` and, among them, the surfaces and the group boxes.
    ///
    /// A box with a side above `glyph_max_side` passes through grouping unchanged, and its
    /// segment is reported only when it does not continue past the analysed area: such a box
    /// is reported whole or not at all, and only a change on its edge band (or one that
    /// splits it, L1) can change it. Those are the surfaces.
    fn detect_parts(&self, frame: &Frame, region: Rect) -> Result<Parts, PortError> {
        let area = region.clip(&frame.bounds());
        if area.is_empty() {
            return Ok(Parts::default());
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
        // `merge_glyphs` passes the large boxes through first, in order, and adds the groups:
        // the same boxes as grouping all of them at once.
        let (large, glyphs): (Vec<Rect>, Vec<Rect>) = boxes
            .into_iter()
            .partition(|r| r.w > c.glyph_max_side || r.h > c.glyph_max_side);
        let groups = merge_glyphs(glyphs, c.glyph_max_side, c.merge_gap);
        let mut boxes: Vec<Rect> = large.iter().chain(&groups).copied().collect();
        boxes.sort_by_key(|r| (r.y, r.x, r.w, r.h));
        Ok(Parts {
            boxes,
            surfaces: large,
            groups,
        })
    }
}

/// What one area of [`FlatRegionDetector::detect_parts`] found.
#[derive(Debug, Default)]
struct Parts {
    /// Every box, sorted.
    boxes: Vec<Rect>,
    /// The boxes that passed through grouping unchanged.
    surfaces: Vec<Rect>,
    /// The boxes grouping made from glyph-sized boxes.
    groups: Vec<Rect>,
}

/// What a blob is to the engine: a box with no role and no affordance evidence. The
/// detector cannot tell a button from a panel, so it never invents a role and never
/// confirms that anything can be acted on.
fn unknown_control(bbox: BBox) -> Control {
    Control {
        bbox,
        role: Role::Unknown,
        affordance: Affordance {
            actionability: Actionability::Unknown,
            enabled: true,
            visible: true,
        },
    }
}

/// The RGBA frame the segmenter works on.
fn to_rgba(frame: &BgraFrame) -> Result<Frame, PortError> {
    let (w, h) = (frame.width() as usize, frame.height() as usize);
    let mut rgba = Vec::with_capacity(4 * w * h);
    for row in frame.pixels().chunks(frame.row_stride()).take(h) {
        for px in row[..4 * w].as_chunks::<4>().0 {
            rgba.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
        }
    }
    Frame::from_rgba(frame.width(), frame.height(), rgba)
        .map_err(|e| PortError::new(NAME, "the frame cannot be converted").with_source(e))
}

/// The pixels a region covers, rounded outwards and clipped to the frame.
fn pixel_rect(region: &BBox, frame: &BgraFrame) -> Rect {
    let clamp = |v: f64, max: u32| v.clamp(0.0, f64::from(max)) as u32;
    let (x0, y0) = (
        clamp(region.x0().floor(), frame.width()),
        clamp(region.y0().floor(), frame.height()),
    );
    let (x1, y1) = (
        clamp(region.x1().ceil(), frame.width()),
        clamp(region.y1().ceil(), frame.height()),
    );
    Rect::new(x0, y0, x1.saturating_sub(x0), y1.saturating_sub(y0))
}

fn to_bbox(r: Rect) -> Option<BBox> {
    BBox::new(
        f64::from(r.x),
        f64::from(r.y),
        f64::from(r.right()),
        f64::from(r.bottom()),
    )
    .ok()
}

impl VisionDetector for FlatRegionDetector {
    fn name(&self) -> &'static str {
        NAME
    }

    fn models_loaded(&self) -> bool {
        false
    }

    /// Blobs inside each region, as controls of unknown role and actionability, sorted by
    /// box and without duplicates.
    fn detect(&mut self, frame: &BgraFrame, regions: &[BBox]) -> Result<Vec<Control>, PortError> {
        self.last_surfaces.clear();
        if regions.is_empty() {
            return Ok(Vec::new());
        }
        let rgba = to_rgba(frame)?;
        let (mut boxes, mut surfaces, mut groups) = (Vec::new(), Vec::new(), Vec::new());
        for region in regions {
            let parts = self.detect_parts(&rgba, pixel_rect(region, frame))?;
            boxes.extend(parts.boxes.into_iter().filter_map(to_bbox));
            surfaces.extend(parts.surfaces.into_iter().filter_map(to_bbox));
            groups.extend(parts.groups.into_iter().filter_map(to_bbox));
        }
        boxes.sort_by(BBox::cmp_corners);
        boxes.dedup();
        groups.sort_by(BBox::cmp_corners);
        // A surface equal to a group's box (of this area or another) is one control with
        // two readings: it is not declared, so the planner closes over it.
        surfaces.retain(|s| groups.binary_search_by(|g| g.cmp_corners(s)).is_err());
        surfaces.sort_by(BBox::cmp_corners);
        surfaces.dedup();
        self.last_surfaces = surfaces;
        Ok(boxes.into_iter().map(unknown_control).collect())
    }

    fn surface_band_px(&self) -> Option<u32> {
        Some(self.surface_band())
    }

    fn surfaces(&self) -> Vec<BBox> {
        self.last_surfaces.clone()
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
            .detect_blobs(frame, region)
            .unwrap()
            .into_iter()
            .map(|d| d.bounds)
            .collect()
    }

    #[test]
    fn as_a_vision_detector_it_reports_unknown_controls_in_the_regions() {
        let mut f = Frame::solid(100, 50, WHITE).unwrap();
        f.fill_rect(Rect::new(10, 10, 20, 10), BLUE);
        f.fill_rect(Rect::new(60, 10, 20, 10), BLUE);
        let mut bgra = f.as_rgba().to_vec();
        for p in bgra.as_chunks_mut::<4>().0 {
            p.swap(0, 2);
        }
        let frame = BgraFrame::new(100, 50, 400, 1, bgra).unwrap();
        let mut detector = FlatRegionDetector::default();
        assert!(!detector.models_loaded());
        let right = BBox::new(50.5, 0.0, 100.0, 50.0).unwrap();
        let found = detector.detect(&frame, &[right]).unwrap();
        assert_eq!(
            found,
            vec![unknown_control(BBox::new(60.0, 10.0, 80.0, 20.0).unwrap())]
        );
        assert_eq!(found[0].role, Role::Unknown);
        assert_eq!(found[0].affordance.actionability, Actionability::Unknown);
        let all = detector.detect(&frame, &[frame.bounds()]).unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(detector.detect(&frame, &[]).unwrap().len(), 0);
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

/// UT-86 (ADR-0045): the contract the flat-region detector declares with its surfaces, checked
/// with the real detector on generated frames.
#[cfg(test)]
mod surface_contract_tests {
    use super::*;

    const BACKDROP: [u8; 4] = [120, 140, 170, 255];

    fn bgra(f: &Frame) -> BgraFrame {
        let mut px = f.as_rgba().to_vec();
        for p in px.as_chunks_mut::<4>().0 {
            p.swap(0, 2);
        }
        BgraFrame::new(f.width(), f.height(), 4 * f.width() as usize, 1, px).unwrap()
    }

    fn bb(r: Rect) -> BBox {
        to_bbox(r).unwrap()
    }

    /// What one call reports: every box, and the surfaces among them.
    fn read(frame: &BgraFrame, area: BBox) -> (Vec<BBox>, Vec<BBox>) {
        let mut d = FlatRegionDetector::default();
        let boxes = d
            .detect(frame, &[area])
            .unwrap()
            .into_iter()
            .map(|c| c.bbox)
            .collect();
        (boxes, d.surfaces())
    }

    /// `inner` repeats `outer` as `drop_nested_duplicates` means it (every edge at most
    /// `inset` in).
    fn repeats(outer: &BBox, inner: &BBox, inset: f64) -> bool {
        outer.contains(inner)
            && inner.x0() - outer.x0() <= inset
            && inner.y0() - outer.y0() <= inset
            && outer.x1() - inner.x1() <= inset
            && outer.y1() - inner.y1() <= inset
    }

    /// xorshift64, as the grouping tests.
    fn rng(mut state: u64) -> impl FnMut(u32) -> u32 {
        move |m| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state % u64::from(m.max(1))) as u32
        }
    }

    /// A screen of windows (some with a 1 or 2 px border around their fill), buttons and
    /// glyph rows on a contrasting backdrop.
    fn screen(next: &mut impl FnMut(u32) -> u32) -> Frame {
        let (w, h) = (360, 240);
        let mut f = Frame::solid(w, h, BACKDROP).unwrap();
        for i in 0..=next(3) {
            let (ww, wh) = (60 + next(180), 50 + next(120));
            let (x, y) = (2 + next(w - ww - 4), 2 + next(h - wh - 4));
            let shade = 200 + 15 * i as u8;
            f.fill_rect(Rect::new(x, y, ww, wh), [shade, shade, shade, 255]);
            if next(2) == 0 {
                let b = 1 + next(2);
                f.fill_rect(
                    Rect::new(x + b, y + b, ww - 2 * b, wh - 2 * b),
                    [250, 250, 250 - 10 * i as u8, 255],
                );
            }
            for _ in 0..next(3) {
                let (bw, bh) = (12 + next(40), 8 + next(16));
                let (bx, by) = (x + 4 + next(ww - bw - 8), y + 4 + next(wh - bh - 8));
                f.fill_rect(Rect::new(bx, by, bw, bh), [30, 90, 200, 255]);
            }
            for g in 0..next(6) {
                f.fill_rect(Rect::new(x + 6 + g * 6, y + 8, 4, 7), [0, 0, 0, 255]);
            }
        }
        f
    }

    /// (a) The surfaces are boxes the call returned, with a side above `glyph_max_side`, and
    /// none is a group's box. (b) An area's surfaces are boxes of the full read lying in the
    /// area, unless the area holds a fill whose border it cuts: the fill then repeats a box
    /// of the full read the area does not contain, and the area covers that box's inner box,
    /// which makes the planner close over it (rule R2 (v)).
    #[test]
    fn ut_86_surfaces_are_returned_boxes_and_crops_report_them_whole_or_not_at_all() {
        let mut next = rng(0x5EED_0086);
        let c = FlatRegionConfig::default();
        let band = f64::from(c.nested_inset + 1);
        let (mut checked, mut surfaces_seen, mut exceptions) = (0, 0, 0);
        for _ in 0..120 {
            let f = screen(&mut next);
            let frame = bgra(&f);
            let (full_boxes, full_surfaces) = read(&frame, frame.bounds());
            for s in &full_surfaces {
                assert!(full_boxes.contains(s));
                assert!(s.width() > f64::from(c.glyph_max_side) || s.height() > 48.0);
            }
            for _ in 0..8 {
                let (aw, ah) = (8 + next(250), 8 + next(200));
                let (ax, ay) = (next(360 - aw), next(240 - ah));
                let area = bb(Rect::new(ax, ay, aw, ah));
                let (boxes, surfaces) = read(&frame, area);
                for s in &surfaces {
                    surfaces_seen += 1;
                    assert!(boxes.contains(s), "a surface is a returned box");
                    assert!(area.contains(s), "a surface lies in its area");
                    if full_boxes.contains(s) {
                        continue;
                    }
                    let outer = full_boxes
                        .iter()
                        .find(|o| repeats(o, s, f64::from(c.nested_inset)))
                        .unwrap_or_else(|| panic!("{s:?} in {area:?} is no full-read box"));
                    assert!(!area.contains(outer));
                    let inner = BBox::new(
                        outer.x0() + band,
                        outer.y0() + band,
                        outer.x1() - band,
                        outer.y1() - band,
                    )
                    .unwrap();
                    assert!(area.contains(&inner), "R2 (v) closes over {outer:?}");
                    exceptions += 1;
                }
                checked += 1;
            }
        }
        assert_eq!(checked, 960);
        assert!(surfaces_seen > 40, "only {surfaces_seen} surfaces seen");
        eprintln!(
            "UT-86: {surfaces_seen} crop surfaces, {exceptions} fills whose border the area cut"
        );
    }

    /// (a) A surface never equals a group's box: a row of glyphs as wide as a window's fill.
    #[test]
    fn ut_86_a_box_that_is_also_a_group_is_not_a_surface() {
        let mut f = Frame::solid(200, 100, BACKDROP).unwrap();
        // Glyphs grouped into one word 60 px wide (wider than `glyph_max_side`).
        for g in 0..10 {
            f.fill_rect(Rect::new(20 + g * 6, 20, 4, 7), [0, 0, 0, 255]);
        }
        f.fill_rect(Rect::new(100, 20, 60, 60), [250, 250, 250, 255]);
        let frame = bgra(&f);
        let (boxes, surfaces) = read(&frame, frame.bounds());
        let word = bb(Rect::new(20, 20, 58, 7));
        assert!(boxes.contains(&word));
        assert!(!surfaces.contains(&word));
        assert_eq!(surfaces, vec![bb(Rect::new(100, 20, 60, 60))]);
    }

    /// (c) A filled rectangle drawn inside `s.shrink(band)` of a solid surface leaves the
    /// surface's box in a full read unchanged.
    #[test]
    fn ut_86_a_change_inside_the_inner_box_leaves_the_surface_box_unchanged() {
        let mut next = rng(0x86_0002);
        let band = FlatRegionDetector::default().surface_band();
        let s = Rect::new(50, 40, 200, 120);
        for _ in 0..200 {
            let mut f = Frame::solid(320, 200, BACKDROP).unwrap();
            f.fill_rect(s, [240, 240, 240, 255]);
            let before = bgra(&f);
            let (w, h) = (1 + next(60), 1 + next(40));
            let x = s.x + band + next(s.w - 2 * band - w);
            let y = s.y + band + next(s.h - 2 * band - h);
            f.fill_rect(Rect::new(x, y, w, h), [next(255) as u8, 20, 20, 255]);
            let after = bgra(&f);
            let (b0, s0) = read(&before, before.bounds());
            let (b1, s1) = read(&after, after.bounds());
            assert!(b0.contains(&bb(s)) && s0.contains(&bb(s)));
            assert!(
                b1.contains(&bb(s)) && s1.contains(&bb(s)),
                "{x},{y} {w}x{h}"
            );
        }
    }

    /// (d) L1, the stated limit: a bar inside the inner box that cuts the only link between
    /// two parts of a surface splits it, and the full read reports two boxes where it
    /// reported one. The band is unchanged, so the planner keeps the old box until its band
    /// changes or a full refresh.
    #[test]
    fn ut_86_l1_a_change_that_splits_a_surface_changes_the_full_read() {
        let mut f = Frame::solid(320, 200, BACKDROP).unwrap();
        let grey = [240, 240, 240, 255];
        f.fill_rect(Rect::new(50, 40, 70, 120), grey); // left block
        f.fill_rect(Rect::new(180, 40, 70, 120), grey); // right block
        f.fill_rect(Rect::new(120, 95, 60, 10), grey); // the bridge
        let whole = bb(Rect::new(50, 40, 200, 120));
        let before = bgra(&f);
        let (_, surfaces) = read(&before, before.bounds());
        assert_eq!(surfaces, vec![whole]);
        // The bar lies inside `whole.shrink(band)`: no pixel of the band changes.
        f.fill_rect(Rect::new(140, 90, 10, 20), [20, 20, 20, 255]);
        let after = bgra(&f);
        let (boxes, surfaces) = read(&after, after.bounds());
        assert!(!boxes.contains(&whole), "the split is seen by a full read");
        // Each part keeps the piece of the bridge on its side of the bar.
        assert!(
            surfaces.contains(&bb(Rect::new(50, 40, 90, 120))),
            "{surfaces:?}"
        );
        assert!(
            surfaces.contains(&bb(Rect::new(150, 40, 100, 120))),
            "{surfaces:?}"
        );
    }

    /// The detector declares its band, and an empty call declares no surface.
    #[test]
    fn ut_86_the_band_is_declared_and_an_empty_call_has_no_surfaces() {
        let mut d = FlatRegionDetector::default();
        assert_eq!(d.surface_band_px(), Some(4));
        let f = bgra(&Frame::solid(40, 40, BACKDROP).unwrap());
        d.detect(&f, &[]).unwrap();
        assert_eq!(d.surfaces(), Vec::<BBox>::new());
    }
}
