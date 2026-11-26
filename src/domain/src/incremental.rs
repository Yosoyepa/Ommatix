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

use rstar::primitives::{GeomWithData, Rectangle};
use rstar::{AABB, RTree};
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

/// The support-aware closure (ADR-0045): what the last snapshot's evidence is, by how a
/// region may meet it. Passed beside [`Evidence`] to [`plan_regions_with`]; `None` there is
/// today's closure over every support, bit for bit.
///
/// The three lists, as multisets, are `Evidence::supports`: the caller splits the same boxes.
///
/// - `cut_prone`: boxes a perception stage may report in pieces when a region cuts them
///   (detector boxes that are not surfaces, OCR words). The closure always takes them whole.
/// - `surfaces`: detector boxes the detector reports whole or not at all for any area it is
///   asked about ([`VisionDetector::surfaces`](../../ommatix_application) in the
///   application). One is *spanned*, not closed over, while its edge band is unchanged
///   (rule R2 of ADR-0045).
/// - `whole_on_read`: accessibility boxes. The provider returns every element whose box meets
///   the regions it reads, whole, and the engine replaces every held element that meets them,
///   so no region can cut one (rule R1): they are never closed over.
#[derive(Debug, Clone, Copy, Default)]
pub struct Spanning<'a> {
    pub cut_prone: &'a [BBox],
    pub surfaces: &'a [BBox],
    pub whole_on_read: &'a [BBox],
    /// The band, in pixels, inside and outside a surface's edge in which a changed pixel can
    /// change the surface's box as the detector reports it (the flat-region detector: its
    /// `nested_inset + 1`).
    pub band_px: u32,
}

/// What a box of the last evidence is to the support-aware closure (ADR-0045), as the plan
/// trace names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SupportClass {
    /// A box a region must contain whole or not meet: closed over.
    CutProne,
    /// A detector surface: closed over unless spanned.
    Surface,
    /// An accessibility box, re-read whole: never closed over.
    WholeOnRead,
}

impl SupportClass {
    /// The class's name on the plan trace.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CutProne => "cut_prone",
            Self::Surface => "surface",
            Self::WholeOnRead => "whole_on_read",
        }
    }
}

/// Which closure made the plan that ran.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlanChoice {
    /// Today's closure over every support (no [`Spanning`], or rule R4 gave way to it).
    #[default]
    Classic,
    /// The support-aware closure of ADR-0045.
    Spanning,
}

impl PlanChoice {
    /// The choice's name on the plan trace.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Classic => "classic",
            Self::Spanning => "spanning",
        }
    }
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
    plan_regions_recorded(base, current, pending, evidence, None, params, None)
}

/// [`plan_regions`] and the numbers behind its decision (a [`PlanTrace`]). The plan is
/// exactly the one `plan_regions` returns for the same input: both run the same body, and
/// the trace only reads what the body computes.
pub fn plan_regions_traced(
    base: Option<&BgraFrame>,
    current: &BgraFrame,
    pending: &BTreeSet<TileCoord>,
    evidence: Evidence<'_>,
    params: &PlanParams,
) -> Result<(RegionPlan, PlanTrace), IncrementalError> {
    plan_regions_with_traced(base, current, pending, evidence, None, params)
}

/// [`plan_regions`] with the support-aware closure of ADR-0045 when `spanning` is given
/// (rules R1 to R4): an accessibility box is never closed over, a detector surface whose edge
/// band is unchanged is spanned instead of closed over, a relation whose control is such a
/// box is ignored, and a support-aware plan that is a full refresh gives way to the classic
/// plan, so the full refreshes are a subset of today's, with today's reasons. With `None`
/// it is [`plan_regions`].
pub fn plan_regions_with(
    base: Option<&BgraFrame>,
    current: &BgraFrame,
    pending: &BTreeSet<TileCoord>,
    evidence: Evidence<'_>,
    spanning: Option<Spanning<'_>>,
    params: &PlanParams,
) -> Result<RegionPlan, IncrementalError> {
    plan_regions_recorded(base, current, pending, evidence, spanning, params, None)
}

/// [`plan_regions_with`] and its [`PlanTrace`]; the plan is the same.
pub fn plan_regions_with_traced(
    base: Option<&BgraFrame>,
    current: &BgraFrame,
    pending: &BTreeSet<TileCoord>,
    evidence: Evidence<'_>,
    spanning: Option<Spanning<'_>>,
    params: &PlanParams,
) -> Result<(RegionPlan, PlanTrace), IncrementalError> {
    let mut trace = PlanTrace::default();
    let plan = plan_regions_recorded(
        base,
        current,
        pending,
        evidence,
        spanning,
        params,
        Some(&mut trace),
    )?;
    trace.dirty_tiles = plan.dirty.len();
    // The reason of the plan the trace describes: the plan that ran, or the support-aware
    // attempt when R4 gave way to the classic plan (whose own reason is in `spanning`).
    let traced_reason = match plan.work {
        Work::FullRefresh(reason) => Some(reason),
        Work::Nothing | Work::Regions(_) => None,
    };
    trace.decided_by = match trace.spanning.as_ref() {
        Some(s) if s.classic.is_some() => s.attempt_reason,
        _ => traced_reason,
    };
    if trace.decided_by == Some(FullRefreshReason::AreaBudget) {
        // The first of the largest: the region most of the sum came from.
        let mut largest: Option<(usize, f64)> = None;
        for (i, region) in trace.regions.iter().enumerate() {
            if largest.is_none_or(|(_, area)| region.area_px > area) {
                largest = Some((i, region.area_px));
            }
        }
        trace.decisive_region = largest.map(|(i, _)| i);
    }
    Ok((plan, trace))
}

/// What the planner saw when it chose the work of one observation (the research record of
/// `demo --plan-trace`). **Counts and areas only**: no pixel, label, coordinate or box, so it
/// is as safe to print as the plan itself. Areas are in pixels of the frame, as `f64` like
/// the areas the budget compares. **Every measured area is at most the frame's**, and each
/// field says whether it is the area of a *union* (a pixel counted once, however many boxes
/// cover it) or a *sum* of areas of boxes that cannot overlap. It never reaches an MCP
/// client.
///
/// What each field holds depends on how far the planner got: a full refresh decided before
/// any region exists (no committed base, a geometry change, the baseline) has no components
/// or regions, and the sum is filled only once the regions were merged.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PlanTrace {
    /// Area of the whole frame.
    pub frame_area_px: f64,
    pub tile_size: u32,
    /// Tiles the comparison marked as changed; every tile when there was no base to compare
    /// with or its geometry differed.
    pub changed_tiles: usize,
    /// Pending tiles of the frame's grid (from a failed observation), changed or not.
    pub pending_tiles: usize,
    /// The plan's dirty tiles: changed plus pending, or the whole grid for a full refresh
    /// decided before the regions.
    pub dirty_tiles: usize,
    /// One entry per 8-connected component, in the planner's order, as it stood after the
    /// first closure. A closure that ran out of rounds leaves no entry.
    pub components: Vec<ComponentTrace>,
    /// The regions after merging and the last closure: the plan's own, in its order.
    pub regions: Vec<RegionTrace>,
    /// The sum of the regions' areas, which the area budget compares. The regions never
    /// overlap, so the sum is also their union.
    pub covered_area_px: f64,
    /// `max_area_fraction` of the frame (a threshold, not a measured area; at most the frame
    /// while the fraction is at most 1, as the configuration requires).
    pub budget_area_px: f64,
    /// `max_regions` of the parameters.
    pub max_regions: usize,
    /// The check that made the plan a full refresh (its reason), or `None` when the plan is
    /// idle or regional.
    pub decided_by: Option<FullRefreshReason>,
    /// For `area_budget`: the index in `regions` of the largest region (the first on a tie).
    pub decisive_region: Option<usize>,
    /// Which closure made the plan that ran (ADR-0045): `Classic` when no [`Spanning`] was
    /// given, when the plan was decided before any region (no base, a geometry change, the
    /// baseline, nothing to do), or when rule R4 gave way to the classic plan.
    pub chosen: PlanChoice,
    /// What the support-aware closure did, when it ran. When it ran and R4 gave way to the
    /// classic plan, `components`, `regions`, `covered_area_px`, `decided_by` and
    /// `decisive_region` describe the support-aware attempt, and
    /// [`SpanningTrace::classic`] the plan that ran.
    pub spanning: Option<SpanningTrace>,
}

/// The support-aware closure of one plan (ADR-0045): counts and areas only.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SpanningTrace {
    /// Distinct surfaces the detector declared in the last evidence.
    pub surfaces_declared: usize,
    /// Of those, the ones spanned (not closed over) in the attempt the trace describes.
    pub surfaces_spanned: usize,
    /// The area of the largest spanned surface (one box); `0` when none was spanned.
    pub largest_spanned_px: f64,
    /// Accessibility boxes that a final region overlaps without containing: the boxes R1 lets
    /// a region cut, because the provider re-reads them whole.
    pub whole_on_read_cut: usize,
    /// The area of the largest of those (one box); `0` when there is none.
    pub largest_whole_on_read_cut_px: f64,
    /// Relations the closure ignored because their control is a spanned surface or an
    /// accessibility box (rule R3).
    pub relations_ignored: usize,
    /// The reason the support-aware attempt was a full refresh, `None` when it was regional.
    pub attempt_reason: Option<FullRefreshReason>,
    /// The classic plan, when R4 computed it because the attempt was a full refresh: it is
    /// the plan that ran.
    pub classic: Option<ClassicTrace>,
}

/// The classic plan rule R4 computed and ran.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ClassicTrace {
    /// Its reason, `None` when it was regional.
    pub full_refresh_reason: Option<FullRefreshReason>,
    /// The regions its closure produced (the number the region budget compared); `0` when
    /// its closure ran out of rounds.
    pub region_count: usize,
    /// The sum of their areas (the sum the area budget compared; they never overlap).
    pub covered_area_px: f64,
}

/// One connected component of dirty tiles and what the closure did to its region.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ComponentTrace {
    pub tiles: usize,
    /// The area of the box around the component's tiles, before the halo (one box).
    pub tight_area_px: f64,
    /// The area of that box after the halo, clipped to the frame (one box): the region
    /// **before** the closure.
    pub padded_area_px: f64,
    /// The area of the region **after** the closure over the last snapshot's evidence (one
    /// box).
    pub closed_area_px: f64,
    /// The largest evidence box (a support, or a member of a relation) the closed region
    /// overlaps; `0` when it touches none. Under the classic closure it contains that box; a
    /// box that large next to a small `padded_area_px` is what makes the closure, and not the
    /// change, big. Under the support-aware closure it may be a spanned surface or an
    /// accessibility box the region overlaps without containing.
    pub largest_support_px: f64,
    /// The class of that box (ADR-0045); `None` when it touches none, and under the classic
    /// closure without [`Spanning`], which has no classes.
    pub largest_support_class: Option<SupportClass>,
}

