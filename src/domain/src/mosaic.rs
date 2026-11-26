//! The geometry of a mosaic of regions (ADR-0041, candidate A, "mosaic" variant).
//!
//! The mosaic puts the regions of one plan side by side on one canvas, so that one detector
//! inference could replace one inference per region. Between two tiles, and between a tile
//! and the canvas edge, there is a margin, so that a detection cannot be attributed to the
//! wrong region.
//!
//! **This is only geometry, and the candidate it serves has not been measured.** ADR-0041 is
//! Proposed and its rule for adopting the mosaic needs measurements nobody has taken. Nothing
//! calls this module: it packs no pixels, runs no detector and changes no behaviour. If the
//! candidate fails its rule, this module is removed with it. A caller that gets no layout
//! keeps the per-region path, which is the explicit fallback Algorithm 1 requires.
//!
//! ## Coordinates
//!
//! - *Frame* coordinates are those of the plan's region boxes ([`BBox`], physical pixels).
//! - *Canvas* coordinates are those of the mosaic canvas, whose size the caller chooses.
//! - A tile keeps the size of its region: **there is no scaling here.** Scaling the canvas to
//!   the detector's input size is the adapter's concern.
//!
//! ## Planning rule ([`plan_mosaic`])
//!
//! 1. Each region becomes a whole-pixel *source rectangle*: its lower corner is rounded down
//!    and its upper corner rounded up, so the rectangle covers the region and is the smallest
//!    one that does. A region with a negative corner, or one that does not fit the `u32`
//!    pixel range, is refused: frame coordinates start at zero.
//! 2. The tiles are packed in *shelves*, in the plan's order, with no sorting. A cursor starts
//!    at `(margin, margin)`. A tile goes at the cursor and the cursor moves right by the tile's
//!    width plus the margin. A tile that would end less than a margin short of the canvas's
//!    right edge starts a new shelf instead: `x` returns to the margin and `y` moves down by
//!    the tallest tile of the shelf plus the margin. A tile that fits on no shelf makes the
//!    plan fail. The layout depends only on the arguments, so the same regions in the same
//!    order always give the same layout. It is not optimal; a plan that a cleverer packing
//!    could place may be refused.
//!
//! ## Mapping a detection
//!
//! A detection in canvas coordinates belongs to a tile only if it lies entirely inside the
//! tile, edges included. Boxes are half-open, as everywhere in the crate, so a box that ends
//! exactly on a tile's edge does not enter the margin; any box that does enter a margin, even
//! by a fraction of a pixel, crosses an edge, straddles two tiles or lies outside every tile,
//! and is discarded. The frame box is the canvas box moved by the tile's whole-pixel offset,
//! with no rounding of its own, so mapping a box forward and back is exact whenever the
//! additions are (for corners with few fractional bits, as the tests use).

use thiserror::Error;

use crate::Rect;
use crate::bbox::BBox;

/// Why no layout was returned. In every case the caller keeps the per-region path.
#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum MosaicError {
    #[error("the plan has no regions")]
    NoRegions,
    #[error("the margin must be at least one pixel")]
    ZeroMargin,
    #[error("the canvas has no area")]
    EmptyCanvas,
    #[error("region {region} has a negative corner or does not fit the pixel range")]
    RegionOutOfRange { region: usize },
    #[error("region {region} does not fit on the canvas with the margin")]
    DoesNotFit { region: usize },
}

/// One region's place on the canvas: the same whole-pixel size in both coordinate systems.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tile {
    frame: Rect,
    canvas: Rect,
}

impl Tile {
    /// The region rounded outward to whole pixels, in frame coordinates.
    pub const fn frame(&self) -> Rect {
        self.frame
    }

    /// Where the region's pixels go, in canvas coordinates.
    pub const fn canvas(&self) -> Rect {
        self.canvas
    }

