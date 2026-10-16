//! Algorithm 1 of the thesis: incremental perception without losing evidence (chapter 04 §4.2).
//!
//! Each frame is compared exactly, tile by tile, against the **committed base** (the last
//! frame whose evidence was published), not against the previous capture. Dirty tiles plus
//! the regions still pending from a failed observation are grouped into 8-connected
//! components, padded with a halo and closed over the evidence they touch. A plan that runs
//! out of budget falls back to a full refresh; nothing is ever dropped.
//!
//! The reference behaviour is the Python oracle in
//! `docs/thesis/referencia_ejecutable/oraculo/core.py` (`Frame`, `dirty_tiles`, `halo_close`,
//! `merge_regions`); the tests below port its cases under their planned IDs (UT-01..12).

use std::collections::{BTreeSet, VecDeque};
use std::sync::Arc;

use thiserror::Error;

use crate::bbox::BBox;

/// Largest frame accepted, in pixels (the oracle's bound; above 8K UHD).
pub const MAX_FRAME_PIXELS: u64 = 20_000_000;

/// Initial tile size of the thesis. It plays no part in identity.
pub const DEFAULT_TILE_SIZE: u32 = 64;

/// Bytes per BGRA8 pixel.
const BPP: usize = 4;

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum IncrementalError {
    #[error("frame dimensions must be positive and at most {MAX_FRAME_PIXELS} pixels")]
    InvalidDimensions,
    #[error("frame buffer does not match its width, height and row stride")]
    InvalidBuffer,
    #[error("tile size must be positive")]
    InvalidTileSize,
    #[error("region lies outside the frame")]
    RegionOutOfBounds,
    #[error("support lies outside the frame")]
    SupportOutOfBounds,
}

/// One BGRA8 capture of the selected monitor with its row stride and geometry epoch.
///
/// Rows may carry padding after `4 * width` bytes; padding and alpha are never compared.
/// The pixels are shared, so keeping a frame as the committed base is cheap.
#[derive(Clone, PartialEq, Eq)]
pub struct BgraFrame {
    width: u32,
    height: u32,
    row_stride: usize,
    geometry_epoch: u64,
    pixels: Arc<[u8]>,
}

impl std::fmt::Debug for BgraFrame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BgraFrame")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("row_stride", &self.row_stride)
            .field("geometry_epoch", &self.geometry_epoch)
            .finish_non_exhaustive()
    }
}

impl BgraFrame {
    pub fn new(
        width: u32,
        height: u32,
        row_stride: usize,
        geometry_epoch: u64,
        pixels: impl Into<Arc<[u8]>>,
    ) -> Result<Self, IncrementalError> {
        let pixels = pixels.into();
        if width == 0 || height == 0 || u64::from(width) * u64::from(height) > MAX_FRAME_PIXELS {
            return Err(IncrementalError::InvalidDimensions);
        }
        let row_bytes = BPP * width as usize;
        let expected = row_stride.checked_mul(height as usize);
        if row_stride < row_bytes || expected != Some(pixels.len()) {
            return Err(IncrementalError::InvalidBuffer);
        }
        Ok(Self {
            width,
            height,
            row_stride,
            geometry_epoch,
            pixels,
        })
    }

    pub const fn width(&self) -> u32 {
        self.width
    }

    pub const fn height(&self) -> u32 {
        self.height
    }

    pub const fn row_stride(&self) -> usize {
        self.row_stride
    }

    /// Bumped by a change of monitor, DPI, rotation or model configuration.
    pub const fn geometry_epoch(&self) -> u64 {
        self.geometry_epoch
    }

    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// Overwrites the pixels with zeros when this frame holds the only copy of its buffer, and
    /// returns whether it did. A buffer still shared with another holder is left alone, so the
    /// caller must not count on it being overwritten (chapter 08 §8.4).
    pub fn overwrite(&mut self) -> bool {
        let Some(pixels) = Arc::get_mut(&mut self.pixels) else {
            return false;
        };
        pixels.fill(0);
        true
    }

    /// The whole frame as a box.
    pub fn bounds(&self) -> BBox {
        frame_box(self.width, self.height)
    }

    /// Bytes `[x0, x1)` of row `y`, without padding.
    fn row(&self, y: u32, x0: u32, x1: u32) -> &[u8] {
        let start = y as usize * self.row_stride;
        &self.pixels[start + BPP * x0 as usize..start + BPP * x1 as usize]
    }
}

fn frame_box(width: u32, height: u32) -> BBox {
    BBox::new(0.0, 0.0, f64::from(width), f64::from(height))
        .expect("a validated frame has positive dimensions")
}

/// A tile of the grid, ordered row-major so sets of tiles are emitted top to bottom.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TileCoord {
    row: u32,
    col: u32,
}

impl TileCoord {
    pub const fn new(col: u32, row: u32) -> Self {
        Self { row, col }
    }

    pub const fn col(&self) -> u32 {
        self.col
    }

    pub const fn row(&self) -> u32 {
        self.row
    }
}

/// Square tiles over a frame; the right and bottom tiles may be partial.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TileGrid {
    width: u32,
    height: u32,
    tile_size: u32,
    cols: u32,
    rows: u32,
}

impl TileGrid {
    pub fn new(width: u32, height: u32, tile_size: u32) -> Result<Self, IncrementalError> {
        if tile_size == 0 {
            return Err(IncrementalError::InvalidTileSize);
        }
        if width == 0 || height == 0 {
            return Err(IncrementalError::InvalidDimensions);
        }
        Ok(Self {
            width,
            height,
            tile_size,
            cols: width.div_ceil(tile_size),
            rows: height.div_ceil(tile_size),
        })
    }

    pub fn for_frame(frame: &BgraFrame, tile_size: u32) -> Result<Self, IncrementalError> {
        Self::new(frame.width, frame.height, tile_size)
    }

    pub const fn cols(&self) -> u32 {
        self.cols
    }

    pub const fn rows(&self) -> u32 {
        self.rows
    }

    pub const fn contains(&self, tile: TileCoord) -> bool {
        tile.col < self.cols && tile.row < self.rows
    }

    /// Every tile, row-major.
    pub fn all_tiles(&self) -> BTreeSet<TileCoord> {
        (0..self.rows)
            .flat_map(|row| (0..self.cols).map(move |col| TileCoord::new(col, row)))
            .collect()
    }

    /// Pixel columns `[x0, x1)` and rows `[y0, y1)` covered by `tile`.
    fn span(&self, tile: TileCoord) -> (u32, u32, u32, u32) {
        let x0 = tile.col * self.tile_size;
        let y0 = tile.row * self.tile_size;
        (
            x0,
            y0,
            (x0 + self.tile_size).min(self.width),
            (y0 + self.tile_size).min(self.height),
        )
    }