/// One region of the final plan.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct RegionTrace {
    /// The area of the region (one box).
    pub area_px: f64,
    /// The components whose padded region lies inside this one.
    pub components: usize,
    /// What the region is made of before any closure or merge: the area of the **union** of
    /// those components' padded boxes. Halos of neighbouring components overlap, and an
    /// overlap is counted once; it never exceeds `area_px`.
    pub pre_closure_area_px: f64,
    /// As [`ComponentTrace::largest_support_px`], for the final region.
    pub largest_support_px: f64,
    /// As [`ComponentTrace::largest_support_class`], for the final region.
    pub largest_support_class: Option<SupportClass>,
}

/// The largest box of the evidence that `region` overlaps: a support, or a member of a
/// relation one of whose members it overlaps (the boxes the closure unions). `0` for none.
/// With `classed` (the support-aware closure), every box of the evidence counts, closed over
/// or not, and the answer carries its class.
fn largest_evidence(
    region: &BBox,
    supports: &[BBox],
    relations: &[(BBox, BBox)],
    classed: Option<&Classed>,
) -> (f64, Option<SupportClass>) {
    let related = relations
        .iter()
        .filter(|(a, b)| region.overlaps(a) || region.overlaps(b))
        .flat_map(|(a, b)| [*a, *b]);
    let Some(classed) = classed else {
        let area = supports
            .iter()
            .filter(|s| region.overlaps(s))
            .map(BBox::area)
            .chain(related.map(|b| b.area()))
            .fold(0.0, f64::max);
        return (area, None);
    };
    let mut best: Option<(f64, SupportClass)> = None;
    let mut consider = |area: f64, class: SupportClass| {
        // The larger area, then the class later in the list: a total order, so the answer
        // does not depend on the order of the evidence.
        if best.is_none_or(|(a, c)| area.total_cmp(&a).then(class.cmp(&c)).is_gt()) {
            best = Some((area, class));
        }
    };
    for (b, class) in &classed.all {
        if region.overlaps(b) {
            consider(b.area(), *class);
        }
    }
    for b in related {
        consider(b.area(), classed.class_of(&b));
    }
    best.map_or((0.0, None), |(area, class)| (area, Some(class)))
}

/// Every box of the support-aware evidence with its class, and sorted lists to look a box up.
struct Classed {
    all: Vec<(BBox, SupportClass)>,
    cut_prone: Vec<BBox>,
    surfaces: Vec<BBox>,
    whole_on_read: Vec<BBox>,
}

fn sorted(boxes: &[BBox]) -> Vec<BBox> {
    let mut out = boxes.to_vec();
    out.sort_by(BBox::cmp_corners);
    out.dedup();
    out
}

fn holds(sorted: &[BBox], b: &BBox) -> bool {
    sorted
        .binary_search_by(|probe| probe.cmp_corners(b))
        .is_ok()
}

impl Classed {
    fn new(spanning: &Spanning<'_>) -> Self {
        let mut all = Vec::new();
        for (list, class) in [
            (spanning.cut_prone, SupportClass::CutProne),
            (spanning.surfaces, SupportClass::Surface),
            (spanning.whole_on_read, SupportClass::WholeOnRead),
        ] {
            all.extend(list.iter().map(|b| (*b, class)));
        }
        Self {
            all,
            cut_prone: sorted(spanning.cut_prone),
            surfaces: sorted(spanning.surfaces),
            whole_on_read: sorted(spanning.whole_on_read),
        }
    }

    /// A box's class; a box in no list (a relation member fusion made) is cut-prone, the
    /// conservative answer. A box in several lists takes the first of cut-prone, surface.
    fn class_of(&self, b: &BBox) -> SupportClass {
        if holds(&self.cut_prone, b) {
            SupportClass::CutProne
        } else if holds(&self.surfaces, b) {
            SupportClass::Surface
        } else if holds(&self.whole_on_read, b) {
            SupportClass::WholeOnRead
        } else {
            SupportClass::CutProne
        }
    }
}

/// The area of the union of `boxes`: a pixel covered by several boxes counts once. A sweep
/// over the boxes' left and right edges with a segment tree over their distinct `y` values,
/// `O(n log n)`; used only to describe a plan, never to make one.
fn union_area(boxes: &[BBox]) -> f64 {
    let mut ys: Vec<f64> = boxes.iter().flat_map(|b| [b.y0(), b.y1()]).collect();
    ys.sort_by(f64::total_cmp);
    ys.dedup();
    if ys.len() < 2 {
        return 0.0;
    }
    let y_index = |y: f64| {
        ys.binary_search_by(|probe| probe.total_cmp(&y))
            .expect("a box edge is one of the distinct y values")
    };
    // (x, +1 for a left edge and -1 for a right edge, first and last elementary interval).
    let mut edges: Vec<(f64, i32, usize, usize)> = Vec::with_capacity(2 * boxes.len());
    for b in boxes {
        let (lo, hi) = (y_index(b.y0()), y_index(b.y1()) - 1);
        edges.push((b.x0(), 1, lo, hi));
        edges.push((b.x1(), -1, lo, hi));
    }
    edges.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    let intervals = ys.len() - 1;
    let mut sweep = CoverTree {
        ys: &ys,
        count: vec![0; 4 * intervals],
        length: vec![0.0; 4 * intervals],
    };
    let mut area = 0.0;
    let mut previous_x = edges[0].0;
    for (x, delta, lo, hi) in edges {
        area += sweep.length[1] * (x - previous_x);
        previous_x = x;
        sweep.update(1, (0, intervals - 1), (lo, hi), delta);
    }
    area
}

/// The covered length of a set of intervals of `ys` under insertions and removals.
struct CoverTree<'a> {
    ys: &'a [f64],
    /// How many inserted intervals cover each node's whole range.
    count: Vec<i32>,
    /// The covered length inside each node's range.
    length: Vec<f64>,
}

impl CoverTree<'_> {
    /// Adds `delta` to the cover of the elementary intervals `target.0..=target.1` (interval
    /// `i` is `ys[i]..ys[i + 1]`), inside `node`, which stands for `range`.
    fn update(&mut self, node: usize, range: (usize, usize), target: (usize, usize), delta: i32) {
        if target.0 > range.1 || target.1 < range.0 {
            return;
        }
        if target.0 <= range.0 && range.1 <= target.1 {
            self.count[node] += delta;
        } else {
            let middle = usize::midpoint(range.0, range.1);
            self.update(2 * node, (range.0, middle), target, delta);
            self.update(2 * node + 1, (middle + 1, range.1), target, delta);
        }
        self.length[node] = if self.count[node] > 0 {
            self.ys[range.1 + 1] - self.ys[range.0]
        } else if range.0 == range.1 {
            0.0
        } else {
            self.length[2 * node] + self.length[2 * node + 1]
        };
    }
}

/// For each of `regions` (disjoint boxes), the `padded` boxes that lie inside it. A padded box
/// lies inside the region its closure grew into, and a box with area has its centre strictly
/// inside exactly one of two regions that do not overlap, so an R*-tree on the regions finds
/// it without comparing every pair.
fn members_of(regions: &[BBox], padded: &[BBox]) -> Vec<Vec<BBox>> {
    let tree = RTree::bulk_load(
        regions
            .iter()
            .enumerate()
            .map(|(i, r)| {
                GeomWithData::new(
                    Rectangle::from_corners([r.x0(), r.y0()], [r.x1(), r.y1()]),
                    i,
                )
            })
            .collect(),
    );
    let mut members = vec![Vec::new(); regions.len()];
    for b in padded {
        let (cx, cy) = b.center();
        let at = AABB::from_point([cx, cy]);
        if let Some(entry) = tree
            .locate_in_envelope_intersecting(at)
            .find(|e| regions[e.data].contains(b))
        {
            members[entry.data].push(*b);
        }
    }
    members
}
/// One closure of the dirty tiles over a set of supports and relations, and the budget
/// checks: the plan's work, and the merged regions the checks looked at (empty when the
/// closure ran out of rounds).
struct Decided {
    work: Work,
    closed: Vec<BBox>,
}

/// Algorithm 1 from the dirty tiles on: components, halo, closure, merge, budgets. With a
/// `trace` it replaces the trace's components, regions and covered area by this closure's.
// One algorithm of the thesis (Algorithm 1) in one place, as the oracle has it.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn close_and_decide(
    grid: &TileGrid,
    bounds: BBox,
    dirty: &BTreeSet<TileCoord>,
    supports: &[BBox],
    relations: &[(BBox, BBox)],
    classed: Option<&Classed>,
    params: &PlanParams,
    mut trace: Option<&mut PlanTrace>,
) -> Result<Decided, IncrementalError> {
    let full = |reason, closed| Decided {
        work: Work::FullRefresh(reason),
        closed,
    };
    if let Some(t) = trace.as_deref_mut() {
        t.components.clear();
        t.regions.clear();
        t.covered_area_px = 0.0;
    }
    let mut regions = Vec::new();
    // The padded regions, kept only to say which final region each component ended in.
    let mut padded_boxes = Vec::new();
    for component in connected_components(dirty) {
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
            supports,
            relations,
            bounds,
            params.max_closure_rounds,
        )? {
            Closure::Closed(region) => {
                if let Some(t) = trace.as_deref_mut() {
                    let (largest, class) = largest_evidence(&region, supports, relations, classed);
                    t.components.push(ComponentTrace {
                        tiles: component.len(),
                        tight_area_px: tight.area(),
                        padded_area_px: padded.area(),
                        closed_area_px: region.area(),
                        largest_support_px: largest,
                        largest_support_class: class,
                    });
                    padded_boxes.push(padded);
                }
                regions.push(region);
            }
            Closure::BudgetExhausted => {
                return Ok(full(FullRefreshReason::ClosureBudget, Vec::new()));
            }
        }
    }
    // Merging can make a region touch new evidence, so close and merge until both hold.
    loop {
        let merged = merge_regions(&regions);
        let mut closed = Vec::with_capacity(merged.len());
        for region in &merged {
            match halo_close(
                *region,
                supports,
                relations,
                bounds,
                params.max_closure_rounds,
            )? {
                Closure::Closed(r) => closed.push(r),
                Closure::BudgetExhausted => {
                    return Ok(full(FullRefreshReason::ClosureBudget, Vec::new()));
                }
            }
        }
        if closed == merged {
            regions = merged;
            break;
        }
        regions = closed;
    }

    if let Some(t) = trace {
        t.regions = regions
            .iter()
            .zip(members_of(&regions, &padded_boxes))
            .map(|(region, members)| {
                let (largest, class) = largest_evidence(region, supports, relations, classed);
                RegionTrace {
                    area_px: region.area(),
                    components: members.len(),
                    // The union lies inside the region; the `min` only guards the last bit of
                    // rounding of a sum of differences.
                    pre_closure_area_px: union_area(&members).min(region.area()),
                    largest_support_px: largest,
                    largest_support_class: class,
                }
            })
            .collect();
        t.covered_area_px = regions.iter().map(BBox::area).sum();
    }
    if regions.len() > params.max_regions {
        return Ok(full(FullRefreshReason::TooManyRegions, regions));
    }
    let covered: f64 = regions.iter().map(BBox::area).sum();
    if covered > params.max_area_fraction * bounds.area() {
        return Ok(full(FullRefreshReason::AreaBudget, regions));
    }
    if params.adaptive_min_regions > 0 && regions.len() >= params.adaptive_min_regions {
        return Ok(full(FullRefreshReason::AdaptiveCost, regions));
    }
    Ok(Decided {
        work: Work::Regions(regions.clone()),
        closed: regions,
    })
}