    /// The offset that moves a canvas coordinate to a frame one, as `(dx, dy)`.
    fn offset(&self) -> (f64, f64) {
        (
            f64::from(self.frame.x) - f64::from(self.canvas.x),
            f64::from(self.frame.y) - f64::from(self.canvas.y),
        )
    }
}

/// A placement of every region of a plan on one canvas (ADR-0041, candidate A).
///
/// `tiles()[i]` belongs to the plan's region `i`; no region is ever left out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MosaicLayout {
    width: u32,
    height: u32,
    margin: u32,
    tiles: Vec<Tile>,
}

impl MosaicLayout {
    /// The canvas size, `(width, height)`.
    pub const fn canvas_size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// The clear space kept between tiles and around them, in pixels.
    pub const fn margin(&self) -> u32 {
        self.margin
    }

    /// One tile per region, in the plan's order.
    pub fn tiles(&self) -> &[Tile] {
        &self.tiles
    }

    /// Maps a detection from canvas coordinates back to the frame.
    ///
    /// Returns the index of the region the detection belongs to and its box in frame
    /// coordinates, or `None` when it must be discarded: it lies outside every tile, enters a
    /// margin, crosses a tile's edge or straddles two tiles. A [`BBox`] is finite and has an
    /// area by construction, so a detection without one never reaches this function; the
    /// caller discards it when it fails to build the box.
    pub fn to_frame(&self, detection: &BBox) -> Option<(usize, BBox)> {
        let (index, tile) = self
            .tiles
            .iter()
            .enumerate()
            .find(|(_, tile)| lies_in(&tile.canvas, detection))?;
        let (dx, dy) = tile.offset();
        Some((index, shifted(detection, dx, dy)?))
    }

    /// Maps a box from frame coordinates into the tile of `region`, in canvas coordinates.
    ///
    /// Returns `None` when `region` is not in the plan or the box does not lie entirely inside
    /// the region's whole-pixel source rectangle ([`Tile::frame`]).
    pub fn to_canvas(&self, region: usize, bbox: &BBox) -> Option<BBox> {
        let tile = self.tiles.get(region)?;
        if !lies_in(&tile.frame, bbox) {
            return None;
        }
        let (dx, dy) = tile.offset();
        shifted(bbox, -dx, -dy)
    }
}

/// Plans where the regions of one plan go on a canvas of `canvas_width` × `canvas_height`
/// pixels, with at least `margin` pixels between tiles and between a tile and the canvas edge.
///
/// `regions` are in frame coordinates and in the plan's order; the layout has one tile per
/// region in that order. The packing rule is in the module documentation. The margin is at
/// least one pixel, since a margin is what lets a detection be told from its neighbour's.
pub fn plan_mosaic(
    regions: &[BBox],
    canvas_width: u32,
    canvas_height: u32,
    margin: u32,
) -> Result<MosaicLayout, MosaicError> {
    if regions.is_empty() {
        return Err(MosaicError::NoRegions);
    }
    if margin == 0 {
        return Err(MosaicError::ZeroMargin);
    }
    if canvas_width == 0 || canvas_height == 0 {
        return Err(MosaicError::EmptyCanvas);
    }
    let sources = regions
        .iter()
        .enumerate()
        .map(|(region, bbox)| whole_pixels(bbox).ok_or(MosaicError::RegionOutOfRange { region }))
        .collect::<Result<Vec<_>, _>>()?;

    // Every sum below is of a few `u32` values, so `u64` cannot overflow.
    let (canvas_w, canvas_h, margin_w) = (
        u64::from(canvas_width),
        u64::from(canvas_height),
        u64::from(margin),
    );
    let (mut x, mut y, mut shelf_height) = (margin_w, margin_w, 0_u64);
    let mut tiles = Vec::with_capacity(sources.len());
    for (region, source) in sources.iter().enumerate() {
        let (w, h) = (u64::from(source.w), u64::from(source.h));
        if x > margin_w && x + w + margin_w > canvas_w {
            y += shelf_height + margin_w;
            x = margin_w;
            shelf_height = 0;
        }
        if x + w + margin_w > canvas_w || y + h + margin_w > canvas_h {
            return Err(MosaicError::DoesNotFit { region });
        }
        let (Ok(tile_x), Ok(tile_y)) = (u32::try_from(x), u32::try_from(y)) else {
            return Err(MosaicError::DoesNotFit { region });
        };
        tiles.push(Tile {
            frame: *source,
            canvas: Rect::new(tile_x, tile_y, source.w, source.h),
        });
        x += w + margin_w;
        shelf_height = shelf_height.max(h);
    }
    Ok(MosaicLayout {
        width: canvas_width,
        height: canvas_height,
        margin,
        tiles,
    })
}