    /// The pixels covered by `tile`, as a box.
    pub fn tile_box(&self, tile: TileCoord) -> BBox {
        let (x0, y0, x1, y1) = self.span(tile);
        BBox::new(f64::from(x0), f64::from(y0), f64::from(x1), f64::from(y1))
            .expect("a tile inside the grid has positive size")
    }
}

/// Why the whole frame must be processed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FullRefreshReason {
    /// Nothing has been committed yet.
    NoCommittedBase,
    /// Width, height or geometry epoch differ from the committed base.
    GeometryChanged,
    /// The halo closure did not reach a fixed point within its budget.
    ClosureBudget,
    /// More regions than the plan allows.
    TooManyRegions,
    /// The regions cover more of the frame than the plan allows.
    AreaBudget,
    /// The experimental full-frame baseline (B0, `[engine] experimental_full_frame`) asks for
    /// the whole frame on every observation. Never a production reason (dossier chapter 09
    /// §9.2): it exists to measure what incrementality saves.
    ExperimentalBaseline,
    /// The adaptive cost rule (INC-07, ADR-0030): the regional plan has at least
    /// `adaptive_min_regions` regions, so it would cost more inferences than the one full-frame
    /// inference. Algorithm 1's full refresh, taken earlier than the budgets would take it.
    AdaptiveCost,
}

impl FullRefreshReason {
    /// The reason's name on the research record and in `_meta` (`io.ommatix/plan`).
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NoCommittedBase => "no_committed_base",
            Self::GeometryChanged => "geometry_changed",
            Self::ClosureBudget => "closure_budget",
            Self::TooManyRegions => "too_many_regions",
            Self::AreaBudget => "area_budget",
            Self::ExperimentalBaseline => "experimental_baseline",
            Self::AdaptiveCost => "adaptive_cost",
        }
    }
}

/// Tiles of `current` that differ from `base`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TileDiff {
    /// Changed tiles; every tile when `full_refresh` is set.
    pub tiles: BTreeSet<TileCoord>,
    pub full_refresh: Option<FullRefreshReason>,
}

/// Exact BGR comparison of `current` against the committed `base`, ignoring alpha and row
/// padding. Every pixel of a tile is visited until one differs. Pure: no state changes.
pub fn dirty_tiles(
    base: Option<&BgraFrame>,
    current: &BgraFrame,
    tile_size: u32,
) -> Result<TileDiff, IncrementalError> {
    dirty_tiles_matching(base, current, tile_size, |_, _| true)
}

/// Exact comparison restricted to tiles intersecting `regions` (ADR-0032). Geometry
/// and epoch changes still return every tile of the whole frame. Algorithm 1 continues
/// to use [`dirty_tiles`], so observation incrementality is unchanged.
pub fn dirty_tiles_in_regions(
    base: Option<&BgraFrame>,
    current: &BgraFrame,
    tile_size: u32,
    regions: &[BBox],
) -> Result<TileDiff, IncrementalError> {
    dirty_tiles_matching(base, current, tile_size, |grid, tile| {
        let bbox = grid.tile_box(tile);
        regions.iter().any(|region| region.overlaps(&bbox))
    })
}

fn dirty_tiles_matching(
    base: Option<&BgraFrame>,
    current: &BgraFrame,
    tile_size: u32,
    includes: impl Fn(&TileGrid, TileCoord) -> bool,
) -> Result<TileDiff, IncrementalError> {
    let grid = TileGrid::for_frame(current, tile_size)?;
    let reason = match base {
        None => Some(FullRefreshReason::NoCommittedBase),
        Some(b)
            if (b.width, b.height, b.geometry_epoch)
                != (current.width, current.height, current.geometry_epoch) =>
        {
            Some(FullRefreshReason::GeometryChanged)
        }
        Some(_) => None,
    };
    let Some(base) = base.filter(|_| reason.is_none()) else {
        return Ok(TileDiff {
            tiles: grid.all_tiles(),
            full_refresh: reason,
        });
    };
    let tiles = grid
        .all_tiles()
        .into_iter()
        .filter(|&tile| includes(&grid, tile) && tile_changed(base, current, &grid, tile))
        .collect();
    Ok(TileDiff {
        tiles,
        full_refresh: None,
    })
}

fn tile_changed(base: &BgraFrame, current: &BgraFrame, grid: &TileGrid, tile: TileCoord) -> bool {
    let (x0, y0, x1, y1) = grid.span(tile);
    (y0..y1).any(|y| {
        let (old, _) = base.row(y, x0, x1).as_chunks::<BPP>();
        let (new, _) = current.row(y, x0, x1).as_chunks::<BPP>();
        old.iter().zip(new).any(|(o, n)| o[..3] != n[..3])
    })
}

/// Result of a halo closure.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Closure {
    /// The fixed point: no support or relation touches the region without being inside it.
    Closed(BBox),
    /// The rounds ran out first; the conservative answer is a full refresh.
    BudgetExhausted,
}

/// Grows `region` by every support it overlaps, and by every member of a relation one of
/// whose members it overlaps, until nothing changes. The fixed point does not depend on the
/// order of `supports` or `relations`.
pub fn halo_close(
    region: BBox,
    supports: &[BBox],
    relations: &[(BBox, BBox)],
    bounds: BBox,
    max_rounds: usize,
) -> Result<Closure, IncrementalError> {
    if !bounds.contains(&region) {
        return Err(IncrementalError::RegionOutOfBounds);
    }
    let in_bounds = |b: &BBox| bounds.contains(b);
    if !supports.iter().all(in_bounds)
        || !relations.iter().all(|(a, b)| in_bounds(a) && in_bounds(b))
    {
        return Err(IncrementalError::SupportOutOfBounds);
    }
    let mut region = region;
    for _ in 0..max_rounds {
        let previous = region;
        for support in supports {
            if region.overlaps(support) {
                region = region.union(support);
            }
        }
        for (a, b) in relations {
            if region.overlaps(a) || region.overlaps(b) {
                region = region.union(a).union(b);
            }
        }
        if region == previous {
            return Ok(Closure::Closed(region));
        }
    }
    Ok(Closure::BudgetExhausted)
}

/// Merges overlapping regions until none overlap, sorted by `(x0, y0, x1, y1)`.
pub fn merge_regions(regions: &[BBox]) -> Vec<BBox> {
    let mut work = regions.to_vec();
    work.sort_by(BBox::cmp_corners);
    'restart: loop {
        for i in 0..work.len() {
            for j in i + 1..work.len() {
                if work[i].overlaps(&work[j]) {
                    let other = work.remove(j);
                    work[i] = work[i].union(&other);
                    continue 'restart;
                }
            }
        }
        break;
    }
    work.sort_by(BBox::cmp_corners);
    work
}