/// A coordinate of a box that lies inside a validated frame, as whole pixels.
// The caller has checked the box against the frame's bounds: the value is finite and in
// `0..=20_000_000` (`MAX_FRAME_PIXELS`), so the cast neither truncates nor wraps, and any
// band of a `u32` added to it stays far inside `i64`.
#[allow(clippy::cast_possible_truncation)]
fn whole(v: f64) -> i64 {
    v as i64
}

/// A relation of the evidence: a control and its external label.
type Relation = (BBox, BBox);

/// Pixel rectangle `[x0, x1) x [y0, y1)` as `i64`.
type PixelBox = (i64, i64, i64, i64);

/// The pixels of `s` shrunk by `band` on every side, rounding inwards; `None` when nothing is
/// left (R2 (iv)).
fn shrunk(s: &BBox, band: u32) -> Option<PixelBox> {
    let b = i64::from(band);
    let r = (
        whole(s.x0().ceil()) + b,
        whole(s.y0().ceil()) + b,
        whole(s.x1().floor()) - b,
        whole(s.y1().floor()) - b,
    );
    (r.0 < r.2 && r.1 < r.3).then_some(r)
}

/// The pixels a box covers, rounding outwards (what a stage reads for a region).
fn covered_pixels(r: &BBox) -> PixelBox {
    (
        whole(r.x0().floor()),
        whole(r.y0().floor()),
        whole(r.x1().ceil()),
        whole(r.y1().ceil()),
    )
}

fn pixel_box_contains(outer: PixelBox, inner: PixelBox) -> bool {
    outer.0 <= inner.0 && outer.1 <= inner.1 && outer.2 >= inner.2 && outer.3 >= inner.3
}

/// Whether some pixel of row `y`, columns `[x0, x1)`, differs from the base in B, G or R
/// (exact, as [`dirty_tiles`]). The caller clipped the columns and the row to the frame.
fn row_differs(base: &BgraFrame, current: &BgraFrame, y: i64, x0: i64, x1: i64) -> bool {
    let (Ok(y), Ok(x0), Ok(x1)) = (u32::try_from(y), u32::try_from(x0), u32::try_from(x1)) else {
        return true; // outside the frame: the conservative answer, never reached
    };
    if x0 >= x1 {
        return false;
    }
    let (old, _) = base.row(y, x0, x1).as_chunks::<BPP>();
    let (new, _) = current.row(y, x0, x1).as_chunks::<BPP>();
    old.iter().zip(new).any(|(o, n)| o[..3] != n[..3])
}

/// Rule R2 (i) to (iv) of ADR-0045: whether the surface `s`, as the detector reported it on
/// `base`, still has the same box on `current` whatever changed strictly inside it. It does
/// when it does not touch the frame's edge (i), no pixel of its band `s.pad(band) \
/// s.shrink(band)`, clipped to the frame, differs from the base (ii), no pending tile meets
/// the outer box without lying inside the inner one (iii), and the inner box is not empty
/// (iv). Pure in its inputs; `s` lies inside the frame, which has the base's geometry.
pub fn band_unchanged(
    s: &BBox,
    base: &BgraFrame,
    current: &BgraFrame,
    grid: &TileGrid,
    pending: &BTreeSet<TileCoord>,
    band: u32,
) -> bool {
    let (w, h) = (i64::from(current.width()), i64::from(current.height()));
    if (base.width(), base.height()) != (current.width(), current.height())
        || !current.bounds().contains(s)
    {
        return false;
    }
    let b = i64::from(band);
    let (sx0, sy0, sx1, sy1) = covered_pixels(s);
    if sx0 <= 0 || sy0 <= 0 || sx1 >= w || sy1 >= h {
        return false; // (i)
    }
    let Some(inner) = shrunk(s, band) else {
        return false; // (iv)
    };
    let outer = (
        (sx0 - b).max(0),
        (sy0 - b).max(0),
        (sx1 + b).min(w),
        (sy1 + b).min(h),
    );
    // (ii): every row of the outer box, minus the inner box on the rows it spans.
    for y in outer.1..outer.3 {
        let changed = if (inner.1..inner.3).contains(&y) {
            row_differs(base, current, y, outer.0, inner.0)
                || row_differs(base, current, y, inner.2, outer.2)
        } else {
            row_differs(base, current, y, outer.0, outer.2)
        };
        if changed {
            return false;
        }
    }
    // (iii)
    #[allow(clippy::cast_precision_loss)] // pixel coordinates far below 2^53
    let to_box = |p: PixelBox| BBox::new(p.0 as f64, p.1 as f64, p.2 as f64, p.3 as f64);
    let (Ok(outer_box), Ok(inner_box)) = (to_box(outer), to_box(inner)) else {
        return false;
    };
    !pending
        .iter()
        .filter(|&&t| grid.contains(t))
        .map(|&t| grid.tile_box(t))
        .any(|tile| tile.overlaps(&outer_box) && !inner_box.contains(&tile))
}

/// The support-aware plan (rules R1 to R4 of ADR-0045) for the dirty tiles of a frame that
/// has a committed base of its own geometry.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn plan_spanning(
    grid: &TileGrid,
    bounds: BBox,
    base: &BgraFrame,
    current: &BgraFrame,
    dirty: &BTreeSet<TileCoord>,
    pending: &BTreeSet<TileCoord>,
    evidence: Evidence<'_>,
    spanning: Spanning<'_>,
    params: &PlanParams,
    trace: Option<&mut PlanTrace>,
) -> Result<Work, IncrementalError> {
    let in_bounds = |b: &BBox| bounds.contains(b);
    if !spanning
        .cut_prone
        .iter()
        .chain(spanning.surfaces)
        .chain(spanning.whole_on_read)
        .all(in_bounds)
    {
        return Err(IncrementalError::SupportOutOfBounds);
    }
    let classed = Classed::new(&spanning);
    // R2 (i) to (iv), fixed before any closure: pure in the frames, the pending tiles, the
    // boxes and the band. A surface that is also a cut-prone box is closed over as one.
    let surfaces = classed.surfaces.clone();
    let mut spanned: Vec<bool> = surfaces
        .iter()
        .map(|s| {
            !holds(&classed.cut_prone, s)
                && band_unchanged(s, base, current, grid, pending, spanning.band_px)
        })
        .collect();
    let tracing = trace.is_some();
    let (decided, scratch, ignored) = loop {
        // R1: accessibility boxes are not supports; R2: spanned surfaces are not.
        let supports: Vec<BBox> = spanning
            .cut_prone
            .iter()
            .copied()
            .chain(
                surfaces
                    .iter()
                    .zip(&spanned)
                    .filter(|(_, spanned)| !**spanned)
                    .map(|(s, _)| *s),
            )
            .collect();
        // R3: a relation whose control is a box R1 or R2 leaves out (and no cut-prone box)
        // does not pull its label in; the label is closed over as a word.
        let left_out = |control: &BBox| {
            !holds(&classed.cut_prone, control)
                && (holds(&classed.whole_on_read, control)
                    || surfaces
                        .iter()
                        .zip(&spanned)
                        .any(|(s, spanned)| *spanned && s == control))
        };
        let (ignored, relations): (Vec<Relation>, Vec<Relation>) = evidence
            .relations
            .iter()
            .partition(|(control, _)| left_out(control));
        let mut scratch = tracing.then(PlanTrace::default);
        let decided = close_and_decide(
            grid,
            bounds,
            dirty,
            &supports,
            &relations,
            Some(&classed),
            params,
            scratch.as_mut(),
        )?;
        // R2 (v): a spanned surface whose inner box a region covers is closed over: that
        // region could read a box that repeats the surface (a fill inside its border), which
        // a full read drops as a duplicate. Closing over more only grows the regions, so the
        // loop ends, at a fixed point that does not depend on the order of the boxes.
        let mut more = false;
        if let Work::Regions(regions) = &decided.work {
            for (s, spanned) in surfaces.iter().zip(spanned.iter_mut()) {
                let covered = shrunk(s, spanning.band_px).is_some_and(|inner| {
                    regions
                        .iter()
                        .any(|r| pixel_box_contains(covered_pixels(r), inner))
                });
                if *spanned && covered {
                    *spanned = false;
                    more = true;
                }
            }
        }
        if !more {
            break (decided, scratch, ignored.len());
        }
    };
    let attempt_reason = match decided.work {
        Work::FullRefresh(reason) => Some(reason),
        Work::Nothing | Work::Regions(_) => None,
    };
    // R4: a support-aware full refresh gives way to the classic plan, today's plan.
    let classic = match attempt_reason {
        Some(_) => Some(close_and_decide(
            grid,
            bounds,
            dirty,
            evidence.supports,
            evidence.relations,
            None,
            params,
            None,
        )?),
        None => None,
    };
    if let Some(t) = trace {
        if let Some(s) = scratch {
            t.components = s.components;
            t.regions = s.regions;
            t.covered_area_px = s.covered_area_px;
        }
        let spanned_boxes: Vec<&BBox> = surfaces
            .iter()
            .zip(&spanned)
            .filter(|(_, spanned)| **spanned)
            .map(|(s, _)| s)
            .collect();
        let cut: Vec<f64> = classed
            .whole_on_read
            .iter()
            .filter(|a| {
                decided
                    .closed
                    .iter()
                    .any(|r| r.overlaps(a) && !r.contains(a))
            })
            .map(BBox::area)
            .collect();
        t.chosen = if classic.is_some() {
            PlanChoice::Classic
        } else {
            PlanChoice::Spanning
        };
        t.spanning = Some(SpanningTrace {
            surfaces_declared: surfaces.len(),
            surfaces_spanned: spanned_boxes.len(),
            largest_spanned_px: spanned_boxes.iter().map(|s| s.area()).fold(0.0, f64::max),
            whole_on_read_cut: cut.len(),
            largest_whole_on_read_cut_px: cut.iter().copied().fold(0.0, f64::max),
            relations_ignored: ignored,
            attempt_reason,
            classic: classic.as_ref().map(|c| ClassicTrace {
                full_refresh_reason: match c.work {
                    Work::FullRefresh(reason) => Some(reason),
                    Work::Nothing | Work::Regions(_) => None,
                },
                region_count: c.closed.len(),
                covered_area_px: c.closed.iter().map(BBox::area).sum(),
            }),
        });
    }
    Ok(classic.map_or(decided.work, |c| c.work))
}