/// The region rounded outward to whole pixels, or `None` outside the `u32` pixel range.
fn whole_pixels(region: &BBox) -> Option<Rect> {
    let (x0, y0) = (region.x0().floor(), region.y0().floor());
    let (x1, y1) = (region.x1().ceil(), region.y1().ceil());
    let limit = f64::from(u32::MAX);
    if x0 < 0.0 || y0 < 0.0 || x1 > limit || y1 > limit {
        return None;
    }
    // A box has positive extent, so each rounded size is at least one pixel.
    Some(Rect::new(
        x0 as u32,
        y0 as u32,
        (x1 - x0) as u32,
        (y1 - y0) as u32,
    ))
}

/// Whether `bbox` lies entirely inside `rect`, edges included.
fn lies_in(rect: &Rect, bbox: &BBox) -> bool {
    !rect.is_empty()
        && bbox.x0() >= f64::from(rect.x)
        && bbox.y0() >= f64::from(rect.y)
        && bbox.x1() <= f64::from(rect.right())
        && bbox.y1() <= f64::from(rect.bottom())
}

/// `bbox` moved by `(dx, dy)`, with no rounding of its own.
fn shifted(bbox: &BBox, dx: f64, dy: f64) -> Option<BBox> {
    BBox::new(
        bbox.x0() + dx,
        bbox.y0() + dy,
        bbox.x1() + dx,
        bbox.y1() + dy,
    )
    .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bx(x0: f64, y0: f64, x1: f64, y1: f64) -> BBox {
        BBox::new(x0, y0, x1, y1).unwrap()
    }

    /// Fixed-seed generator (splitmix64): the same cases on every run, with no random crate
    /// in the domain (chapter 05 §5.5).
    struct Rng(u64);

    impl Rng {
        fn next_u64(&mut self) -> u64 {
            self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = self.0;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^ (z >> 31)
        }

        /// A number in `0..n`, for `n > 0`.
        fn below(&mut self, n: u32) -> u32 {
            u32::try_from(self.next_u64() % u64::from(n)).unwrap()
        }
    }

    /// Quarter pixels: every corner the tests generate is a multiple of 0.25, so each
    /// addition the mapping does is exact and boxes are compared with `==`.
    fn quarters(q: u32) -> f64 {
        f64::from(q) / 4.0
    }

    /// A region with quarter-pixel corners, its origin below `origin_side` and its sides at
    /// most `longest` pixels.
    fn generated_region(rng: &mut Rng, origin_side: u32, longest: u32) -> BBox {
        let x0 = rng.below(origin_side * 4);
        let y0 = rng.below(origin_side * 4);
        let x1 = x0 + 1 + rng.below(longest * 4);
        let y1 = y0 + 1 + rng.below(longest * 4);
        bx(quarters(x0), quarters(y0), quarters(x1), quarters(y1))
    }

    #[derive(Debug, Clone)]
    struct Case {
        regions: Vec<BBox>,
        width: u32,
        height: u32,
        margin: u32,
    }

    impl Case {
        fn plan(&self) -> Result<MosaicLayout, MosaicError> {
            plan_mosaic(&self.regions, self.width, self.height, self.margin)
        }

        /// A necessary condition for any packing, whatever its rule: every tile with a margin
        /// on its right and below it is a disjoint cell of the canvas inside `[margin, size)`.
        /// When the cells need more area than that, no layout can exist.
        fn exceeds_any_packing(&self) -> bool {
            let m = i128::from(self.margin);
            let room = (i128::from(self.width) - m).max(0) * (i128::from(self.height) - m).max(0);
            let need: i128 = self
                .regions
                .iter()
                .map(|r| {
                    let s = whole_pixels(r).unwrap();
                    (i128::from(s.w) + m) * (i128::from(s.h) + m)
                })
                .sum();
            need > room
        }
    }

    fn generated_case(rng: &mut Rng) -> Case {
        let count = 1 + rng.below(6);
        Case {
            regions: (0..count).map(|_| generated_region(rng, 40, 20)).collect(),
            width: 1 + rng.below(70),
            height: 1 + rng.below(70),
            margin: 1 + rng.below(4),
        }
    }

    /// Everything a returned layout promises, checked without using the planner's own code.
    fn assert_layout_is_valid(case: &Case, layout: &MosaicLayout) {
        let m = u64::from(case.margin);
        let tiles = layout.tiles();
        assert_eq!(layout.canvas_size(), (case.width, case.height));
        assert_eq!(layout.margin(), case.margin);
        assert_eq!(tiles.len(), case.regions.len(), "one tile per region");
        for (region, tile) in case.regions.iter().zip(tiles) {
            let (frame, canvas) = (tile.frame(), tile.canvas());
            // Rounded outward, and no more than that.
            assert_eq!(f64::from(frame.x), region.x0().floor());
            assert_eq!(f64::from(frame.y), region.y0().floor());
            assert_eq!(f64::from(frame.right()), region.x1().ceil());
            assert_eq!(f64::from(frame.bottom()), region.y1().ceil());
            assert!(lies_in(&frame, region));
            assert_eq!((canvas.w, canvas.h), (frame.w, frame.h));
            // Inside the canvas, a margin from every edge.
            assert!(u64::from(canvas.x) >= m && u64::from(canvas.y) >= m);
            assert!(u64::from(canvas.right()) + m <= u64::from(case.width));
            assert!(u64::from(canvas.bottom()) + m <= u64::from(case.height));
        }
        for (i, a) in tiles.iter().enumerate() {
            for b in &tiles[i + 1..] {
                let (a, b) = (a.canvas(), b.canvas());
                assert!(!a.intersects(&b), "tiles overlap: {a:?} {b:?}");
                let apart = u64::from(a.right()) + m <= u64::from(b.x)
                    || u64::from(b.right()) + m <= u64::from(a.x)
                    || u64::from(a.bottom()) + m <= u64::from(b.y)
                    || u64::from(b.bottom()) + m <= u64::from(a.y);
                assert!(apart, "tiles less than a margin apart: {a:?} {b:?}");
            }
        }
    }

    fn overlap_in_the_frame(regions: &[BBox]) -> bool {
        regions
            .iter()
            .enumerate()
            .any(|(i, a)| regions[i + 1..].iter().any(|b| a.overlaps(b)))
    }

    const CASES: u32 = 4000;

    /// The packing rule, pinned on one example: shelves in the plan's order.
    #[test]
    fn tiles_are_packed_in_shelves_in_the_plans_order() {
        let regions = [
            bx(50.0, 50.0, 80.0, 60.0),
            bx(0.0, 0.0, 30.0, 10.0),
            bx(7.0, 7.0, 37.0, 17.0),
            bx(0.0, 0.0, 10.0, 4.0),
        ];
        let layout = plan_mosaic(&regions, 80, 80, 5).unwrap();
        let placed: Vec<_> = layout.tiles().iter().map(Tile::canvas).collect();
        assert_eq!(
            placed,
            [
                Rect::new(5, 5, 30, 10),
                Rect::new(40, 5, 30, 10),
                Rect::new(5, 20, 30, 10),
                Rect::new(40, 20, 10, 4),
            ]
        );
        assert_eq!(layout.tiles()[0].frame(), Rect::new(50, 50, 30, 10));
    }

    /// A region with fractional corners becomes the smallest whole-pixel rectangle that
    /// covers it.
    #[test]
    fn a_region_with_fractional_corners_is_rounded_outward() {
        let regions = [bx(10.25, 3.75, 20.5, 4.0), bx(1.0, 1.0, 1.25, 1.25)];
        let layout = plan_mosaic(&regions, 100, 100, 2).unwrap();
        assert_eq!(layout.tiles()[0].frame(), Rect::new(10, 3, 11, 1));
        assert_eq!(layout.tiles()[1].frame(), Rect::new(1, 1, 1, 1));
        assert_eq!(layout.tiles()[1].canvas().w, 1);
    }

    /// For generated regions, canvases and margins: a returned layout has one tile per region
    /// in order, each the size of its region rounded outward, inside the canvas, a margin from
    /// its edges and from every other tile.
    #[test]
    fn generated_layouts_keep_every_tile_inside_the_canvas_and_a_margin_apart() {
        let mut rng = Rng(0x0005_EED0_1234_0041);
        let (mut fitted, mut refused, mut overlapping, mut impossible) = (0, 0, 0, 0);
        for _ in 0..CASES {
            let case = generated_case(&mut rng);
            match case.plan() {
                Ok(layout) => {
                    assert!(!case.exceeds_any_packing(), "{case:?}");
                    assert_layout_is_valid(&case, &layout);
                    fitted += 1;
                    overlapping += usize::from(overlap_in_the_frame(&case.regions));
                }
                Err(MosaicError::DoesNotFit { region }) => {
                    assert!(region < case.regions.len());
                    refused += 1;
                    impossible += usize::from(case.exceeds_any_packing());
                }
                Err(other) => panic!("unexpected {other:?} for {case:?}"),
            }
        }
        assert!(fitted > 100, "layouts that fit: {fitted}");
        assert!(refused > 100, "layouts refused: {refused}");
        assert!(
            overlapping > 0,
            "layouts of regions that overlap in the frame"
        );
        assert!(impossible > 0, "refusals that no packing could avoid");
    }

    /// A plan that cannot fit returns no layout and names the first region that did not, and
    /// a plan that just fits returns every region.
    #[test]
    fn no_layout_is_returned_when_a_region_does_not_fit() {
        let wide = bx(0.0, 0.0, 30.0, 10.0);
        let tall = bx(0.0, 0.0, 10.0, 30.0);
        // Exactly the margin on every side.
        assert!(plan_mosaic(&[wide], 40, 20, 5).is_ok());
        // One pixel too narrow, or too short.
        assert_eq!(
            plan_mosaic(&[wide], 39, 20, 5),
            Err(MosaicError::DoesNotFit { region: 0 })
        );
        assert_eq!(
            plan_mosaic(&[wide], 40, 19, 5),
            Err(MosaicError::DoesNotFit { region: 0 })
        );
        // Two fit, the third needs a shelf that the canvas does not have.
        let three = [wide, wide, wide];
        assert_eq!(
            plan_mosaic(&three, 75, 20, 5),
            Err(MosaicError::DoesNotFit { region: 2 })
        );
        assert_eq!(
            plan_mosaic(&[wide, tall, wide], 80, 40, 5),
            Err(MosaicError::DoesNotFit { region: 2 })
        );
        let layout = plan_mosaic(&three, 75, 40, 5).unwrap();
        assert_eq!(layout.tiles().len(), 3);
    }

    /// Mapping forward and then back returns the same region and the same box, compared with
    /// `==`. The corners are multiples of a quarter pixel, so the additions are exact.
    #[test]
    fn mapping_a_box_forward_and_back_is_exact() {
        let mut rng = Rng(0x00C0_FFEE_0000_0041);
        let (mut trips, mut out_of_region) = (0, 0);
        for _ in 0..CASES {
            let case = generated_case(&mut rng);
            let Ok(layout) = case.plan() else { continue };
            for (index, tile) in layout.tiles().iter().enumerate() {
                let inside = inside_box(&mut rng, &tile.frame());
                let on_canvas = layout.to_canvas(index, &inside).unwrap();
                assert!(lies_in(&tile.canvas(), &on_canvas));
                assert_eq!(layout.to_frame(&on_canvas), Some((index, inside)));
                trips += 1;

                // A box that sticks out of the region's rectangle has no canvas box.
                let sticking = bx(
                    inside.x0(),
                    inside.y0(),
                    f64::from(tile.frame().right()) + 0.25,
                    inside.y1(),
                );
                assert_eq!(layout.to_canvas(index, &sticking), None);
                out_of_region += 1;
            }
            assert_eq!(
                layout.to_canvas(layout.tiles().len(), &case.regions[0]),
                None
            );
        }
        assert!(trips > 1000 && out_of_region > 1000);
    }

    /// A box with quarter-pixel corners entirely inside `rect`.
    fn inside_box(rng: &mut Rng, rect: &Rect) -> BBox {
        let (left, top) = (rect.x * 4, rect.y * 4);
        let x0 = left + rng.below(rect.w * 4);
        let y0 = top + rng.below(rect.h * 4);
        let x1 = x0 + 1 + rng.below(rect.right() * 4 - x0);
        let y1 = y0 + 1 + rng.below(rect.bottom() * 4 - y0);
        bx(quarters(x0), quarters(y0), quarters(x1), quarters(y1))
    }

    /// A detection that fills its tile exactly is kept: it ends on the edge and does not
    /// enter the margin. One quarter of a pixel further and it is discarded.
    #[test]
    fn a_detection_filling_its_tile_is_kept_and_one_quarter_pixel_more_is_not() {
        let layout = plan_mosaic(&[bx(10.0, 10.0, 30.0, 20.0)], 40, 30, 4).unwrap();
        let tile = layout.tiles()[0].canvas();
        let exact = bx(4.0, 4.0, 24.0, 14.0);
        assert_eq!(tile, Rect::new(4, 4, 20, 10));
        assert_eq!(
            layout.to_frame(&exact),
            Some((0, bx(10.0, 10.0, 30.0, 20.0)))
        );
        assert_eq!(layout.to_frame(&bx(3.75, 4.0, 24.0, 14.0)), None);
        assert_eq!(layout.to_frame(&bx(4.0, 3.75, 24.0, 14.0)), None);
        assert_eq!(layout.to_frame(&bx(4.0, 4.0, 24.25, 14.0)), None);
        assert_eq!(layout.to_frame(&bx(4.0, 4.0, 24.0, 14.25)), None);
    }

    /// Indices of the reasons a detection is discarded, tallied by the generated test.
    const REASONS: [&str; 8] = [
        "crosses the right edge",
        "crosses the left edge",
        "crosses the top edge",
        "crosses the bottom edge",
        "straddles two tiles",
        "in the margin beside a tile",
        "in the band at the canvas edge",
        "outside the canvas",
    ];

    /// A detection that touches a margin, straddles two tiles, crosses a tile's edge by any
    /// amount, lies in the band at the canvas edge or outside the canvas is discarded; one
    /// inside a tile is kept.
    #[test]
    fn detections_that_enter_a_margin_or_cross_a_tile_are_discarded() {
        let mut rng = Rng(0x0D15_CA2D_0000_0041);
        let mut discarded = [0_usize; REASONS.len()];
        let mut kept = 0;
        for _ in 0..CASES {
            let case = generated_case(&mut rng);
            let Ok(layout) = case.plan() else { continue };
            let (width, height) = (f64::from(case.width), f64::from(case.height));
            let tiles: Vec<Rect> = layout.tiles().iter().map(Tile::canvas).collect();
            for (index, t) in tiles.iter().enumerate() {
                let own = inside_box(&mut rng, t);
                assert!(layout.to_frame(&own).is_some_and(|(i, _)| i == index));
                kept += 1;
                // Any amount from a quarter pixel up to the whole margin.
                let reach = quarters(1 + rng.below(case.margin * 4));
                let (left, top) = (f64::from(t.x), f64::from(t.y));
                let (right, bottom) = (f64::from(t.right()), f64::from(t.bottom()));
                let probes = [
                    bx(own.x0(), own.y0(), right + reach, own.y1()),
                    bx(left - reach, own.y0(), own.x1(), own.y1()),
                    bx(own.x0(), top - reach, own.x1(), own.y1()),
                    bx(own.x0(), own.y0(), own.x1(), bottom + reach),
                    // Wholly in the margin beside the tile, right of it.
                    bx(right, own.y0(), right + reach, own.y1()),
                    // Wholly in the band along the left and bottom canvas edges.
                    bx(0.0, own.y0(), reach, own.y1()),
                    bx(width - reach, height - reach, width, height),
                    // Wholly outside the canvas.
                    bx(width, height, width + 1.0, height + 1.0),
                ];
                let reasons = [0, 1, 2, 3, 5, 6, 6, 7];
                for (probe, reason) in probes.iter().zip(reasons) {
                    assert_eq!(layout.to_frame(probe), None, "{}", REASONS[reason]);
                    discarded[reason] += 1;
                }
                if let Some(next) = tiles.get(index + 1) {
                    let across = own.union(&inside_box(&mut rng, next));
                    assert_eq!(layout.to_frame(&across), None, "{}", REASONS[4]);
                    discarded[4] += 1;
                }
            }
        }
        assert!(kept > 1000, "detections kept: {kept}");
        for (count, reason) in discarded.iter().zip(REASONS) {
            assert!(*count > 100, "{reason}: {count}");
        }
    }

    /// Two regions that overlap in the frame get separate tiles, and a detection in one tile
    /// is never attributed to the other, wherever it lies in the shared pixels.
    #[test]
    fn regions_that_overlap_in_the_frame_get_separate_tiles() {
        let first = bx(100.0, 100.0, 140.0, 130.0);
        let second = bx(120.0, 110.0, 170.0, 150.0);
        let shared = bx(125.0, 115.0, 135.0, 125.0);
        assert!(first.overlaps(&second) && first.contains(&shared) && second.contains(&shared));

        let layout = plan_mosaic(&[first, second], 200, 100, 6).unwrap();
        let (a, b) = (layout.tiles()[0].canvas(), layout.tiles()[1].canvas());
        assert!(!a.intersects(&b) && a.gap(&b) >= 6);

        let in_first = layout.to_canvas(0, &shared).unwrap();
        let in_second = layout.to_canvas(1, &shared).unwrap();
        assert_ne!(in_first, in_second);
        assert_eq!(layout.to_frame(&in_first), Some((0, shared)));
        assert_eq!(layout.to_frame(&in_second), Some((1, shared)));
        // The shared box exists in the frame only; no canvas box is in both tiles.
        assert!(lies_in(&a, &in_first) && !lies_in(&b, &in_first));
        assert!(lies_in(&b, &in_second) && !lies_in(&a, &in_second));
        // The same region listed twice is two tiles, too.
        let twice = plan_mosaic(&[first, first], 200, 100, 6).unwrap();
        assert_ne!(twice.tiles()[0].canvas(), twice.tiles()[1].canvas());
    }

    /// Planning twice gives equal layouts. The order of the regions may change the layout, but
    /// every invariant holds for it as well.
    #[test]
    fn planning_is_deterministic_and_every_order_keeps_the_invariants() {
        let mut rng = Rng(0x0DE7_E241_0000_0041);
        let (mut reordered, mut differed) = (0, 0);
        for _ in 0..CASES {
            let mut case = generated_case(&mut rng);
            let first = case.plan();
            assert_eq!(first, case.plan());
            if let Ok(layout) = &first {
                assert_layout_is_valid(&case, layout);
            }
            case.regions.reverse();
            let second = case.plan();
            assert_eq!(second, case.plan());
            if let Ok(layout) = &second {
                assert_layout_is_valid(&case, layout);
                reordered += 1;
            }
            if first.is_ok() != second.is_ok() {
                differed += 1;
            }
        }
        assert!(reordered > 100, "reversed plans that fit: {reordered}");
        assert!(differed > 0, "the order never changed the outcome");
    }

    /// Degenerate input returns a value and never panics.
    #[test]
    fn degenerate_input_returns_a_value_and_never_panics() {
        let one = [bx(0.0, 0.0, 4.0, 4.0)];
        assert_eq!(plan_mosaic(&[], 100, 100, 4), Err(MosaicError::NoRegions));
        assert_eq!(plan_mosaic(&one, 100, 100, 0), Err(MosaicError::ZeroMargin));
        assert_eq!(plan_mosaic(&one, 0, 100, 4), Err(MosaicError::EmptyCanvas));
        assert_eq!(plan_mosaic(&one, 100, 0, 4), Err(MosaicError::EmptyCanvas));
        assert_eq!(
            plan_mosaic(&one, 8, 8, 4),
            Err(MosaicError::DoesNotFit { region: 0 })
        );
        // A margin and a canvas of the largest sizes: the sums do not overflow.
        assert_eq!(
            plan_mosaic(&one, u32::MAX, u32::MAX, u32::MAX),
            Err(MosaicError::DoesNotFit { region: 0 })
        );
        assert!(plan_mosaic(&one, u32::MAX, u32::MAX, 4).is_ok());
        let wide = bx(0.0, 0.0, f64::from(u32::MAX), 1.0);
        assert_eq!(
            plan_mosaic(&[wide], u32::MAX, u32::MAX, 1),
            Err(MosaicError::DoesNotFit { region: 0 })
        );

        // Outside the frame's coordinates: negative, or past the pixel range.
        for outside in [
            bx(-1.0, 0.0, 4.0, 4.0),
            bx(0.0, -0.5, 4.0, 4.0),
            bx(-9.0, -9.0, -1.0, -1.0),
            bx(0.0, 0.0, f64::from(u32::MAX) + 1.0, 4.0),
            bx(0.0, 0.0, 4.0, 1.0e300),
            bx(f64::MAX / 2.0, 0.0, f64::MAX, 4.0),
        ] {
            assert_eq!(
                plan_mosaic(&[one[0], outside], 100, 100, 4),
                Err(MosaicError::RegionOutOfRange { region: 1 })
            );
        }

        // A box is finite and has an area by construction, so the non-finite and empty
        // detections are refused where the box is built, before any mapping.
        for (x0, y0, x1, y1) in [
            (f64::NAN, 0.0, 1.0, 1.0),
            (0.0, 0.0, f64::INFINITY, 1.0),
            (0.0, f64::NEG_INFINITY, 1.0, 1.0),
            (1.0, 1.0, 1.0, 2.0),
            (1.0, 1.0, 2.0, 1.0),
            (2.0, 2.0, 1.0, 1.0),
        ] {
            assert!(BBox::new(x0, y0, x1, y1).is_err());
        }

        // Mapping with a layout on far-away and tiny boxes.
        let layout = plan_mosaic(&one, 20, 20, 4).unwrap();
        for far in [
            bx(-1.0e300, -1.0e300, 1.0e300, 1.0e300),
            bx(1.0e300, 1.0e300, 2.0e300, 2.0e300),
            bx(5.0, 5.0, 5.0 + f64::EPSILON * 8.0, 6.0),
        ] {
            let _ = layout.to_frame(&far);
            let _ = layout.to_canvas(0, &far);
        }
        assert_eq!(layout.to_canvas(1, &one[0]), None);
        assert_eq!(layout.to_canvas(usize::MAX, &one[0]), None);
    }
}