/// 8-connected components of `tiles`, each as its tile set, in row-major order of their
/// first tile.
pub fn connected_components(tiles: &BTreeSet<TileCoord>) -> Vec<BTreeSet<TileCoord>> {
    let mut unvisited = tiles.clone();
    let mut components = Vec::new();
    while let Some(seed) = unvisited.pop_first() {
        let mut component = BTreeSet::from([seed]);
        let mut queue = VecDeque::from([seed]);
        while let Some(tile) = queue.pop_front() {
            for dr in -1i64..=1 {
                for dc in -1i64..=1 {
                    let (Ok(row), Ok(col)) = (
                        u32::try_from(i64::from(tile.row) + dr),
                        u32::try_from(i64::from(tile.col) + dc),
                    ) else {
                        continue;
                    };
                    let next = TileCoord::new(col, row);
                    if unvisited.remove(&next) {
                        component.insert(next);
                        queue.push_back(next);
                    }
                }
            }
        }
        components.push(component);
    }
    components
}

/// Initial plan parameters of the thesis; to be replaced by a measured cost model.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlanParams {
    pub tile_size: u32,
    /// Padding added around each component before the closure, in pixels.
    pub halo_px: f64,
    /// Rounds a closure may take before the plan falls back to a full refresh.
    pub max_closure_rounds: usize,
    /// More regions than this fall back to a full refresh.
    pub max_regions: usize,
    /// Regions covering more than this fraction of the frame fall back to a full refresh.
    pub max_area_fraction: f64,
    /// The experimental full-frame baseline (B0): every observation processes the whole
    /// frame, whatever changed. Off by default and never a production mode; the planner
    /// still computes the tile diff, so both variants pay the same planning cost.
    pub experimental_full_frame: bool,
    /// The adaptive cost rule (INC-07, ADR-0030): a regional plan with this many regions or
    /// more becomes the full refresh, reason `AdaptiveCost`, because every region is a whole
    /// detector inference and the full frame is one. `0` switches the rule off. The hard
    /// budgets (`max_regions`, `max_area_fraction`, the closure) are checked first and keep
    /// their own reasons; the rule never plans less work than they do, only earlier.
    pub adaptive_min_regions: usize,
}

impl Default for PlanParams {
    fn default() -> Self {
        Self {
            tile_size: DEFAULT_TILE_SIZE,
            halo_px: 32.0,
            max_closure_rounds: 1000,
            max_regions: 4,
            max_area_fraction: 0.4,
            experimental_full_frame: false,
            adaptive_min_regions: 0,
        }
    }
}

/// Visual evidence of the last snapshot that a region must not cut in half.
#[derive(Debug, Clone, Copy, Default)]
pub struct Evidence<'a> {
    /// Boxes of detected controls, texts and icons.
    pub supports: &'a [BBox],
    /// Pairs that must be re-processed together, such as a control and its external label.
    pub relations: &'a [(BBox, BBox)],
}

/// What the next observation has to process.
#[derive(Debug, Clone, PartialEq)]
pub enum Work {
    /// Nothing changed and nothing is pending.
    Nothing,
    /// Re-analyse these regions, disjoint and sorted by `(x0, y0, x1, y1)`.
    Regions(Vec<BBox>),
    /// Re-analyse the whole frame.
    FullRefresh(FullRefreshReason),
}

/// The plan for one frame. `dirty` is what a failed observation must keep pending.
#[derive(Debug, Clone, PartialEq)]
pub struct RegionPlan {
    pub dirty: BTreeSet<TileCoord>,
    pub work: Work,
}