/// The planner's body. With a `trace` it also records what it computes; the plan does not
/// depend on whether one is passed.
fn plan_regions_recorded(
    base: Option<&BgraFrame>,
    current: &BgraFrame,
    pending: &BTreeSet<TileCoord>,
    evidence: Evidence<'_>,
    spanning: Option<Spanning<'_>>,
    params: &PlanParams,
    mut trace: Option<&mut PlanTrace>,
) -> Result<RegionPlan, IncrementalError> {
    let grid = TileGrid::for_frame(current, params.tile_size)?;
    let diff = dirty_tiles(base, current, params.tile_size)?;
    if let Some(t) = trace.as_deref_mut() {
        t.frame_area_px = current.bounds().area();
        t.tile_size = params.tile_size;
        t.changed_tiles = diff.tiles.len();
        t.pending_tiles = pending.iter().filter(|&&tile| grid.contains(tile)).count();
        t.budget_area_px = params.max_area_fraction * current.bounds().area();
        t.max_regions = params.max_regions;
    }
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
    // R0: without a declared band the plan is today's, bit for bit. (Without a base the diff
    // already made it a full refresh.)
    let work = match (spanning, base) {
        (Some(spanning), Some(base)) => plan_spanning(
            &grid, bounds, base, current, &dirty, pending, evidence, spanning, params, trace,
        )?,
        _ => {
            close_and_decide(
                &grid,
                bounds,
                &dirty,
                evidence.supports,
                evidence.relations,
                None,
                params,
                trace,
            )?
            .work
        }
    };
    Ok(RegionPlan { dirty, work })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every planner test below goes through this: it plans with `plan_regions` and with
    /// `plan_regions_traced` and requires the same plan, so the tests of the shared body also
    /// check the traced entry point (a local item shadows the glob import).
    fn plan_regions(
        base: Option<&BgraFrame>,
        current: &BgraFrame,
        pending: &BTreeSet<TileCoord>,
        evidence: Evidence<'_>,
        params: &PlanParams,
    ) -> Result<RegionPlan, IncrementalError> {
        let plain = super::plan_regions(base, current, pending, evidence, params);
        let traced =
            plan_regions_traced(base, current, pending, evidence, params).map(|(plan, _)| plan);
        assert_eq!(
            plain, traced,
            "the traced planner must plan what plan_regions plans"
        );
        plain
    }

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

/// The plan trace (`demo --plan-trace`): the same plan as `plan_regions`, and the numbers
/// that say why it was chosen.
#[cfg(test)]
mod plan_trace_tests {
    use super::*;

    fn bx(x0: f64, y0: f64, x1: f64, y1: f64) -> BBox {
        BBox::new(x0, y0, x1, y1).unwrap()
    }

    fn blank(w: u32, h: u32, epoch: u64) -> BgraFrame {
        BgraFrame::new(w, h, 4 * w as usize, epoch, vec![0u8; 4 * (w * h) as usize]).unwrap()
    }

    /// `frame` with the blue byte of every pixel of `rect` (`x0, y0, x1, y1`, in pixels) set.
    fn painted(frame: &BgraFrame, rect: (u32, u32, u32, u32)) -> BgraFrame {
        let mut buf = frame.pixels().to_vec();
        for y in rect.1..rect.3 {
            for x in rect.0..rect.2 {
                buf[y as usize * frame.row_stride() + 4 * x as usize] = 255;
            }
        }
        BgraFrame::new(
            frame.width(),
            frame.height(),
            frame.row_stride(),
            frame.geometry_epoch(),
            buf,
        )
        .unwrap()
    }

    fn traced(
        base: &BgraFrame,
        current: &BgraFrame,
        supports: &[BBox],
        params: &PlanParams,
    ) -> (RegionPlan, PlanTrace) {
        plan_regions_traced(
            Some(base),
            current,
            &BTreeSet::new(),
            Evidence {
                supports,
                relations: &[],
            },
            params,
        )
        .unwrap()
    }

    /// The product configuration's budgets on a 1920x1200 screen.
    fn product() -> PlanParams {
        PlanParams {
            adaptive_min_regions: 2,
            ..PlanParams::default()
        }
    }

    const FRAME: f64 = 1920.0 * 1200.0;

    /// The decision rule of audit A5 (section 6), written before the run: H1 when a region
    /// is small before the closure and above the budget after it; H2 when it is above the
    /// budget before.
    fn hypotheses(trace: &PlanTrace) -> (bool, bool) {
        let h1 = trace.components.iter().any(|c| {
            c.padded_area_px < 0.05 * trace.frame_area_px
                && c.closed_area_px > 0.4 * trace.frame_area_px
        });
        let h2 = trace
            .components
            .iter()
            .any(|c| c.padded_area_px > 0.4 * trace.frame_area_px);
        (h1, h2)
    }

    /// H1: one changed tile that touches an evidence box covering half the frame. The change
    /// is small; the closure makes the region big. The full refresh is `area_budget`.
    #[test]
    fn plan_trace_h1_a_small_change_swallowed_by_a_large_support() {
        let base = blank(1920, 1200, 1);
        let current = painted(&base, (100, 100, 101, 101));
        let half_frame = bx(0.0, 0.0, 960.0, 1200.0);
        let (plan, trace) = traced(&base, &current, &[half_frame], &product());

        assert_eq!(plan.work, Work::FullRefresh(FullRefreshReason::AreaBudget));
        assert_eq!(trace.decided_by, Some(FullRefreshReason::AreaBudget));
        assert_eq!((trace.changed_tiles, trace.dirty_tiles), (1, 1));
        let [component] = trace.components[..] else {
            panic!("one component expected: {trace:?}");
        };
        assert_eq!(component.tiles, 1);
        assert_eq!(component.tight_area_px, 64.0 * 64.0);
        assert_eq!(component.padded_area_px, 128.0 * 128.0);
        assert_eq!(component.closed_area_px, half_frame.area());
        assert_eq!(component.largest_support_px, half_frame.area());
        assert_eq!(trace.covered_area_px, half_frame.area());
        assert_eq!(trace.budget_area_px, 0.4 * FRAME);
        assert!(trace.covered_area_px > trace.budget_area_px);
        assert_eq!(trace.decisive_region, Some(0));
        assert_eq!(trace.regions[0].pre_closure_area_px, 128.0 * 128.0);
        assert_eq!(trace.regions[0].components, 1);
        assert_eq!(hypotheses(&trace), (true, false));
    }

    /// H2: a change that is itself half the frame, with no evidence at all. The same reason,
    /// the opposite cause: the region is big before the closure.
    #[test]
    fn plan_trace_h2_a_change_that_is_itself_large() {
        let base = blank(1920, 1200, 1);
        let current = painted(&base, (0, 0, 1000, 1200));
        let (plan, trace) = traced(&base, &current, &[], &product());

        assert_eq!(plan.work, Work::FullRefresh(FullRefreshReason::AreaBudget));
        assert_eq!(trace.decided_by, Some(FullRefreshReason::AreaBudget));
        let [component] = trace.components[..] else {
            panic!("one component expected: {trace:?}");
        };
        assert!(component.padded_area_px > trace.budget_area_px, "{trace:?}");
        assert_eq!(component.largest_support_px, 0.0);
        assert_eq!(component.closed_area_px, component.padded_area_px);
        assert_eq!(hypotheses(&trace), (false, true));
    }

    /// The two causes give the same plan and the same reason; only the trace tells them apart.
    #[test]
    fn plan_trace_tells_the_two_hypotheses_apart_when_the_plans_are_equal() {
        let base = blank(1920, 1200, 1);
        let small = painted(&base, (100, 100, 101, 101));
        let large = painted(&base, (0, 0, 1000, 1200));
        let (plan_1, trace_1) = traced(&base, &small, &[bx(0.0, 0.0, 960.0, 1200.0)], &product());
        let (plan_2, trace_2) = traced(&base, &large, &[], &product());
        assert_eq!(plan_1.work, plan_2.work);
        assert_ne!(hypotheses(&trace_1), hypotheses(&trace_2));
    }

    /// Two small distant changes are two regions: under the product configuration the plan is
    /// `adaptive_cost`, and the trace shows the sum far from the budget.
    #[test]
    fn plan_trace_two_small_regions_decide_by_the_adaptive_rule_not_the_area() {
        let base = blank(1920, 1200, 1);
        let current = painted(
            &painted(&base, (100, 100, 101, 101)),
            (1500, 900, 1501, 901),
        );
        let (plan, trace) = traced(&base, &current, &[], &product());
        assert_eq!(
            plan.work,
            Work::FullRefresh(FullRefreshReason::AdaptiveCost)
        );
        assert_eq!(trace.decided_by, Some(FullRefreshReason::AdaptiveCost));
        assert_eq!(trace.regions.len(), 2);
        assert!(trace.covered_area_px < 0.05 * FRAME, "{trace:?}");
        assert_eq!(trace.decisive_region, None);
    }

    /// A regional plan has no deciding check, and its regions are the plan's.
    #[test]
    fn plan_trace_a_regional_plan_has_no_deciding_check() {
        let base = blank(1920, 1200, 1);
        let current = painted(&base, (100, 100, 101, 101));
        let (plan, trace) = traced(&base, &current, &[], &PlanParams::default());
        let Work::Regions(regions) = &plan.work else {
            panic!("regional plan expected: {plan:?}");
        };
        assert_eq!(trace.decided_by, None);
        assert_eq!(trace.decisive_region, None);
        assert_eq!(regions.len(), trace.regions.len());
        assert_eq!(trace.regions[0].area_px, regions[0].area());
    }

    /// Nothing to do: no components, no regions, no deciding check.
    #[test]
    fn plan_trace_an_idle_plan_is_empty() {
        let base = blank(256, 256, 1);
        let (plan, trace) = traced(&base, &base, &[], &PlanParams::default());
        assert_eq!(plan.work, Work::Nothing);
        assert_eq!(trace.dirty_tiles, 0);
        assert!(trace.components.is_empty() && trace.regions.is_empty());
        assert_eq!(trace.decided_by, None);
        assert_eq!(trace.frame_area_px, 256.0 * 256.0);
    }

    /// A full refresh decided before any region exists says so: the whole grid is dirty and
    /// there are no components.
    #[test]
    fn plan_trace_a_missing_base_is_decided_before_the_regions() {
        let frame = blank(256, 192, 1);
        let (plan, trace) = plan_regions_traced(
            None,
            &frame,
            &BTreeSet::new(),
            Evidence::default(),
            &PlanParams::default(),
        )
        .unwrap();
        assert_eq!(
            plan.work,
            Work::FullRefresh(FullRefreshReason::NoCommittedBase)
        );
        assert_eq!(trace.decided_by, Some(FullRefreshReason::NoCommittedBase));
        assert_eq!(trace.dirty_tiles, 4 * 3);
        assert!(trace.components.is_empty() && trace.regions.is_empty());
    }

    /// The trace is only numbers, counts and a reason: this destructures every field without
    /// a rest pattern, so a field that names a place (a box, a corner, a label) cannot be
    /// added without changing this test and showing it to a reviewer.
    #[test]
    fn plan_trace_holds_counts_and_areas_only_never_text_or_a_coordinate() {
        let PlanTrace {
            frame_area_px: _frame_area,
            tile_size: _tile_size,
            changed_tiles: _changed,
            pending_tiles: _pending,
            dirty_tiles: _dirty,
            components,
            regions,
            covered_area_px: _covered,
            budget_area_px: _budget,
            max_regions: _max_regions,
            decided_by: _decided_by,
            decisive_region: _decisive,
            chosen: _chosen,
            spanning: _spanning,
        } = PlanTrace::default();
        let SpanningTrace {
            surfaces_declared: _declared,
            surfaces_spanned: _spanned,
            largest_spanned_px: _largest_spanned,
            whole_on_read_cut: _cut,
            largest_whole_on_read_cut_px: _largest_cut,
            relations_ignored: _ignored,
            attempt_reason: _attempt,
            classic: _classic,
        } = SpanningTrace::default();
        let ClassicTrace {
            full_refresh_reason: _reason,
            region_count: _region_count,
            covered_area_px: _classic_covered,
        } = ClassicTrace::default();
        let ComponentTrace {
            tiles: _tiles,
            tight_area_px: _tight,
            padded_area_px: _padded,
            closed_area_px: _closed,
            largest_support_px: _support,
            largest_support_class: _support_class,
        } = ComponentTrace::default();
        let RegionTrace {
            area_px: _area,
            components: _members,
            pre_closure_area_px: _pre,
            largest_support_px: _largest,
            largest_support_class: _largest_class,
        } = RegionTrace::default();
        assert!(components.is_empty() && regions.is_empty());
    }

    /// The reviewer's case: a 32x16 frame, two disconnected 8 px edge tiles and a 16 px halo.
    /// Each padded box is 384 px2 and they share 256 px2, so the union is 512 px2, the whole
    /// frame and the final region. A sum of the two (768 px2) would exceed the frame.
    #[test]
    fn plan_trace_overlapping_halos_are_counted_once_before_the_closure() {
        let base = blank(32, 16, 1);
        // The tile at column 0, row 0 and the one at column 3, row 1: 8-connectivity keeps them apart.
        let current = painted(&painted(&base, (0, 0, 1, 1)), (31, 15, 32, 16));
        let params = PlanParams {
            tile_size: 8,
            halo_px: 16.0,
            max_area_fraction: 1.0,
            ..PlanParams::default()
        };
        let (plan, trace) = traced(&base, &current, &[], &params);
        assert!(
            matches!(plan.work, Work::Regions(ref rs) if rs.len() == 1),
            "{plan:?}"
        );
        assert_eq!(trace.components.len(), 2);
        let padded: Vec<f64> = trace.components.iter().map(|c| c.padded_area_px).collect();
        assert_eq!(padded, [384.0, 384.0]);
        let [region] = trace.regions[..] else {
            panic!("one region expected: {trace:?}");
        };
        assert_eq!(region.components, 2);
        assert_eq!(region.area_px, 512.0);
        assert_eq!(region.pre_closure_area_px, 512.0, "a union, not 768");
        assert_eq!(trace.frame_area_px, 512.0);
    }

    /// Two halos that do not touch have a union equal to their sum.
    #[test]
    fn plan_trace_disjoint_halos_before_the_closure_add_up() {
        let base = blank(400, 100, 1);
        let current = painted(&painted(&base, (0, 0, 1, 1)), (399, 0, 400, 1));
        let params = PlanParams {
            tile_size: 8,
            halo_px: 4.0,
            max_area_fraction: 1.0,
            ..PlanParams::default()
        };
        let (_, trace) = traced(&base, &current, &[], &params);
        assert_eq!(trace.regions.len(), 2, "{trace:?}");
        for region in &trace.regions {
            assert_eq!(region.components, 1);
            assert_eq!(region.pre_closure_area_px, 12.0 * 12.0);
        }
    }

    /// `union_area` against the area counted pixel by pixel, on generated boxes (integer
    /// corners, so every pixel is whole).
    #[test]
    fn plan_trace_the_union_area_is_the_area_of_the_covered_pixels() {
        let mut rng = Rng(0x0011_0022);
        for case in 0..300 {
            let (w, h) = (rng.between(1, 40), rng.between(1, 30));
            let boxes: Vec<BBox> = (0..rng.between(0, 9))
                .map(|_| random_box(&mut rng, w, h))
                .collect();
            let mut covered = vec![false; (w * h) as usize];
            for b in &boxes {
                for y in b.y0() as u32..b.y1() as u32 {
                    for x in b.x0() as u32..b.x1() as u32 {
                        covered[(y * w + x) as usize] = true;
                    }
                }
            }
            let expected = covered.iter().filter(|&&c| c).count() as f64;
            assert_eq!(union_area(&boxes), expected, "case {case}: {boxes:?}");
            let sum: f64 = boxes.iter().map(BBox::area).sum();
            assert!(union_area(&boxes) <= sum);
        }
        assert_eq!(union_area(&[]), 0.0);
    }

    /// Fixed-seed generator (splitmix64), as in the tests above.
    struct Rng(u64);

    impl Rng {
        fn next_u64(&mut self) -> u64 {
            self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = self.0;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^ (z >> 31)
        }

        /// A number in `lo..hi`, for `lo < hi`.
        fn between(&mut self, lo: u32, hi: u32) -> u32 {
            lo + u32::try_from(self.next_u64() % u64::from(hi - lo)).unwrap()
        }
    }

    /// A box with area inside a `w` x `h` frame; three in four are control-sized.
    fn random_box(rng: &mut Rng, w: u32, h: u32) -> BBox {
        let (x0, y0) = (rng.between(0, w), rng.between(0, h));
        let cap = if rng.between(0, 4) == 0 { u32::MAX } else { 96 };
        let (x1, y1) = (
            rng.between(x0 + 1, x0.saturating_add(cap).min(w) + 1),
            rng.between(y0 + 1, y0.saturating_add(cap).min(h) + 1),
        );
        bx(f64::from(x0), f64::from(y0), f64::from(x1), f64::from(y1))
    }

    /// One generated planning problem.
    struct Input {
        base: Option<BgraFrame>,
        current: BgraFrame,
        pending: BTreeSet<TileCoord>,
        supports: Vec<BBox>,
        relations: Vec<(BBox, BBox)>,
        params: PlanParams,
    }

    /// The kinds of problem the generator is built to reach, one per `profile % PROFILES`.
    const PROFILES: u32 = 11;
    /// Tile sizes from the smallest the grid test uses to the largest the configuration allows
    /// (`tile_size` 8-1024), with sizes that do not divide the frames.
    const TILES: [u32; 11] = [4, 8, 16, 32, 64, 65, 100, 128, 256, 512, 1024];

    /// A rectangle `(x0, y0, x1, y1)` of at most `span` px in each direction inside `w` x `h`.
    fn random_rect(rng: &mut Rng, w: u32, h: u32, span: u32) -> (u32, u32, u32, u32) {
        let (x0, y0) = (rng.between(0, w), rng.between(0, h));
        (
            x0,
            y0,
            rng.between(x0 + 1, (x0 + span).min(w) + 1),
            rng.between(y0 + 1, (y0 + span).min(h) + 1),
        )
    }

    /// The problem of `profile`: 0 general, 1 no base, 2 base of another geometry, 3 closure
    /// that runs out of rounds, 4 many separate regions, 5 a change over the area budget, 6 the
    /// baseline, 7 two separate regions under the adaptive rule, 8 nothing changed, 9 thin
    /// frames, 10 wide frames with tiles that do not divide them.
    // One generator for every profile, so that the profiles stay side by side.
    #[allow(clippy::too_many_lines)]
    fn generate(rng: &mut Rng, profile: u32) -> Input {
        let (w, h) = match profile {
            4 | 7 => (rng.between(300, 700), rng.between(120, 300)),
            9 if rng.between(0, 2) == 0 => (rng.between(1, 5), rng.between(1, 500)),
            9 => (rng.between(1, 500), rng.between(1, 5)),
            10 => (rng.between(500, 900), rng.between(33, 120)),
            _ => (rng.between(33, 400), rng.between(33, 300)),
        };
        let tile = TILES[rng.between(0, TILES.len() as u32) as usize];
        let mut params = PlanParams {
            tile_size: tile,
            halo_px: f64::from(rng.between(0, 41)),
            max_closure_rounds: [1, 2, 1000][rng.between(0, 3) as usize],
            max_regions: rng.between(2, 6) as usize,
            max_area_fraction: f64::from(rng.between(3, 11)) / 10.0,
            experimental_full_frame: false,
            adaptive_min_regions: [0, 0, 2, 3][rng.between(0, 4) as usize],
        };
        let base_frame = blank(w, h, 1);
        let mut current = base_frame.clone();
        let mut supports: Vec<BBox> = Vec::new();
        match profile {
            // Nothing changed and nothing is pending.
            8 => {}
            // Separate single-pixel changes, far enough apart to stay separate regions.
            4 | 7 => {
                let n = if profile == 4 { rng.between(5, 9) } else { 2 };
                params.tile_size = 8;
                params.halo_px = f64::from(rng.between(0, 5));
                params.max_closure_rounds = 1000;
                params.max_area_fraction = 1.0;
                params.adaptive_min_regions = if profile == 4 { 0 } else { 2 };
                params.max_regions = if profile == 4 {
                    rng.between(2, 5) as usize
                } else {
                    4
                };
                for i in 0..n {
                    let x = i * (w / n) + rng.between(0, 8);
                    let y = rng.between(0, h);
                    current = painted(&current, (x, y, x + 1, y + 1));
                }
            }
            // A change that alone covers most of the frame, over a small budget.
            5 => {
                params.max_area_fraction = 0.3;
                params.adaptive_min_regions = 0;
                params.max_closure_rounds = 1000;
                current = painted(&current, (0, 0, w, h * 2 / 3 + 1));
            }
            // A support that grows the region, with a closure that has one round.
            3 => {
                params.tile_size = 8 + 8 * rng.between(0, 2);
                params.halo_px = f64::from(rng.between(0, 5));
                params.max_closure_rounds = 1;
                let rect = random_rect(rng, w, h, 20);
                current = painted(&current, rect);
                let grown = bx(
                    f64::from(rect.0.saturating_sub(30)),
                    f64::from(rect.1.saturating_sub(30)),
                    f64::from((rect.2 + 30).min(w)),
                    f64::from((rect.3 + 30).min(h)),
                );
                supports.push(grown);
            }
            // Mostly small changes, so that regional plans are common.
            _ => {
                for _ in 0..rng.between(0, 5) {
                    current = painted(&current, random_rect(rng, w, h, 48));
                }
            }
        }
        if !matches!(profile, 3 | 4 | 5 | 7 | 8) {
            supports.extend((0..rng.between(0, 6)).map(|_| random_box(rng, w, h)));
        }
        let relations: Vec<(BBox, BBox)> = if matches!(profile, 0 | 9 | 10) {
            (0..rng.between(0, 3))
                .map(|_| (random_box(rng, w, h), random_box(rng, w, h)))
                .collect()
        } else {
            Vec::new()
        };
        let pending: BTreeSet<TileCoord> = if matches!(profile, 0 | 9 | 10) {
            (0..rng.between(0, 4))
                .map(|_| TileCoord::new(rng.between(0, w / tile + 3), rng.between(0, h / tile + 3)))
                .collect()
        } else {
            BTreeSet::new()
        };
        params.experimental_full_frame = profile == 6;
        let base = match profile {
            1 => None,
            2 if rng.between(0, 2) == 0 => Some(blank(w + 1, h, 1)),
            2 => Some(blank(w, h, 2)),
            _ => Some(base_frame),
        };
        Input {
            base,
            current,
            pending,
            supports,
            relations,
            params,
        }
    }

    /// The label of the kind of plan, for the coverage count.
    fn kind(plan: &RegionPlan) -> &'static str {
        match &plan.work {
            Work::Nothing => "idle",
            Work::Regions(_) => "regional",
            Work::FullRefresh(reason) => reason.as_str(),
        }
    }

    /// Everything the trace must satisfy for `input`, in `why`'s words on failure.
    // Every property of one trace, in one place, so that a failure shows the whole case.
    #[allow(clippy::too_many_lines)]
    fn check(input: &Input, why: &str) -> &'static str {
        let evidence = Evidence {
            supports: &input.supports,
            relations: &input.relations,
        };
        let expected = plan_regions(
            input.base.as_ref(),
            &input.current,
            &input.pending,
            evidence,
            &input.params,
        )
        .unwrap();
        let (plan, trace) = plan_regions_traced(
            input.base.as_ref(),
            &input.current,
            &input.pending,
            evidence,
            &input.params,
        )
        .unwrap();
        let why = format!("{why}: {trace:?}");
        let params = &input.params;

        // The plan is bit for bit the plan.
        assert_eq!(plan, expected, "{why}");

        // The trace agrees with it.
        let frame_area = f64::from(input.current.width()) * f64::from(input.current.height());
        assert_eq!(trace.frame_area_px, frame_area, "{why}");
        assert_eq!(trace.dirty_tiles, plan.dirty.len(), "{why}");
        assert_eq!(trace.tile_size, params.tile_size, "{why}");
        assert_eq!(trace.max_regions, params.max_regions, "{why}");
        assert_eq!(
            trace.budget_area_px,
            params.max_area_fraction * frame_area,
            "{why}"
        );
        match &plan.work {
            Work::FullRefresh(reason) => assert_eq!(trace.decided_by, Some(*reason), "{why}"),
            Work::Nothing | Work::Regions(_) => assert_eq!(trace.decided_by, None, "{why}"),
        }
        if let Work::Regions(regions) = &plan.work {
            let areas: Vec<f64> = regions.iter().map(BBox::area).collect();
            let traced_areas: Vec<f64> = trace.regions.iter().map(|r| r.area_px).collect();
            assert_eq!(traced_areas, areas, "{why}");
            assert!(trace.covered_area_px <= trace.budget_area_px, "{why}");
        }
        if matches!(plan.work, Work::Nothing) {
            assert!(
                trace.components.is_empty() && trace.regions.is_empty(),
                "{why}"
            );
        }
        if trace.decided_by == Some(FullRefreshReason::AreaBudget) {
            assert!(trace.covered_area_px > trace.budget_area_px, "{why}");
            let decisive = trace.decisive_region.expect("a decisive region");
            let largest = trace.regions.iter().map(|r| r.area_px).fold(0.0, f64::max);
            assert_eq!(trace.regions[decisive].area_px, largest, "{why}");
        } else {
            assert_eq!(trace.decisive_region, None, "{why}");
        }
        if trace.decided_by == Some(FullRefreshReason::TooManyRegions) {
            assert!(trace.regions.len() > params.max_regions, "{why}");
        }

        // Every area is at most the frame's, a union is at most its parts' sum, and the
        // closure only grows a region.
        assert!(trace.covered_area_px <= frame_area, "{why}");
        assert!(trace.budget_area_px <= frame_area, "{why}");
        for c in &trace.components {
            assert!(c.tiles > 0, "{why}");
            assert!(c.tight_area_px <= c.padded_area_px, "{why}");
            assert!(c.padded_area_px <= c.closed_area_px, "{why}");
            assert!(c.closed_area_px <= frame_area, "{why}");
            assert!(c.largest_support_px <= c.closed_area_px, "{why}");
        }
        let padded_sum: f64 = trace.components.iter().map(|c| c.padded_area_px).sum();
        for r in &trace.regions {
            assert!(r.area_px <= frame_area, "{why}");
            assert!(r.pre_closure_area_px <= r.area_px, "{why}");
            assert!(r.pre_closure_area_px <= padded_sum, "{why}");
            assert!(r.pre_closure_area_px > 0.0, "{why}");
            assert!(r.largest_support_px <= r.area_px, "{why}");
            assert!(r.components >= 1, "{why}");
        }
        // Every component ends in exactly one final region.
        if !trace.regions.is_empty() {
            let members: usize = trace.regions.iter().map(|r| r.components).sum();
            assert_eq!(members, trace.components.len(), "{why}");
        }
        kind(&plan)
    }

    /// For generated frames of every shape the engine accepts (thin, wide, partial tiles, tile
    /// sizes up to 1024), bases, evidence, pending tiles and parameters: the traced plan is the
    /// plan `plan_regions` returns, and the trace agrees with it. Every kind of plan must be
    /// generated, so a generator that stops reaching one fails here.
    #[test]
    fn plan_trace_the_traced_plan_equals_the_plan_and_the_numbers_agree_with_it() {
        use std::collections::BTreeMap;

        let mut rng = Rng(0x00A5_0001);
        let mut kinds: BTreeMap<&'static str, usize> = BTreeMap::new();
        for case in 0..660 {
            let profile = case % PROFILES;
            let input = generate(&mut rng, profile);
            let label = check(&input, &format!("case {case} (profile {profile})"));
            *kinds.entry(label).or_default() += 1;
        }
        for needed in [
            "idle",
            "regional",
            "no_committed_base",
            "geometry_changed",
            "closure_budget",
            "too_many_regions",
            "area_budget",
            "experimental_baseline",
            "adaptive_cost",
        ] {
            assert!(
                kinds.get(needed).copied().unwrap_or(0) >= 10,
                "the generator stopped reaching {needed}: {kinds:?}"
            );
        }
    }

    /// The frames at the edge of what the engine accepts: 20 000 000 px (`MAX_FRAME_PIXELS`),
    /// wide, tall and square, with the largest tiles, with and without a support that covers
    /// almost half of the frame. Areas stay exact and at most the frame's.
    #[test]
    fn plan_trace_holds_at_the_pixel_bound() {
        for (w, h, tile) in [
            (20_000, 1_000, 1024),
            (1_000, 20_000, 512),
            (4_472, 4_472, 100),
        ] {
            let base = blank(w, h, 1);
            let current = painted(&base, (w / 2, h / 2, w / 2 + 3, h / 2 + 3));
            let frame_area = f64::from(w) * f64::from(h);
            assert!(frame_area <= MAX_FRAME_PIXELS as f64);
            for support in [
                None,
                Some(bx(0.0, 0.0, f64::from(w) * 0.7, f64::from(h) * 0.7)),
            ] {
                let input = Input {
                    base: Some(base.clone()),
                    current: current.clone(),
                    pending: BTreeSet::new(),
                    supports: support.into_iter().collect(),
                    relations: Vec::new(),
                    params: PlanParams {
                        tile_size: tile,
                        ..PlanParams::default()
                    },
                };
                let label = check(
                    &input,
                    &format!("{w}x{h}, tile {tile}, support {support:?}"),
                );
                assert_eq!(
                    label,
                    if support.is_some() {
                        "area_budget"
                    } else {
                        "regional"
                    }
                );
            }
        }
    }
}

