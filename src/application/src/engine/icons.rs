//! Algorithm 3A in the observation (thesis chapter 04 §4.4, ADR-0004): the curated label
//! of an icon-like control, or nothing.
//!
//! A control is a candidate when its role takes no external label and it has no inner OCR
//! text, or when it is an `IconButton`. Its crop is the control's box, without an inner
//! margin (the dossier rejects the fixed 15% crop as an axiom), converted to greyscale and
//! resampled to the 9×8 grid of [`gradient_descriptor`]; the [`IconCache`] then answers a
//! curated label for the configured context or `unknown`. An unknown icon keeps an empty
//! label: similarity is never turned into a guess. The label never changes actionability,
//! which comes from the detector only.
//!
//! Incrementality: only candidates that overlap a planned region are resolved again; the
//! others keep the label of the last committed observation.

use ommatix_domain::appearance::{
    CachePolicy, GRID_LEN, IconCache, IconMatch, gradient_descriptor,
};
use ommatix_domain::bbox::BBox;
use ommatix_domain::fusion::{FusedControl, Role};
use ommatix_domain::incremental::BgraFrame;

/// Columns and rows of the descriptor grid.
const GRID_W: u64 = 9;
const GRID_H: u64 = 8;

/// Algorithm 3A's settings. The thesis calibrates the thresholds on its corpus (hypothesis
/// H4), so none has a default: without this configuration the resolver is off and every
/// icon stays unknown.
#[derive(Debug, Clone, PartialEq)]
pub struct IconResolverConfig {
    /// Curated prototypes. Only those of [`Self::context`] and of the current descriptor
    /// version can confirm a label.
    pub cache: IconCache,
    /// Acceptance thresholds (distance and margin) and the support policy.
    pub policy: CachePolicy,
    /// Gradient threshold of the descriptor, in grey levels (1..=255). An invalid value
    /// leaves every icon unknown.
    pub tau: u8,
    /// The application or screen the prototypes must belong to. Until the engine can
    /// identify the foreground application (S3), the operator configures it.
    pub context: String,
}

/// Which controls the current plan re-analyses.
#[derive(Debug, Clone, Copy)]
pub(super) enum Scope<'a> {
    /// Every control (first observation, full refresh).
    All,
    /// Controls overlapping these regions; the rest keep their previous label.
    Regions(&'a [BBox]),
}

impl Scope<'_> {
    fn covers(&self, bbox: &BBox) -> bool {
        match self {
            Self::All => true,
            Self::Regions(rs) => rs.iter().any(|r| r.overlaps(bbox)),
        }
    }
}

/// A curated label resolved for one control box, kept for the next observation.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct IconLabel {
    pub bbox: BBox,
    pub role: Role,
    pub label: String,
}

/// Whether the appearance cache may label `fc`.
pub(super) fn is_candidate(fc: &FusedControl) -> bool {
    let role = fc.control.role;
    role == Role::IconButton
        || (role != Role::TextNode
            && !role.takes_external_label()
            && !role.holds_value()
            && fc.inner_text().is_empty())
}

/// The curated label of every candidate in `controls`, in their order: resolved again
/// inside `scope`, copied from `previous` outside it.
pub(super) fn resolve(
    controls: &[FusedControl],
    frame: &BgraFrame,
    scope: Scope<'_>,
    previous: &[IconLabel],
    config: Option<&IconResolverConfig>,
) -> Vec<IconLabel> {
    let Some(config) = config else {
        return Vec::new();
    };
    controls
        .iter()
        .filter(|fc| is_candidate(fc))
        .filter_map(|fc| {
            let (bbox, role) = (fc.control.bbox, fc.control.role);
            let label = if scope.covers(&bbox) {
                lookup(frame, &bbox, config)?
            } else {
                previous
                    .iter()
                    .find(|p| p.bbox == bbox && p.role == role)?
                    .label
                    .clone()
            };
            Some(IconLabel { bbox, role, label })
        })
        .collect()
}

/// The curated label of the crop under `bbox`, or `None` for any unknown (including a
/// crop or a configuration the descriptor rejects).
fn lookup(frame: &BgraFrame, bbox: &BBox, config: &IconResolverConfig) -> Option<String> {
    let grid = grey_grid(frame, bbox)?;
    let descriptor = gradient_descriptor(&grid, config.tau).ok()?;
    match config
        .cache
        .lookup(&descriptor, &config.context, config.policy)
        .ok()?
    {
        IconMatch::Known { label, .. } => Some(label),
        IconMatch::Unknown(_) => None,
    }
}

/// Luma of one BGRA pixel with the ITU-R BT.601 weights (0.299 R + 0.587 G + 0.114 B),
/// in integer arithmetic, rounded to nearest. Alpha is ignored.
fn luma(bgra: &[u8]) -> u64 {
    let (b, g, r) = (u64::from(bgra[0]), u64::from(bgra[1]), u64::from(bgra[2]));
    (299 * r + 587 * g + 114 * b + 500) / 1000
}

/// The crop normalisation of descriptor version 1
/// ([`DESCRIPTOR_VERSION`](ommatix_domain::appearance::DESCRIPTOR_VERSION)): the crop under `bbox` (outward to whole pixels, clipped to the frame) as a 9×8
/// greyscale grid, row-major. Each cell is the rounded mean luma of its pixel block; the
/// blocks partition the crop by integer division, and a crop narrower than the grid
/// repeats pixels (every block has at least one). `None` when the crop is empty.
pub(super) fn grey_grid(frame: &BgraFrame, bbox: &BBox) -> Option<[u8; GRID_LEN]> {
    let clamp = |v: f64, max: u32| -> u64 { v.clamp(0.0, f64::from(max)) as u64 };
    let (x0, x1) = (
        clamp(bbox.x0().floor(), frame.width()),
        clamp(bbox.x1().ceil(), frame.width()),
    );
    let (y0, y1) = (
        clamp(bbox.y0().floor(), frame.height()),
        clamp(bbox.y1().ceil(), frame.height()),
    );
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    let (w, h) = (x1 - x0, y1 - y0);
    let stride = frame.row_stride() as u64;
    let pixels = frame.pixels();
    let mut grid = [0u8; GRID_LEN];
    for gy in 0..GRID_H {
        let sy0 = y0 + gy * h / GRID_H;
        let sy1 = (y0 + (gy + 1) * h / GRID_H).max(sy0 + 1);
        for gx in 0..GRID_W {
            let sx0 = x0 + gx * w / GRID_W;
            let sx1 = (x0 + (gx + 1) * w / GRID_W).max(sx0 + 1);
            let mut sum = 0u64;
            for y in sy0..sy1 {
                let row = (y * stride) as usize;
                for x in sx0..sx1 {
                    let i = row + 4 * x as usize;
                    sum += luma(&pixels[i..i + 4]);
                }
            }
            let n = (sy1 - sy0) * (sx1 - sx0);
            grid[(gy * GRID_W + gx) as usize] = ((sum + n / 2) / n) as u8;
        }
    }
    Some(grid)
}