/// Plans the regions to re-analyse in `current`: the tiles that differ from the committed
/// `base` plus the `pending` ones, grouped, padded, closed over `evidence` and merged.
pub fn plan_regions(
    base: Option<&BgraFrame>,
    current: &BgraFrame,
    pending: &BTreeSet<TileCoord>,
    evidence: Evidence<'_>,
    params: &PlanParams,
) -> Result<RegionPlan, IncrementalError> {
    let grid = TileGrid::for_frame(current, params.tile_size)?;
    let diff = dirty_tiles(base, current, params.tile_size)?;
    if let Some(reason) = diff.full_refresh {
        return Ok(RegionPlan {
            dirty: diff.tiles,
            work: Work::FullRefresh(reason),
        });
    }
    if params.experimental_full_frame {
        // B0: the same planner, forced to the whole frame. Every tile is processed, so every
        // tile is dirty if the observation fails.
        return Ok(RegionPlan {
            dirty: grid.all_tiles(),
            work: Work::FullRefresh(FullRefreshReason::ExperimentalBaseline),
        });
    }
    let mut dirty = diff.tiles;
    dirty.extend(pending.iter().copied().filter(|&t| grid.contains(t)));
    if dirty.is_empty() {
        return Ok(RegionPlan {
            dirty,
            work: Work::Nothing,
        });
    }

    let bounds = current.bounds();
    let full = |dirty, reason| {
        Ok(RegionPlan {
            dirty,
            work: Work::FullRefresh(reason),
        })
    };
    let mut regions = Vec::new();
    for component in connected_components(&dirty) {
        let tight = component
            .iter()
            .map(|&t| grid.tile_box(t))
            .reduce(|a, b| a.union(&b))
            .expect("a component has at least one tile");
        let padded = tight
            .pad(params.halo_px)
            .clip(&bounds)
            .expect("a padded tile still overlaps the frame");
        match halo_close(
            padded,
            evidence.supports,
            evidence.relations,
            bounds,
            params.max_closure_rounds,
        )? {
            Closure::Closed(region) => regions.push(region),
            Closure::BudgetExhausted => return full(dirty, FullRefreshReason::ClosureBudget),
        }
    }
    // Merging can make a region touch new evidence, so close and merge until both hold.
    loop {
        let merged = merge_regions(&regions);
        let mut closed = Vec::with_capacity(merged.len());
        for region in &merged {
            match halo_close(
                *region,
                evidence.supports,
                evidence.relations,
                bounds,
                params.max_closure_rounds,
            )? {
                Closure::Closed(r) => closed.push(r),
                Closure::BudgetExhausted => return full(dirty, FullRefreshReason::ClosureBudget),
            }
        }
        if closed == merged {
            regions = merged;
            break;
        }
        regions = closed;
    }

    if regions.len() > params.max_regions {
        return full(dirty, FullRefreshReason::TooManyRegions);
    }
    let covered: f64 = regions.iter().map(BBox::area).sum();
    if covered > params.max_area_fraction * bounds.area() {
        return full(dirty, FullRefreshReason::AreaBudget);
    }
    if params.adaptive_min_regions > 0 && regions.len() >= params.adaptive_min_regions {
        return full(dirty, FullRefreshReason::AdaptiveCost);
    }
    Ok(RegionPlan {
        dirty,
        work: Work::Regions(regions),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bx(x0: f64, y0: f64, x1: f64, y1: f64) -> BBox {
        BBox::new(x0, y0, x1, y1).unwrap()
    }

    /// The oracle's `blank`: a black frame with optional row padding.
    fn blank(w: u32, h: u32, padding: usize, epoch: u64) -> BgraFrame {
        let stride = 4 * w as usize + padding;
        BgraFrame::new(w, h, stride, epoch, vec![0u8; stride * h as usize]).unwrap()
    }

    /// The oracle's `changed`: one byte of pixel `(x, y)` set to 255.
    fn changed(frame: &BgraFrame, x: u32, y: u32, channel: usize) -> BgraFrame {
        let mut buf = frame.pixels().to_vec();
        buf[y as usize * frame.row_stride() + 4 * x as usize + channel] = 255;
        BgraFrame::new(
            frame.width(),
            frame.height(),
            frame.row_stride(),
            frame.geometry_epoch(),
            buf,
        )
        .unwrap()
    }

    fn tiles(list: &[(u32, u32)]) -> BTreeSet<TileCoord> {
        list.iter().map(|&(c, r)| TileCoord::new(c, r)).collect()
    }

    fn diff(base: Option<&BgraFrame>, current: &BgraFrame) -> TileDiff {
        dirty_tiles(base, current, DEFAULT_TILE_SIZE).unwrap()
    }

    /// UT-01 (OR-04): with no committed base the whole screen is dirty, even when it is black.
    #[test]
    fn ut_01_first_frame_marks_every_tile_dirty() {
        let d = diff(None, &blank(65, 65, 0, 1));
        assert_eq!(d.tiles.len(), 4);
        assert_eq!(d.full_refresh, Some(FullRefreshReason::NoCommittedBase));
    }

    /// UT-02 (OR-05): identical pixels give an empty dirty set.
    #[test]
    fn ut_02_identical_frames_have_no_dirty_tiles() {
        let f = blank(65, 65, 0, 1);
        assert_eq!(
            diff(Some(&f), &f),
            TileDiff {
                tiles: BTreeSet::new(),
                full_refresh: None
            }
        );
    }

    /// UT-03 (OR-06): a single pixel on an odd row is detected.
    #[test]
    fn ut_03_single_pixel_on_odd_row_is_detected() {
        let f = blank(65, 65, 0, 1);
        assert_eq!(
            diff(Some(&f), &changed(&f, 10, 1, 0)).tiles,
            tiles(&[(0, 0)])
        );
    }

    /// UT-04 (OR-07): the last pixel of an odd-sized frame lands in the partial tile.
    #[test]
    fn ut_04_last_pixel_of_odd_frame_is_detected() {
        let f = blank(65, 65, 0, 1);
        assert_eq!(
            diff(Some(&f), &changed(&f, 64, 64, 0)).tiles,
            tiles(&[(1, 1)])
        );
    }

    /// UT-05 (OR-08..OR-10): alpha and row padding are ignored, a real pixel behind padding
    /// is not.
    #[test]
    fn ut_05_alpha_and_padding_are_ignored_but_real_pixels_are_not() {
        let f = blank(65, 65, 0, 1);
        assert_eq!(diff(Some(&f), &changed(&f, 1, 1, 3)).tiles.len(), 0);

        let padded = blank(65, 65, 8, 1);
        let mut buf = padded.pixels().to_vec();
        buf[4 * 65] = 1; // first padding byte of row 0
        let g = BgraFrame::new(65, 65, padded.row_stride(), 1, buf).unwrap();
        assert_eq!(diff(Some(&padded), &g).tiles.len(), 0);

        let padded = blank(65, 65, 12, 1);
        assert_eq!(
            diff(Some(&padded), &changed(&padded, 4, 63, 0)).tiles,
            tiles(&[(0, 0)])
        );
    }

    /// UT-06 (OR-11, OR-13): invalid dimensions, stride, buffer or tile size are errors, not
    /// panics.
    #[test]
    fn ut_06_invalid_input_is_rejected_without_panicking() {
        assert_eq!(
            BgraFrame::new(2, 2, 8, 1, b"bad".to_vec()).unwrap_err(),
            IncrementalError::InvalidBuffer
        );
        assert_eq!(
            BgraFrame::new(2, 2, 7, 1, vec![0; 14]).unwrap_err(),
            IncrementalError::InvalidBuffer
        );
        assert_eq!(
            BgraFrame::new(0, 2, 0, 1, Vec::new()).unwrap_err(),
            IncrementalError::InvalidDimensions
        );
        assert_eq!(
            BgraFrame::new(5000, 5000, 20_000, 1, Vec::new()).unwrap_err(),
            IncrementalError::InvalidDimensions
        );
        assert_eq!(
            BgraFrame::new(2, 2, usize::MAX, 1, Vec::new()).unwrap_err(),
            IncrementalError::InvalidBuffer
        );
        assert_eq!(
            dirty_tiles(None, &blank(65, 65, 0, 1), 0).unwrap_err(),
            IncrementalError::InvalidTileSize
        );
    }

    /// UT-07 (OR-12): a new geometry epoch forces a full refresh even with equal pixels.
    #[test]
    fn ut_07_epoch_change_forces_full_refresh() {
        let f = blank(65, 65, 0, 1);
        let g = BgraFrame::new(65, 65, f.row_stride(), 2, f.pixels().to_vec()).unwrap();
        let d = diff(Some(&f), &g);
        assert_eq!(d.tiles.len(), 4);
        assert_eq!(d.full_refresh, Some(FullRefreshReason::GeometryChanged));
    }

    /// UT-08 (OR-48): every valid pixel, when changed, marks exactly its own tile.
    #[test]
    fn ut_08_every_changed_pixel_marks_its_tile() {
        let f = blank(9, 7, 8, 1);
        for y in 0..f.height() {
            for x in 0..f.width() {
                let d = dirty_tiles(Some(&f), &changed(&f, x, y, 0), 4).unwrap();
                assert_eq!(d.tiles, tiles(&[(x / 4, y / 4)]), "pixel ({x}, {y})");
            }
        }
    }

    /// UT-09 (OR-14): the closure reaches supports that only touch the region through another.
    #[test]
    fn ut_09_halo_closure_is_transitive() {
        let r = bx(0.0, 0.0, 10.0, 10.0);
        let a = bx(8.0, 0.0, 20.0, 10.0);
        let b = bx(18.0, 0.0, 30.0, 10.0);
        let bounds = bx(0.0, 0.0, 100.0, 100.0);
        assert_eq!(
            halo_close(r, &[b, a], &[], bounds, 1000),
            Ok(Closure::Closed(bx(0.0, 0.0, 30.0, 10.0)))
        );
    }

    /// UT-10 (OR-15): permuting the supports does not change the closure.
    #[test]
    fn ut_10_halo_closure_does_not_depend_on_order() {
        let r = bx(0.0, 0.0, 10.0, 10.0);
        let s = [bx(8.0, 0.0, 20.0, 10.0), bx(18.0, 0.0, 30.0, 10.0)];
        let reversed = [s[1], s[0]];
        let bounds = bx(0.0, 0.0, 100.0, 100.0);
        assert_eq!(
            halo_close(r, &s, &[], bounds, 1000),
            halo_close(r, &reversed, &[], bounds, 1000)
        );
    }

    /// UT-11 (OR-16): an exhausted budget falls back to a full refresh.
    #[test]
    fn ut_11_exhausted_closure_budget_falls_back_to_full_refresh() {
        let bounds = bx(0.0, 0.0, 100.0, 100.0);
        assert_eq!(
            halo_close(
                bx(0.0, 0.0, 10.0, 10.0),
                &[bx(8.0, 0.0, 20.0, 10.0)],
                &[],
                bounds,
                0
            ),
            Ok(Closure::BudgetExhausted)
        );

        // In a plan: a chain of supports longer than the budget.
        let base = blank(256, 64, 0, 1);
        let current = changed(&base, 0, 0, 0);
        let chain: Vec<BBox> = (0..8)
            .map(|i| bx(f64::from(i) * 30.0, 0.0, f64::from(i) * 30.0 + 40.0, 10.0))
            .collect();
        let params = PlanParams {
            max_closure_rounds: 1,
            ..PlanParams::default()
        };
        let plan = plan_regions(
            Some(&base),
            &current,
            &BTreeSet::new(),
            Evidence {
                supports: &chain,
                relations: &[],
            },
            &params,
        )
        .unwrap();
        assert_eq!(
            plan.work,
            Work::FullRefresh(FullRefreshReason::ClosureBudget)
        );
        assert_eq!(plan.dirty, tiles(&[(0, 0)]));
    }

    /// UT-12 (OR-17): overlapping regions merge transitively, whatever their order.
    #[test]
    fn ut_12_overlapping_regions_merge_to_fixed_point() {
        let a = bx(0.0, 0.0, 10.0, 10.0);
        let b = bx(8.0, 0.0, 20.0, 10.0);
        let c = bx(18.0, 0.0, 30.0, 10.0);
        assert_eq!(merge_regions(&[c, a, b]), vec![bx(0.0, 0.0, 30.0, 10.0)]);
    }

    #[test]
    fn halo_closure_rejects_out_of_bounds_input() {
        let bounds = bx(0.0, 0.0, 100.0, 100.0);
        let inside = bx(0.0, 0.0, 10.0, 10.0);
        let outside = bx(90.0, 90.0, 110.0, 100.0);
        assert_eq!(
            halo_close(outside, &[], &[], bounds, 10),
            Err(IncrementalError::RegionOutOfBounds)
        );
        assert_eq!(
            halo_close(inside, &[outside], &[], bounds, 10),
            Err(IncrementalError::SupportOutOfBounds)
        );
    }

    #[test]
    fn halo_closure_pulls_in_the_other_member_of_a_relation() {
        let bounds = bx(0.0, 0.0, 200.0, 100.0);
        let label = bx(0.0, 0.0, 30.0, 10.0);
        let field = bx(100.0, 0.0, 150.0, 10.0);
        assert_eq!(
            halo_close(bx(5.0, 0.0, 10.0, 5.0), &[], &[(label, field)], bounds, 10),
            Ok(Closure::Closed(bx(0.0, 0.0, 150.0, 10.0)))
        );
    }

    #[test]
    fn diagonal_tiles_form_one_component() {
        let comps = connected_components(&tiles(&[(0, 0), (1, 1), (3, 1)]));
        assert_eq!(comps, vec![tiles(&[(0, 0), (1, 1)]), tiles(&[(3, 1)])]);
    }

    #[test]
    fn plan_pads_each_component_with_the_halo() {
        let base = blank(640, 320, 0, 1);
        let current = changed(&base, 200, 200, 0);
        let plan = plan_regions(
            Some(&base),
            &current,
            &BTreeSet::new(),
            Evidence::default(),
            &PlanParams::default(),
        )
        .unwrap();
        assert_eq!(plan.dirty, tiles(&[(3, 3)]));
        assert_eq!(
            plan.work,
            Work::Regions(vec![bx(160.0, 160.0, 288.0, 288.0)])
        );
    }

    #[test]
    fn plan_keeps_pending_tiles_from_a_failed_observation() {
        let base = blank(640, 320, 0, 1);
        let pending = tiles(&[(9, 0)]);
        let plan = plan_regions(
            Some(&base),
            &base,
            &pending,
            Evidence::default(),
            &PlanParams::default(),
        )
        .unwrap();
        assert_eq!(plan.dirty, pending);
        assert_eq!(plan.work, Work::Regions(vec![bx(544.0, 0.0, 640.0, 96.0)]));
    }

    #[test]
    fn plan_with_nothing_changed_has_no_work() {
        let base = blank(640, 320, 0, 1);
        let plan = plan_regions(
            Some(&base),
            &base,
            &BTreeSet::new(),
            Evidence::default(),
            &PlanParams::default(),
        )
        .unwrap();
        assert_eq!(plan.work, Work::Nothing);
    }

    #[test]
    fn plan_with_too_many_regions_falls_back_to_full_refresh() {
        let base = blank(1920, 1080, 0, 1);
        let mut current = base.clone();
        for x in [0, 400, 800, 1200, 1600] {
            current = changed(&current, x, 500, 0);
        }
        let plan = plan_regions(
            Some(&base),
            &current,
            &BTreeSet::new(),
            Evidence::default(),
            &PlanParams::default(),
        )
        .unwrap();
        assert_eq!(plan.dirty.len(), 5);
        assert_eq!(
            plan.work,
            Work::FullRefresh(FullRefreshReason::TooManyRegions)
        );
    }

    #[test]
    fn plan_covering_too_much_area_falls_back_to_full_refresh() {
        let base = blank(128, 128, 0, 1);
        let current = changed(&base, 100, 100, 0);
        let plan = plan_regions(
            Some(&base),
            &current,
            &BTreeSet::new(),
            Evidence::default(),
            &PlanParams::default(),
        )
        .unwrap();
        assert_eq!(plan.work, Work::FullRefresh(FullRefreshReason::AreaBudget));
    }

    #[test]
    fn regions_that_merge_are_closed_again_over_new_evidence() {
        let base = blank(1280, 640, 0, 1);
        let current = changed(&changed(&base, 100, 300, 0), 200, 400, 0);
        // `bridge` joins the two regions; only their merged box reaches `support`.
        let bridge = bx(150.0, 340.0, 170.0, 360.0);
        let support = bx(250.0, 250.0, 280.0, 270.0);
        let plan = plan_regions(
            Some(&base),
            &current,
            &BTreeSet::new(),
            Evidence {
                supports: &[bridge, support],
                relations: &[],
            },
            &PlanParams::default(),
        )
        .unwrap();
        let Work::Regions(regions) = plan.work else {
            panic!("expected regions, got {:?}", plan.work);
        };
        assert_eq!(regions.len(), 1);
        assert_eq!(regions, vec![bx(32.0, 224.0, 288.0, 480.0)]);
        assert!(regions[0].contains(&support));
    }

    /// SEC-07 (chapter 08 §8.4): an unshared frame buffer is overwritten; one still held by
    /// another frame is left alone.
    #[test]
    fn sec_07_frame_buffer_is_overwritten_only_when_nothing_else_holds_it() {
        let len = 4 * 9 * 7;
        let mut alone = BgraFrame::new(9, 7, 36, 1, vec![0xAB; len]).unwrap();
        assert!(alone.overwrite());
        assert!(alone.pixels().iter().all(|&b| b == 0));

        let mut first = BgraFrame::new(9, 7, 36, 1, vec![0xAB; len]).unwrap();
        let second = first.clone();
        assert!(!first.overwrite());
        assert_eq!(second.pixels(), &vec![0xAB; len][..]);
    }

    /// Fixed-seed generator (splitmix64): the same frames on every run, with no random crate
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
        fn below(&mut self, n: u64) -> u64 {
            self.next_u64() % n
        }
    }

    /// A base buffer and the byte edits that turn it into the current frame. Offsets are
    /// distinct and every new value differs from the base byte, so each edit is a real change.
    #[derive(Debug, Clone)]
    struct Case {
        width: u32,
        height: u32,
        pad: usize,
        tile: u32,
        base: Vec<u8>,
        edits: Vec<(usize, u8)>,
    }

    impl Case {
        fn stride(&self) -> usize {
            4 * self.width as usize + self.pad
        }

        fn frames(&self) -> (BgraFrame, BgraFrame) {
            let mut current = self.base.clone();
            for &(offset, value) in &self.edits {
                current[offset] = value;
            }
            let frame = |pixels: Vec<u8>| {
                BgraFrame::new(self.width, self.height, self.stride(), 1, pixels).unwrap()
            };
            (frame(self.base.clone()), frame(current))
        }

        /// The dirty set from the byte positions alone, without any tile loop: a changed B,
        /// G or R byte marks its tile; a changed alpha or padding byte marks nothing (OR-08..
        /// OR-10, and OR-48 for every pixel of a small frame).
        fn expected(&self) -> BTreeSet<TileCoord> {
            let stride = self.stride();
            self.edits
                .iter()
                .filter_map(|&(offset, _)| {
                    let (row, col) = (offset / stride, offset % stride);
                    let (x, channel) = (col / 4, col % 4);
                    (col < 4 * self.width as usize && channel < 3)
                        .then(|| TileCoord::new(x as u32 / self.tile, row as u32 / self.tile))
                })
                .collect()
        }
    }

    /// The straightforward reference of chapter 05 §5.5: every pixel of every tile, by explicit
    /// coordinates, comparing its B, G and R bytes. Alpha and padding are never read.
    fn scalar_dirty_tiles(base: &BgraFrame, current: &BgraFrame, tile: u32) -> BTreeSet<TileCoord> {
        let at = |f: &BgraFrame, x: u32, y: u32, c: usize| {
            f.pixels()[y as usize * f.row_stride() + 4 * x as usize + c]
        };
        let mut dirty = BTreeSet::new();
        for row in 0..current.height().div_ceil(tile) {
            for col in 0..current.width().div_ceil(tile) {
                let mut changed = false;
                for y in row * tile..((row + 1) * tile).min(current.height()) {
                    for x in col * tile..((col + 1) * tile).min(current.width()) {
                        for c in 0..3 {
                            changed |= at(base, x, y, c) != at(current, x, y, c);
                        }
                    }
                }
                if changed {
                    dirty.insert(TileCoord::new(col, row));
                }
            }
        }
        dirty
    }

    /// `Err` with the three answers when the optimised diff, the scalar reference and the
    /// expected set do not agree.
    fn check(case: &Case) -> Result<(), String> {
        let (base, current) = case.frames();
        let expected = case.expected();
        let optimised = dirty_tiles(Some(&base), &current, case.tile).unwrap();
        let scalar = scalar_dirty_tiles(&base, &current, case.tile);
        if optimised.full_refresh.is_some() || optimised.tiles != expected || scalar != expected {
            return Err(format!(
                "optimised {:?} (full refresh {:?}), scalar {scalar:?}, expected {expected:?}",
                optimised.tiles, optimised.full_refresh
            ));
        }
        Ok(())
    }

    /// Greedy reduction of a failing case: drops each edit the failure does not need.
    fn reduce(mut case: Case) -> Case {
        let mut i = 0;
        while i < case.edits.len() {
            let mut smaller = case.clone();
            smaller.edits.remove(i);
            if check(&smaller).is_err() {
                case = smaller;
            } else {
                i += 1;
            }
        }
        case
    }

    fn assert_matches_reference(case: Case) {
        if check(&case).is_err() {
            let minimal = reduce(case);
            panic!(
                "differential mismatch, reduced to {minimal:?}: {}",
                check(&minimal).unwrap_err()
            );
        }
    }

    /// `base` with the bytes at `offsets` changed to other values (`rng` picks them).
    fn edited(base: &Case, mut offsets: Vec<usize>, rng: &mut Rng) -> Case {
        offsets.sort_unstable();
        offsets.dedup();
        let mut case = base.clone();
        case.edits = offsets
            .into_iter()
            .map(|o| (o, base.base[o] ^ (1 + rng.below(255) as u8)))
            .collect();
        case
    }

    /// The bytes that edge cases are about: the first and last pixel of the first and last
    /// row, each channel (alpha included), and the first and last padding byte of those rows.
    fn edge_offsets(width: u32, height: u32, pad: usize, stride: usize) -> Vec<usize> {
        let (w, h) = (width as usize, height as usize);
        let mut out = Vec::new();
        for y in [0, h - 1] {
            for x in [0, w - 1] {
                out.extend((0..4).map(|c| y * stride + 4 * x + c));
            }
            if pad > 0 {
                out.extend([y * stride + 4 * w, y * stride + 4 * w + pad - 1]);
            }
        }
        out
    }

    /// UT-08 (OR-48, OR-08..OR-10): the optimised tile comparison of `dirty_tiles` agrees with
    /// a scalar reference on generated frames of odd sizes and strides. Every byte of a small
    /// frame is changed on its own (every channel, alpha, padding, last row and column); big
    /// frames get their edge bytes and sampled bytes; then random multi-byte edits.
    #[test]
    fn ut_08_differential_tile_diff_matches_scalar_reference_on_generated_frames() {
        // (width, height, row padding, tile size)
        const SHAPES: [(u32, u32, usize, u32); 16] = [
            (1, 1, 0, 1),
            (1, 1, 0, 64),
            (1, 9, 0, 4),
            (9, 1, 3, 4),
            (3, 3, 0, 3),
            (5, 4, 0, 2),
            (7, 5, 1, 2),
            (9, 7, 8, 4),
            (17, 3, 2, 16),
            (33, 31, 7, 8),
            (63, 129, 5, 64),
            (64, 64, 0, 64),
            (65, 65, 0, 64),
            (65, 65, 12, 64),
            (130, 3, 2, 64),
            (100, 1, 0, 7),
        ];
        const SEED: u64 = 0x5EED_0008;
        let mut rng = Rng(SEED);
        let mut checked = 0;
        for (width, height, pad, tile) in SHAPES {
            let stride = 4 * width as usize + pad;
            let len = stride * height as usize;
            let base: Vec<u8> = (0..len).map(|_| rng.next_u64() as u8).collect();
            let template = Case {
                width,
                height,
                pad,
                tile,
                base,
                edits: Vec::new(),
            };
            let mut cases = vec![template.clone()];
            if len <= 1_200 {
                cases.extend((0..len).map(|o| edited(&template, vec![o], &mut rng)));
            } else {
                let edges = edge_offsets(width, height, pad, stride);
                cases.extend(edges.iter().map(|&o| edited(&template, vec![o], &mut rng)));
                for _ in 0..150 {
                    let o = rng.below(len as u64) as usize;
                    cases.push(edited(&template, vec![o], &mut rng));
                }
            }
            for _ in 0..100 {
                let n = 2 + rng.below(6) as usize;
                let offsets = (0..n).map(|_| rng.below(len as u64) as usize).collect();
                cases.push(edited(&template, offsets, &mut rng));
            }
            checked += cases.len();
            for case in cases {
                assert_matches_reference(case);
            }
        }
        assert!(checked > 3_000, "only {checked} generated cases");
    }

    #[test]
    fn experimental_full_frame_processes_the_whole_frame_even_with_nothing_changed() {
        // B0 (dossier chapter 09 §9.2): the same planner, forced to the whole frame. Without
        // the switch the same frames plan no work at all.
        let base = blank(640, 320, 0, 1);
        let params = PlanParams {
            experimental_full_frame: true,
            ..PlanParams::default()
        };
        let plan = plan_regions(
            Some(&base),
            &base,
            &BTreeSet::new(),
            Evidence::default(),
            &params,
        )
        .unwrap();
        assert_eq!(
            plan.work,
            Work::FullRefresh(FullRefreshReason::ExperimentalBaseline)
        );
        assert_eq!(
            plan.dirty,
            TileGrid::for_frame(&base, 64).unwrap().all_tiles()
        );
    }

    #[test]
    fn experimental_full_frame_does_not_hide_a_missing_base() {
        // The reason stays the true one: nothing was committed, so the first observation is
        // a full refresh for the reason every planner gives, B0 or not.
        let current = blank(640, 320, 0, 1);
        let params = PlanParams {
            experimental_full_frame: true,
            ..PlanParams::default()
        };
        let plan = plan_regions(
            None,
            &current,
            &BTreeSet::new(),
            Evidence::default(),
            &params,
        )
        .unwrap();
        assert_eq!(
            plan.work,
            Work::FullRefresh(FullRefreshReason::NoCommittedBase)
        );
    }

    #[test]
    fn a_changed_pixel_is_not_planned_as_a_region_under_the_baseline() {
        let base = blank(640, 320, 0, 1);
        let current = changed(&base, 100, 100, 0);
        let params = PlanParams {
            experimental_full_frame: true,
            ..PlanParams::default()
        };
        let plan = plan_regions(
            Some(&base),
            &current,
            &BTreeSet::new(),
            Evidence::default(),
            &params,
        )
        .unwrap();
        assert_eq!(
            plan.work,
            Work::FullRefresh(FullRefreshReason::ExperimentalBaseline)
        );
        // The same change under the incremental planner is one region.
        let incremental = plan_regions(
            Some(&base),
            &current,
            &BTreeSet::new(),
            Evidence::default(),
            &PlanParams::default(),
        )
        .unwrap();
        assert!(matches!(incremental.work, Work::Regions(ref rs) if rs.len() == 1));
    }

    /// A base and a current frame with `n` changed pixels far enough apart to plan `n`
    /// separate regions on a 1920x1080 frame.
    fn frames_with_regions(n: usize) -> (BgraFrame, BgraFrame) {
        let base = blank(1920, 1080, 0, 1);
        let mut current = base.clone();
        for i in 0..n {
            current = changed(&current, 400 * i as u32, 500, 0);
        }
        (base, current)
    }

    fn adaptive(min_regions: usize) -> PlanParams {
        PlanParams {
            adaptive_min_regions: min_regions,
            ..PlanParams::default()
        }
    }

    fn plan_of(base: &BgraFrame, current: &BgraFrame, params: &PlanParams) -> RegionPlan {
        plan_regions(
            Some(base),
            current,
            &BTreeSet::new(),
            Evidence::default(),
            params,
        )
        .unwrap()
    }

    /// INC-07 (Algorithm 1, adaptive cost rule): from `adaptive_min_regions` regions on, the
    /// plan is the full refresh, with its own reason. The regional plan costs one inference
    /// per region and the full frame costs one (dossier S0 section 13), so it never wins with two.
    #[test]
    fn inc_07_two_regions_take_the_full_refresh_under_the_adaptive_rule() {
        let (base, current) = frames_with_regions(2);
        let plan = plan_of(&base, &current, &adaptive(2));
        assert_eq!(
            plan.work,
            Work::FullRefresh(FullRefreshReason::AdaptiveCost)
        );
        // The rule only chooses earlier: what a failed observation keeps pending is the same
        // set of changed tiles as the regional plan's.
        let regional = plan_of(&base, &current, &adaptive(0));
        assert_eq!(plan.dirty, regional.dirty);
        assert!(matches!(regional.work, Work::Regions(ref rs) if rs.len() == 2));
    }

    #[test]
    fn inc_07_one_region_stays_regional_under_the_adaptive_rule() {
        let (base, current) = frames_with_regions(1);
        let plan = plan_of(&base, &current, &adaptive(2));
        assert!(matches!(plan.work, Work::Regions(ref rs) if rs.len() == 1));
    }

    #[test]
    fn inc_07_the_rule_is_off_when_its_threshold_is_zero() {
        // Regression guard: a plan built without the rule (the default of `PlanParams`) keeps
        // every regional plan it made before the rule existed.
        assert_eq!(PlanParams::default().adaptive_min_regions, 0);
        let (base, current) = frames_with_regions(3);
        let plan = plan_of(&base, &current, &PlanParams::default());
        assert!(matches!(plan.work, Work::Regions(ref rs) if rs.len() == 3));
    }

    #[test]
    fn inc_07_the_threshold_is_the_number_of_regions_that_triggers_the_full_refresh() {
        let (base, two) = frames_with_regions(2);
        let (_, three) = frames_with_regions(3);
        assert!(matches!(
            plan_of(&base, &two, &adaptive(3)).work,
            Work::Regions(ref rs) if rs.len() == 2
        ));
        assert_eq!(
            plan_of(&base, &three, &adaptive(3)).work,
            Work::FullRefresh(FullRefreshReason::AdaptiveCost)
        );
    }

    #[test]
    fn inc_07_the_hard_budgets_keep_their_own_reasons() {
        // The rule only triggers the fallback earlier: a plan the budgets already send to
        // the full refresh keeps the budget's reason, so the record says which guard fired.
        let (base, current) = frames_with_regions(5);
        let plan = plan_of(&base, &current, &adaptive(2));
        assert_eq!(
            plan.work,
            Work::FullRefresh(FullRefreshReason::TooManyRegions)
        );
        let small = blank(128, 128, 0, 1);
        let big_change = changed(&small, 100, 100, 0);
        assert_eq!(
            plan_of(&small, &big_change, &adaptive(1)).work,
            Work::FullRefresh(FullRefreshReason::AreaBudget)
        );
    }

    #[test]
    fn inc_07_the_baseline_and_a_missing_base_keep_their_reasons() {
        let (base, current) = frames_with_regions(2);
        let b0 = PlanParams {
            adaptive_min_regions: 2,
            experimental_full_frame: true,
            ..PlanParams::default()
        };
        assert_eq!(
            plan_of(&base, &current, &b0).work,
            Work::FullRefresh(FullRefreshReason::ExperimentalBaseline)
        );
        let first = plan_regions(
            None,
            &current,
            &BTreeSet::new(),
            Evidence::default(),
            &adaptive(2),
        )
        .unwrap();
        assert_eq!(
            first.work,
            Work::FullRefresh(FullRefreshReason::NoCommittedBase)
        );
    }

    #[test]
    fn inc_07_nothing_to_do_stays_nothing_under_the_adaptive_rule() {
        let base = blank(640, 320, 0, 1);
        let plan = plan_of(&base, &base, &adaptive(1));
        assert_eq!(plan.work, Work::Nothing);
    }

    #[test]
    fn inc_07_the_decision_does_not_depend_on_the_order_of_the_pending_tiles() {
        let (base, current) = frames_with_regions(1);
        let a: BTreeSet<TileCoord> = [TileCoord::new(20, 2), TileCoord::new(2, 12)]
            .into_iter()
            .collect();
        let b: BTreeSet<TileCoord> = a.iter().rev().copied().collect();
        let go = |pending: &BTreeSet<TileCoord>| {
            plan_regions(
                Some(&base),
                &current,
                pending,
                Evidence::default(),
                &adaptive(2),
            )
            .unwrap()
        };
        assert_eq!(go(&a), go(&b));
        // The pending tiles plus the changed one make three regions: the rule fires.
        assert_eq!(
            go(&a).work,
            Work::FullRefresh(FullRefreshReason::AdaptiveCost)
        );
    }

    #[test]
    fn full_refresh_reasons_have_the_names_of_the_research_record() {
        assert_eq!(
            FullRefreshReason::ExperimentalBaseline.as_str(),
            "experimental_baseline"
        );
        assert_eq!(FullRefreshReason::AreaBudget.as_str(), "area_budget");
        assert_eq!(FullRefreshReason::AdaptiveCost.as_str(), "adaptive_cost");
        assert_eq!(
            FullRefreshReason::NoCommittedBase.as_str(),
            "no_committed_base"
        );
    }

    /// IDX-11 (RF-03, ADR-0027): a chain of supports closes completely, or it runs out of
    /// rounds and the caller takes the full refresh. No order publishes a partial closure.
    /// A1 is not indexed in this increment: this is the closure that an index must keep.
    #[test]
    fn idx_11_a_support_chain_closes_or_exhausts_its_budget_never_partially() {
        let r = bx(0.0, 0.0, 1.0, 1.0);
        let a = bx(0.5, 0.0, 2.0, 1.0);
        let b = bx(1.5, 0.0, 3.0, 1.0);
        let bounds = bx(-10.0, -10.0, 10.0, 10.0);
        let whole = bx(0.0, 0.0, 3.0, 1.0);
        for order in [[a, b], [b, a]] {
            assert_eq!(
                halo_close(r, &order, &[], bounds, 3).unwrap(),
                Closure::Closed(whole)
            );
            match halo_close(r, &order, &[], bounds, 2).unwrap() {
                Closure::Closed(closed) => {
                    assert_eq!(closed, whole, "a closure is the whole chain");
                }
                Closure::BudgetExhausted => {}
            }
        }
        assert_eq!(
            halo_close(r, &[b, a], &[], bounds, 2).unwrap(),
            Closure::BudgetExhausted
        );
    }

    /// IDX-12 (RF-03, ADR-0027): a region in the gap between the two ends of a relation does not
    /// touch either end, so the relation stays inactive, however large the envelope of its ends.
    /// A region that touches one end brings in both.
    #[test]
    fn idx_12_a_region_in_the_gap_between_relation_ends_does_not_activate_it() {
        let a = bx(0.0, 0.0, 10.0, 10.0);
        let b = bx(100.0, 0.0, 110.0, 10.0);
        let gap = bx(50.0, 0.0, 60.0, 10.0);
        let bounds = bx(-10.0, -10.0, 200.0, 50.0);
        assert!(a.union(&b).overlaps(&gap));
        assert_eq!(
            halo_close(gap, &[], &[(a, b)], bounds, 2).unwrap(),
            Closure::Closed(gap)
        );
        let touching = bx(5.0, 5.0, 6.0, 6.0);
        match halo_close(touching, &[], &[(a, b)], bounds, 2).unwrap() {
            Closure::Closed(closed) => assert!(closed.contains(&a) && closed.contains(&b)),
            Closure::BudgetExhausted => panic!("one touched end closes within two rounds"),
        }
    }
}

#[cfg(test)]
mod freshness_tests {
    use super::*;

    /// OR-12's global geometry rule is unchanged even when the requested region is tiny.
    #[test]
    fn ut_60_dimensions_force_a_whole_frame_diff_under_a_regional_comparison() {
        let base = BgraFrame::new(130, 129, 520, 1, vec![0; 520 * 129]).unwrap();
        for (width, height, epoch) in [(131, 129, 1), (130, 130, 1), (130, 129, 2)] {
            let frame = BgraFrame::new(
                width,
                height,
                width as usize * 4,
                epoch,
                vec![0; width as usize * height as usize * 4],
            )
            .unwrap();
            let region = BBox::new(0.0, 0.0, 1.0, 1.0).unwrap();
            let diff = dirty_tiles_in_regions(Some(&base), &frame, 64, &[region]).unwrap();
            assert_eq!(diff.full_refresh, Some(FullRefreshReason::GeometryChanged));
            assert_eq!(
                diff.tiles,
                TileGrid::for_frame(&frame, 64).unwrap().all_tiles()
            );
        }
    }
}