/// UT-83 (ADR-0045): the support-aware closure. Rules R0 to R4, the band of a surface to the
/// pixel, the cases measured on the lab's screen, and a property over generated inputs.
#[cfg(test)]
mod support_aware_tests {
    use super::*;

    fn bx(x0: f64, y0: f64, x1: f64, y1: f64) -> BBox {
        BBox::new(x0, y0, x1, y1).unwrap()
    }

    fn blank(w: u32, h: u32) -> BgraFrame {
        BgraFrame::new(w, h, 4 * w as usize, 1, vec![0u8; 4 * (w * h) as usize]).unwrap()
    }

    /// `frame` with every pixel of `rect` (`x0, y0, x1, y1`) given the BGR value `v`.
    fn painted(frame: &BgraFrame, rect: (u32, u32, u32, u32), v: u8) -> BgraFrame {
        let mut buf = frame.pixels().to_vec();
        for y in rect.1..rect.3 {
            for x in rect.0..rect.2 {
                let i = y as usize * frame.row_stride() + 4 * x as usize;
                buf[i..i + 3].copy_from_slice(&[v, v, v]);
            }
        }
        BgraFrame::new(frame.width(), frame.height(), frame.row_stride(), 1, buf).unwrap()
    }

    /// The product configuration (`adaptive_min_regions = 2`, a 40 % area budget).
    fn product() -> PlanParams {
        PlanParams {
            adaptive_min_regions: 2,
            ..PlanParams::default()
        }
    }

    /// The evidence as the engine splits it: `supports` is the three lists, in that order.
    struct Split {
        cut_prone: Vec<BBox>,
        surfaces: Vec<BBox>,
        whole_on_read: Vec<BBox>,
        relations: Vec<(BBox, BBox)>,
        band: u32,
    }

    impl Split {
        fn supports(&self) -> Vec<BBox> {
            self.cut_prone
                .iter()
                .chain(&self.surfaces)
                .chain(&self.whole_on_read)
                .copied()
                .collect()
        }

        fn spanning(&self) -> Spanning<'_> {
            Spanning {
                cut_prone: &self.cut_prone,
                surfaces: &self.surfaces,
                whole_on_read: &self.whole_on_read,
                band_px: self.band,
            }
        }
    }

    /// Plans with and without the trace and requires the same plan.
    fn plan(
        base: &BgraFrame,
        current: &BgraFrame,
        pending: &BTreeSet<TileCoord>,
        split: &Split,
        on: bool,
        params: &PlanParams,
    ) -> (RegionPlan, PlanTrace) {
        let supports = split.supports();
        let evidence = Evidence {
            supports: &supports,
            relations: &split.relations,
        };
        let spanning = on.then(|| split.spanning());
        let plain =
            plan_regions_with(Some(base), current, pending, evidence, spanning, params).unwrap();
        let (traced, trace) =
            plan_regions_with_traced(Some(base), current, pending, evidence, spanning, params)
                .unwrap();
        assert_eq!(plain, traced, "the traced plan must be the plan");
        (plain, trace)
    }

    fn regions(work: &Work) -> &[BBox] {
        match work {
            Work::Regions(rs) => rs,
            _ => panic!("expected regions, got {work:?}"),
        }
    }

    fn covered(work: &Work) -> f64 {
        regions(work).iter().map(BBox::area).sum()
    }

    const W: u32 = 1920;
    const H: u32 = 1200;
    const FRAME: f64 = 1920.0 * 1200.0;

    /// The UI Automation Document of a maximised Notepad (section 2 of the unit: 1920 x 983
    /// rows, 81.92 % of the frame).
    fn document() -> BBox {
        bx(0.0, 99.0, 1920.0, 1082.0)
    }

    /// The flat-region surface of a Notepad window at its default size (1426 x 811, 49.81 %
    /// of a 1920 x 1200 frame before it was rounded to whole rows).
    fn window_surface() -> BBox {
        bx(247.0, 195.0, 1673.0, 1006.0)
    }

    fn only(cut_prone: Vec<BBox>, surfaces: Vec<BBox>, whole_on_read: Vec<BBox>) -> Split {
        Split {
            cut_prone,
            surfaces,
            whole_on_read,
            relations: Vec::new(),
            band: 4,
        }
    }

    /// (3) Scene (e): a caret blinking inside the maximised Document. Classic: the closure
    /// takes the Document, 81.92 %, `area_budget`. Support-aware: one region under 5 %.
    #[test]
    fn ut_83_a_caret_inside_a_maximised_document_stays_one_small_region() {
        let base = blank(W, H);
        let current = painted(&base, (300, 400, 302, 418), 200);
        let split = only(
            vec![bx(0.0, 49.0, 400.0, 80.0)],
            Vec::new(),
            vec![document()],
        );
        let (classic, _) = plan(&base, &current, &BTreeSet::new(), &split, false, &product());
        assert_eq!(
            classic.work,
            Work::FullRefresh(FullRefreshReason::AreaBudget)
        );
        let (aware, trace) = plan(&base, &current, &BTreeSet::new(), &split, true, &product());
        assert_eq!(regions(&aware.work).len(), 1);
        assert!(covered(&aware.work) < 0.05 * FRAME, "{aware:?}");
        assert_eq!(trace.chosen, PlanChoice::Spanning);
        assert_eq!(
            trace.regions[0].largest_support_class,
            Some(SupportClass::WholeOnRead)
        );
        let s = trace.spanning.unwrap();
        assert_eq!((s.whole_on_read_cut, s.classic), (1, None));
        assert!((s.largest_whole_on_read_cut_px - document().area()).abs() < 1e-9);
    }

    /// (3) Scene (c): the clock in the taskbar; its padded box meets the Document's bottom.
    /// Classic: 91.75 % (the Document and the clock's box). Support-aware: under 15 %.
    #[test]
    fn ut_83_a_clock_tick_next_to_the_document_stays_small() {
        let base = blank(W, H);
        let current = painted(&base, (1850, 1100, 1890, 1180), 200);
        let split = only(Vec::new(), Vec::new(), vec![document()]);
        let (classic, trace) = plan(&base, &current, &BTreeSet::new(), &split, false, &product());
        assert_eq!(
            classic.work,
            Work::FullRefresh(FullRefreshReason::AreaBudget)
        );
        assert!((trace.covered_area_px / FRAME - 0.9175).abs() < 1e-3);
        let (aware, trace) = plan(&base, &current, &BTreeSet::new(), &split, true, &product());
        assert_eq!(regions(&aware.work).len(), 1);
        assert!(covered(&aware.work) < 0.15 * FRAME, "{aware:?}");
        assert_eq!(trace.chosen, PlanChoice::Spanning);
    }

    /// (3) Scene S4: a caret inside a window-sized flat surface. Spanned while its band is
    /// unchanged: one region under 5 %.
    #[test]
    fn ut_83_a_caret_inside_a_window_surface_spans_the_surface() {
        let base = blank(W, H);
        let current = painted(&base, (700, 500, 702, 518), 200);
        let split = only(Vec::new(), vec![window_surface()], Vec::new());
        let (classic, _) = plan(&base, &current, &BTreeSet::new(), &split, false, &product());
        assert_eq!(
            classic.work,
            Work::FullRefresh(FullRefreshReason::AreaBudget)
        );
        let (aware, trace) = plan(&base, &current, &BTreeSet::new(), &split, true, &product());
        assert!(covered(&aware.work) < 0.05 * FRAME, "{aware:?}");
        let s = trace.spanning.unwrap();
        assert_eq!((s.surfaces_declared, s.surfaces_spanned), (1, 1));
        assert!((s.largest_spanned_px - window_surface().area()).abs() < 1e-9);
        assert_eq!(
            trace.regions[0].largest_support_class,
            Some(SupportClass::Surface)
        );
    }

    /// R0: without `Spanning` (a detector that declares no band) the plan is `plan_regions`'s.
    #[test]
    fn ut_83_without_spanning_the_plan_is_todays() {
        let base = blank(W, H);
        let current = painted(&base, (700, 500, 702, 518), 200);
        let split = only(Vec::new(), vec![window_surface()], vec![document()]);
        let supports = split.supports();
        let evidence = Evidence {
            supports: &supports,
            relations: &[],
        };
        let today = plan_regions(
            Some(&base),
            &current,
            &BTreeSet::new(),
            evidence,
            &product(),
        )
        .unwrap();
        let (off, trace) = plan(&base, &current, &BTreeSet::new(), &split, false, &product());
        assert_eq!(today, off);
        assert_eq!((trace.chosen, trace.spanning), (PlanChoice::Classic, None));
        assert!(
            trace
                .components
                .iter()
                .all(|c| c.largest_support_class.is_none())
        );
    }

    /// The surface for the band tests: 200 x 200 inside a 512 x 512 frame, band 4.
    fn band_surface() -> BBox {
        bx(100.0, 100.0, 300.0, 300.0)
    }

    fn closes_over_surface(current: &BgraFrame, pending: &BTreeSet<TileCoord>) -> bool {
        let base = blank(512, 512);
        let split = only(Vec::new(), vec![band_surface()], Vec::new());
        let params = PlanParams::default();
        let (aware, _) = plan(&base, current, pending, &split, true, &params);
        match aware.work {
            Work::Regions(rs) => rs.iter().any(|r| r.contains(&band_surface())),
            Work::FullRefresh(_) => true,
            Work::Nothing => false,
        }
    }

    /// (4) The band to the pixel: a changed pixel at distance `b - 1` inside the edge is in
    /// the band and closes over the surface; at distance `b` it is inside and does not; one
    /// `b` px outside the edge (the last band pixel) closes over it, one further does not.
    #[test]
    fn ut_83_the_band_is_exact_to_the_pixel() {
        let base = blank(512, 512);
        let none = BTreeSet::new();
        let at = |x: u32, y: u32| painted(&base, (x, y, x + 1, y + 1), 255);
        assert!(closes_over_surface(&at(103, 200), &none), "inside, b - 1");
        assert!(!closes_over_surface(&at(104, 200), &none), "inside, b");
        assert!(
            closes_over_surface(&at(296, 200), &none),
            "inside right, b - 1"
        );
        assert!(
            !closes_over_surface(&at(295, 200), &none),
            "inside right, b"
        );
        assert!(closes_over_surface(&at(96, 200), &none), "outside, b");
        assert!(!closes_over_surface(&at(95, 200), &none), "outside, b + 1");
        assert!(
            closes_over_surface(&at(200, 303), &none),
            "outside below, b"
        );
        assert!(
            !closes_over_surface(&at(200, 304), &none),
            "outside below, b + 1"
        );
    }

    /// (4) A pending tile on the band closes over the surface; one inside the inner box does
    /// not.
    #[test]
    fn ut_83_a_pending_tile_on_the_band_closes_over_the_surface() {
        let base = blank(512, 512);
        let on_band: BTreeSet<TileCoord> = [TileCoord::new(1, 2)].into_iter().collect();
        let inside: BTreeSet<TileCoord> = [TileCoord::new(3, 3)].into_iter().collect();
        assert!(closes_over_surface(&base, &on_band));
        assert!(!closes_over_surface(&base, &inside));
    }

    /// (4) A surface touching the frame's edge is closed over: whether it is background
    /// depends on its pixels' mean colour, which a change inside it can move.
    #[test]
    fn ut_83_a_surface_on_the_frame_edge_is_closed_over() {
        let base = blank(512, 512);
        let current = painted(&base, (150, 200, 151, 201), 255);
        let edge = bx(0.0, 100.0, 300.0, 300.0);
        let split = only(Vec::new(), vec![edge], Vec::new());
        let (aware, trace) = plan(
            &base,
            &current,
            &BTreeSet::new(),
            &split,
            true,
            &PlanParams::default(),
        );
        assert!(regions(&aware.work).iter().any(|r| r.contains(&edge)));
        assert_eq!(trace.spanning.unwrap().surfaces_spanned, 0);
        assert!(!band_unchanged(
            &edge,
            &base,
            &current,
            &TileGrid::for_frame(&current, 64).unwrap(),
            &BTreeSet::new(),
            4
        ));
    }

    /// R2 (v): a region that covers a spanned surface's inner box closes over it, so a crop
    /// never holds a box that repeats the surface without the surface.
    #[test]
    fn ut_83_a_region_covering_the_inner_box_closes_over_the_surface() {
        let base = blank(256, 256);
        let s = bx(100.0, 100.0, 140.0, 140.0);
        // The whole inner box changes; the band does not.
        let current = painted(&base, (104, 104, 136, 136), 255);
        let split = only(Vec::new(), vec![s], Vec::new());
        let params = PlanParams {
            tile_size: 4,
            halo_px: 0.0,
            ..PlanParams::default()
        };
        let (aware, trace) = plan(&base, &current, &BTreeSet::new(), &split, true, &params);
        assert_eq!(regions(&aware.work), &[s]);
        assert_eq!(trace.spanning.unwrap().surfaces_spanned, 0);
    }

    /// (5) R3: a relation from the Document to a label word above it does not pull the
    /// Document in; a relation from a cut-prone control still pulls its label.
    #[test]
    fn ut_83_a_relation_from_a_whole_on_read_box_does_not_pull_it_in() {
        let base = blank(W, H);
        let current = painted(&base, (900, 700, 902, 718), 200);
        let label = bx(10.0, 60.0, 80.0, 80.0);
        let mut split = only(vec![label], Vec::new(), vec![document()]);
        split.relations = vec![(document(), label)];
        let (aware, trace) = plan(&base, &current, &BTreeSet::new(), &split, true, &product());
        assert!(covered(&aware.work) < 0.05 * FRAME, "{aware:?}");
        assert!(!regions(&aware.work)[0].overlaps(&label));
        assert_eq!(trace.spanning.unwrap().relations_ignored, 1);

        let control = bx(880.0, 690.0, 960.0, 730.0);
        let far = bx(1500.0, 300.0, 1560.0, 320.0);
        let mut split = only(vec![control, far], Vec::new(), vec![document()]);
        split.relations = vec![(control, far)];
        let (aware, trace) = plan(&base, &current, &BTreeSet::new(), &split, true, &product());
        assert!(regions(&aware.work)[0].contains(&far), "{aware:?}");
        assert_eq!(trace.spanning.unwrap().relations_ignored, 0);
    }

    /// R4: when the support-aware plan is a full refresh the classic plan runs, with its own
    /// reason, and the trace keeps both.
    #[test]
    fn ut_83_a_support_aware_full_refresh_gives_way_to_the_classic_plan() {
        let base = blank(W, H);
        // Five small changes far apart: five regions either way, `too_many_regions`.
        let mut current = base.clone();
        for x in [100, 500, 900, 1300, 1700] {
            current = painted(&current, (x, 600, x + 2, 618), 200);
        }
        let split = only(Vec::new(), Vec::new(), vec![document()]);
        let (classic, _) = plan(&base, &current, &BTreeSet::new(), &split, false, &product());
        let (aware, trace) = plan(&base, &current, &BTreeSet::new(), &split, true, &product());
        assert_eq!(aware, classic);
        assert_eq!(trace.chosen, PlanChoice::Classic);
        let s = trace.spanning.unwrap();
        assert_eq!(s.attempt_reason, Some(FullRefreshReason::TooManyRegions));
        assert_eq!(trace.decided_by, Some(FullRefreshReason::TooManyRegions));
        let c = s.classic.unwrap();
        assert_eq!(c.full_refresh_reason, Some(FullRefreshReason::AreaBudget));
        assert_eq!(c.region_count, 1);
    }

    /// Out-of-frame boxes in the split are an error, as they are in the classic closure.
    #[test]
    fn ut_83_split_boxes_outside_the_frame_are_rejected() {
        let base = blank(256, 256);
        let current = painted(&base, (10, 10, 11, 11), 255);
        let split = only(Vec::new(), vec![bx(200.0, 200.0, 300.0, 300.0)], Vec::new());
        let supports = split.supports();
        let evidence = Evidence {
            supports: &supports,
            relations: &[],
        };
        assert_eq!(
            plan_regions_with(
                Some(&base),
                &current,
                &BTreeSet::new(),
                evidence,
                Some(split.spanning()),
                &PlanParams::default()
            ),
            Err(IncrementalError::SupportOutOfBounds)
        );
    }

    /// The band check stays in range at the largest frame the engine accepts.
    #[test]
    fn ut_83_the_band_check_holds_at_the_pixel_bound() {
        for (w, h) in [(20_000, 1_000), (1_000, 20_000)] {
            let base = blank(w, h);
            let current = painted(&base, (w / 2, h / 2, w / 2 + 1, h / 2 + 1), 9);
            let grid = TileGrid::for_frame(&current, 64).unwrap();
            let s = bx(1.0, 1.0, f64::from(w) - 1.0, f64::from(h) - 1.0);
            assert!(!band_unchanged(
                &s,
                &base,
                &current,
                &grid,
                &BTreeSet::new(),
                u32::MAX
            ));
            assert!(band_unchanged(
                &s,
                &base,
                &current,
                &grid,
                &BTreeSet::new(),
                4
            ));
            let edge = painted(&current, (0, 0, 1, 1), 9);
            assert!(!band_unchanged(
                &s,
                &base,
                &edge,
                &grid,
                &BTreeSet::new(),
                4
            ));
        }
    }

    /// splitmix64 (as UT-08): the same inputs on every run.
    struct Rng(u64);

    impl Rng {
        fn next_u64(&mut self) -> u64 {
            self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = self.0;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^ (z >> 31)
        }

        fn below(&mut self, n: u32) -> u32 {
            (self.next_u64() % u64::from(n.max(1))) as u32
        }
    }

    /// A box with whole-pixel corners inside `w x h`.
    fn random_box(rng: &mut Rng, w: u32, h: u32, max_side: u32) -> BBox {
        let bw = 1 + rng.below(max_side.min(w));
        let bh = 1 + rng.below(max_side.min(h));
        let x = rng.below(w - bw + 1);
        let y = rng.below(h - bh + 1);
        bx(
            f64::from(x),
            f64::from(y),
            f64::from(x + bw),
            f64::from(y + bh),
        )
    }

    fn shuffled<T: Clone>(rng: &mut Rng, items: &[T]) -> Vec<T> {
        let mut out = items.to_vec();
        for i in (1..out.len()).rev() {
            let j = rng.below(i as u32 + 1) as usize;
            out.swap(i, j);
        }
        out
    }

    /// (1) and (2): on generated frames, boxes, relations and pending tiles,
    /// - with no surface and no accessibility box the plan is the classic plan;
    /// - every final region contains every cut-prone box it meets, and every surface it meets
    ///   without containing has an unchanged band and an inner box no region covers (I1);
    /// - permuting the boxes and the relations does not change the plan;
    /// - R4: never a full refresh where the classic plan is regional, every region inside a
    ///   classic region, never more covered area; a full refresh is the classic plan's.
    #[test]
    #[allow(clippy::too_many_lines)]
    fn ut_83_property_the_support_aware_plan_keeps_i1_and_is_never_worse() {
        const SEED: u64 = 0x5EED_0083;
        let mut rng = Rng(SEED);
        let mut spanning_regional = 0;
        let mut spanned_met = 0;
        let mut cases = 0;
        for round in 0..400 {
            let (w, h) = [(256, 192), (320, 320), (640, 400), (97, 61)][round % 4];
            let tile = [16, 32, 64][round / 4 % 3];
            let params = PlanParams {
                tile_size: tile,
                halo_px: f64::from([0, 4, 16, 32][round / 12 % 4]),
                max_regions: 4,
                max_area_fraction: [0.4, 0.9][round / 48 % 2],
                adaptive_min_regions: [0, 2][round / 96 % 2],
                ..PlanParams::default()
            };
            let band = [1, 4, 9][round % 3];
            let base = blank(w, h);
            let mut current = base.clone();
            for _ in 0..rng.below(4) {
                let b = random_box(&mut rng, w, h, 12);
                current = painted(
                    &current,
                    (b.x0() as u32, b.y0() as u32, b.x1() as u32, b.y1() as u32),
                    1 + rng.below(250) as u8,
                );
            }
            let grid = TileGrid::for_frame(&current, tile).unwrap();
            let pending: BTreeSet<TileCoord> = (0..rng.below(2))
                .map(|_| TileCoord::new(rng.below(grid.cols()), rng.below(grid.rows())))
                .collect();
            let n = |rng: &mut Rng, k| (0..rng.below(k)).collect::<Vec<_>>();
            let cut_prone: Vec<BBox> = n(&mut rng, 6)
                .into_iter()
                .map(|_| random_box(&mut rng, w, h, 40))
                .collect();
            let surfaces: Vec<BBox> = n(&mut rng, 4)
                .into_iter()
                .map(|_| random_box(&mut rng, w, h, w.max(h)))
                .collect();
            let whole_on_read: Vec<BBox> = n(&mut rng, 3)
                .into_iter()
                .map(|_| random_box(&mut rng, w, h, w.max(h)))
                .collect();
            let all: Vec<BBox> = cut_prone
                .iter()
                .chain(&surfaces)
                .chain(&whole_on_read)
                .copied()
                .collect();
            let relations: Vec<(BBox, BBox)> = if all.len() >= 2 {
                (0..rng.below(3))
                    .map(|_| {
                        (
                            all[rng.below(all.len() as u32) as usize],
                            all[rng.below(all.len() as u32) as usize],
                        )
                    })
                    .collect()
            } else {
                Vec::new()
            };
            let split = Split {
                cut_prone: cut_prone.clone(),
                surfaces: surfaces.clone(),
                whole_on_read: whole_on_read.clone(),
                relations: relations.clone(),
                band,
            };
            cases += 1;
            let (classic, _) = plan(&base, &current, &pending, &split, false, &params);
            let (aware, _) = plan(&base, &current, &pending, &split, true, &params);

            // (1) Spanning with nothing to span is the classic plan.
            let plain = Split {
                cut_prone: split.supports(),
                surfaces: Vec::new(),
                whole_on_read: Vec::new(),
                relations: relations.clone(),
                band,
            };
            assert_eq!(
                plan(&base, &current, &pending, &plain, true, &params).0,
                classic,
                "round {round}"
            );

            // Permutations.
            let permuted = Split {
                cut_prone: shuffled(&mut rng, &cut_prone),
                surfaces: shuffled(&mut rng, &surfaces),
                whole_on_read: shuffled(&mut rng, &whole_on_read),
                relations: shuffled(&mut rng, &relations),
                band,
            };
            assert_eq!(
                plan(&base, &current, &pending, &permuted, true, &params).0,
                aware,
                "round {round}: order"
            );

            if let Work::FullRefresh(_) = aware.work {
                assert_eq!(aware, classic, "round {round}: R4");
            }
            if let Work::Regions(rs) = &aware.work {
                spanning_regional += 1;
                if let Work::Regions(cs) = &classic.work {
                    let total = |v: &[BBox]| v.iter().map(BBox::area).sum::<f64>();
                    assert!(total(rs) <= total(cs) + 1e-9, "round {round}: area");
                    for r in rs {
                        assert!(cs.iter().any(|c| c.contains(r)), "round {round}: inside");
                    }
                }
                for r in rs {
                    for c in &cut_prone {
                        assert!(!r.overlaps(c) || r.contains(c), "round {round}: I1");
                    }
                    for s in &surfaces {
                        if r.overlaps(s) && !r.contains(s) {
                            spanned_met += 1;
                            assert!(!holds(&sorted(&cut_prone), s));
                            assert!(
                                band_unchanged(s, &base, &current, &grid, &pending, band),
                                "round {round}: a cut surface with a changed band"
                            );
                            let inner = shrunk(s, band).unwrap();
                            assert!(
                                rs.iter()
                                    .all(|q| !pixel_box_contains(covered_pixels(q), inner))
                            );
                        }
                    }
                }
            }
            if aware.work == Work::Nothing {
                assert_eq!(classic.work, Work::Nothing);
            }
        }
        assert_eq!(cases, 400);
        assert!(spanning_regional > 100, "only {spanning_regional} regional");
        assert!(
            spanned_met > 20,
            "only {spanned_met} surfaces spanned by a region"
        );
    }
}
