//! Box 7, path A (dossier chapter 06 §6.7, S0 §19): the offline calibration of the icon cache
//! (Algorithm 3A, chapter 04 §4.4, ADR-0004) on a synthetic corpus, and the generator of the
//! operator's curated prototypes for the icon-cache soak.
//!
//! ```text
//! cargo run --release -p ommatix-acceptance --example icon_calibration -- --out-dir DIR
//! ```
//!
//! It writes, into `DIR` (outside the checkout, never versioned):
//!
//! * `icons.jsonl`: the curated prototypes (`src/app/src/prototypes.rs` format), eight classes
//!   of the synthetic strip of `tools/s4/icon_spec.rs` in four renderings each, for the context
//!   of the test application. The file depends only on the shared spec, not on any threshold;
//!   the operator pins its SHA-256 in `[cache] prototypes_sha256`;
//! * `calibration.json`: the corpus, the selection rule, the chosen thresholds (or why there are
//!   none) and the metrics on three disjoint sets of seeds, with Wilson intervals;
//! * `cache-proposal.toml`: the `[cache]` block the thresholds would give, **a proposal for the
//!   owner**: nothing in the repository carries those numbers (the thesis calibrates them and
//!   they have no defaults);
//! * `seq_100/`, `seq_125/`, `seq_150/`: two identical PNG frames of a window with the strip
//!   at that scale, for `ommatix demo --source images` (the detector check; synthetic).
//!
//! Everything is drawn here: no screen capture, no screen text, no input, no clock, no network.
//! The selection rule and the corpus parameters below are the ones frozen in the dossier
//! (S0 §19.2) before the first run; the crop normalisation is the engine's own
//! (`ommatix_application::grey_grid`), and the decision rule is checked against
//! `IconCache::lookup` by a test.
//!
//! `--smoke` runs a tiny corpus on other seeds to exercise the tool; its report says so and
//! proposes nothing. `--strips-only` writes only the PNG sequences; `--diagnose` writes `diagnosis.json`, exploratory and on the
//! calibration set only.

// The summary is this tool's output: it may print to stdout (the workspace denies it for the
// MCP server).
#![allow(clippy::print_stdout)]
// Pixel and index arithmetic on small bounded values (crops of a few hundred pixels, counts of
// a few thousand): the casts below cannot lose anything that matters.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap
)]

use std::fmt::Write as _;
use std::path::Path;

use ommatix_application::grey_grid;
use ommatix_domain::appearance::{
    GRID_LEN, GradientDescriptor, SupportPolicy, descriptor_distance, gradient_descriptor,
};
use ommatix_domain::bbox::BBox;
use ommatix_domain::incremental::BgraFrame;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

#[path = "../../../tools/s4/icon_spec.rs"]
mod icon_spec;
use icon_spec::{CELL, CLASSES, GAP, INK_GREY, LABELS, Shape, WINDOW_GREY, glyph, strip_width};

// ---- the frozen protocol (S0 §19.2) ----

/// The context the prototypes belong to: the test application's executable.
const CONTEXT: &str = "ommatix-testapp.exe";
/// `[cache] max_entries` of the soak: the bound S0 §17 already used.
const MAX_ENTRIES: u32 = 64;
/// Seeds of the calibration (selection), validation (gate) and test (report) sets.
const SEEDS: [u64; 3] = [
    0x0C1A_55E5_0002_0001,
    0x0C1A_55E5_0002_0002,
    0x0C1A_55E5_0002_0003,
];
/// The development seed of `--diagnose`: where the feasibility of a protocol is looked at before
/// it is frozen. Never one of the three of [`SEEDS`].
const DEV_SEED: u64 = 0x0C1A_55E5_0002_00D0;
/// Other seeds, for `--smoke`.
const SMOKE_SEEDS: [u64; 3] = [0x5E0E_0001, 0x5E0E_0002, 0x5E0E_0003];
/// Samples per set: positives per class, then per out-of-distribution shape (16 shapes) and per
/// procedural kind (4 kinds).
const FULL_SIZES: Sizes = Sizes {
    positives_per_class: 150,
    per_shape: 40,
    per_procedural: 40,
};
const SMOKE_SIZES: Sizes = Sizes {
    positives_per_class: 12,
    per_shape: 3,
    per_procedural: 3,
};
/// Renderings of a sample: scale, sub-pixel offset and detector-box margins.
const SCALES: [f64; 3] = [1.0, 1.25, 1.5];
const OFFSETS: [f64; 4] = [0.0, 0.25, 0.5, 0.75];
/// The detector's box is the glyph's own box with this margin, in pixels, on each side, drawn
/// uniformly per side: a negative margin cuts into the ink.
const MARGIN_RANGE: (i64, i64) = (-1, 4);
/// No crop is smaller than this, in pixels, on either side.
const MIN_CROP: i64 = 9;
/// Gradient thresholds searched (grey levels).
const TAUS: [u8; 11] = [2, 3, 4, 6, 8, 12, 16, 24, 32, 48, 64];
/// `max_distance` and `min_margin` are searched in 0.00 ..= 0.50 in steps of 0.01.
const STEPS: usize = 50;
/// Out-of-distribution false acceptance allowed, per hundred (H4: fixed before the test).
const FAR_LIMIT_PERCENT: u64 = 1;
/// Coverage the validation gate asks of the chosen thresholds, per hundred.
const MIN_COVERAGE_PERCENT: u64 = 50;

#[derive(Debug, Clone, Copy)]
struct Sizes {
    positives_per_class: usize,
    per_shape: usize,
    per_procedural: usize,
}

// ---- a deterministic generator ----

/// xorshift64*: the same bytes on every platform, no dependency.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed | 1)
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// A number in `[0, 1)`.
    fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// A number in `0..n`.
    fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }

    /// A number in `lo..=hi`.
    fn between(&mut self, lo: i64, hi: i64) -> i64 {
        lo + (self.next_u64() % (hi - lo + 1) as u64) as i64
    }

    /// Approximately normal, mean 0, deviation 1 (sum of twelve uniforms).
    fn gauss(&mut self) -> f64 {
        (0..12).map(|_| self.unit()).sum::<f64>() - 6.0
    }
}

// ---- the software rasteriser ----

fn dist2(c: (f64, f64), x: f64, y: f64) -> f64 {
    (x - c.0) * (x - c.0) + (y - c.1) * (y - c.1)
}

/// Whether the design point `(x, y)` is inside `shape`. Only `+ - * /` and comparisons: the same
/// result on every platform.
fn covers(shape: &Shape, x: f64, y: f64) -> bool {
    match *shape {
        Shape::Rect { x0, y0, x1, y1 } => x >= x0 && x < x1 && y >= y0 && y < y1,
        Shape::Disc { c, r } => dist2(c, x, y) <= r * r,
        Shape::Ring { c, r, width } => {
            let d = dist2(c, x, y);
            let inner = (r - width).max(0.0);
            d <= r * r && d >= inner * inner
        }
        Shape::Segment { a, b, width } => {
            let (abx, aby) = (b.0 - a.0, b.1 - a.1);
            let len2 = abx * abx + aby * aby;
            let t = if len2 > 0.0 {
                (((x - a.0) * abx + (y - a.1) * aby) / len2).clamp(0.0, 1.0)
            } else {
                0.0
            };
            dist2((a.0 + t * abx, a.1 + t * aby), x, y) <= (width / 2.0) * (width / 2.0)
        }
        Shape::Poly(points) => {
            let (mut positive, mut negative) = (false, false);
            for (i, p) in points.iter().enumerate() {
                let q = points[(i + 1) % points.len()];
                let cross = (q.0 - p.0) * (y - p.1) - (q.1 - p.1) * (x - p.0);
                positive |= cross > 0.0;
                negative |= cross < 0.0;
            }
            !(positive && negative)
        }
    }
}

/// Grey levels of one rendering.
#[derive(Debug, Clone, Copy)]
struct Style {
    window: u8,
    ink: u8,
    /// Deviation of the Gaussian noise added to each pixel, in grey levels.
    sigma: f64,
}

const NOMINAL: Style = Style {
    window: WINDOW_GREY,
    ink: INK_GREY,
    sigma: 0.0,
};

fn random_style(rng: &mut Rng) -> Style {
    let level = |rng: &mut Rng, lo, hi| rng.between(lo, hi) as u8;
    Style {
        window: level(rng, 230, 255),
        ink: level(rng, 20, 90),
        sigma: [0.0, 1.0, 2.0, 3.0][rng.below(4)],
    }
}

/// The grey of the design point `(dx, dy)` of a glyph made of `shapes`: ink under a shape, the
/// window elsewhere.
fn glyph_sample(style: &Style, shapes: &[Shape], dx: f64, dy: f64) -> u8 {
    if shapes.iter().any(|s| covers(s, dx, dy)) {
        style.ink
    } else {
        style.window
    }
}

/// Samples per pixel along each axis.
const SUPERSAMPLE: usize = 4;

/// Mean of the supersampled `sample` over the pixel `(x, y)`, rounded.
fn pixel(x: usize, y: usize, sample: &dyn Fn(f64, f64) -> u8) -> u8 {
    let mut sum = 0u32;
    for sy in 0..SUPERSAMPLE {
        for sx in 0..SUPERSAMPLE {
            let px = x as f64 + (sx as f64 + 0.5) / SUPERSAMPLE as f64;
            let py = y as f64 + (sy as f64 + 0.5) / SUPERSAMPLE as f64;
            sum += u32::from(sample(px, py));
        }
    }
    let n = (SUPERSAMPLE * SUPERSAMPLE) as u32;
    ((sum + n / 2) / n) as u8
}

/// Where the design origin sits inside the crop the detector would hand over.
#[derive(Debug, Clone, Copy)]
struct View {
    scale: f64,
    tx: f64,
    ty: f64,
    width: usize,
    height: usize,
}

fn grey_frame(width: usize, height: usize, grey: &[u8]) -> BgraFrame {
    let mut pixels = Vec::with_capacity(width * height * 4);
    for &v in grey {
        pixels.extend_from_slice(&[v, v, v, 255]);
    }
    BgraFrame::new(width as u32, height as u32, width * 4, 1, pixels)
        .expect("the generated crops are valid frames")
}

/// One crop: the glyph under `view`, supersampled, with the style's noise.
fn render(view: &View, style: &Style, shapes: &[Shape], rng: &mut Rng) -> BgraFrame {
    let mut grey = Vec::with_capacity(view.width * view.height);
    for y in 0..view.height {
        for x in 0..view.width {
            let mut v = f64::from(pixel(x, y, &|px, py| {
                glyph_sample(
                    style,
                    shapes,
                    (px - view.tx) / view.scale,
                    (py - view.ty) / view.scale,
                )
            }));
            if style.sigma > 0.0 {
                v += rng.gauss() * style.sigma;
            }
            grey.push(v.round().clamp(0.0, 255.0) as u8);
        }
    }
    grey_frame(view.width, view.height, &grey)
}

/// The engine's own 9×8 grid of the whole frame, taken as the detector's box.
fn grid_of(frame: &BgraFrame) -> [u8; GRID_LEN] {
    let bbox = BBox::new(
        0.0,
        0.0,
        f64::from(frame.width()),
        f64::from(frame.height()),
    )
    .expect("the generated crops are not empty");
    grey_grid(frame, &bbox).expect("the generated crops are not empty")
}

fn invert(grid: &[u8; GRID_LEN]) -> [u8; GRID_LEN] {
    grid.map(|v| 255 - v)
}

/// The box of `shapes` in design units: `(x0, y0, x1, y1)`; the cell's interior for no shapes.
fn extent(shapes: &[Shape]) -> (f64, f64, f64, f64) {
    let mut e = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    let mut grow = |x0: f64, y0: f64, x1: f64, y1: f64| {
        e = (e.0.min(x0), e.1.min(y0), e.2.max(x1), e.3.max(y1));
    };
    for shape in shapes {
        match *shape {
            Shape::Rect { x0, y0, x1, y1 } => grow(x0, y0, x1, y1),
            Shape::Disc { c, r } | Shape::Ring { c, r, .. } => {
                grow(c.0 - r, c.1 - r, c.0 + r, c.1 + r);
            }
            Shape::Segment { a, b, width } => {
                let h = width / 2.0;
                grow(
                    a.0.min(b.0) - h,
                    a.1.min(b.1) - h,
                    a.0.max(b.0) + h,
                    a.1.max(b.1) + h,
                );
            }
            Shape::Poly(points) => {
                for p in points {
                    grow(p.0, p.1, p.0, p.1);
                }
            }
        }
    }
    if shapes.is_empty() {
        (8.0, 8.0, CELL - 8.0, CELL - 8.0)
    } else {
        e
    }
}

/// The crop a detector would hand over around `shapes`: the glyph's own box at `scale`, with
/// `margins` pixels (left, top, right, bottom; negative cuts into the ink) and a sub-pixel
/// `offset` of the drawing inside it.
fn view_around(shapes: &[Shape], scale: f64, margins: [i64; 4], offset: (f64, f64)) -> View {
    let (gx0, gy0, gx1, gy1) = extent(shapes);
    let left = (gx0 * scale).floor() as i64 - margins[0];
    let top = (gy0 * scale).floor() as i64 - margins[1];
    let right = (gx1 * scale).ceil() as i64 + margins[2];
    let bottom = (gy1 * scale).ceil() as i64 + margins[3];
    View {
        scale,
        tx: -(left as f64) + offset.0,
        ty: -(top as f64) + offset.1,
        width: (right - left + 1).max(MIN_CROP) as usize,
        height: (bottom - top + 1).max(MIN_CROP) as usize,
    }
}

/// A detector box around `shapes`: a scale, a sub-pixel offset and a margin on each side.
fn random_view(rng: &mut Rng, shapes: &[Shape]) -> View {
    let scale = SCALES[rng.below(SCALES.len())];
    let mut side = || rng.between(MARGIN_RANGE.0, MARGIN_RANGE.1);
    let margins = [side(), side(), side(), side()];
    let offset = (OFFSETS[rng.below(4)], OFFSETS[rng.below(4)]);
    view_around(shapes, scale, margins, offset)
}

// ---- the shapes that are not in the strip ----

const fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Shape {
    Shape::Rect { x0, y0, x1, y1 }
}

const fn seg(a: (f64, f64), b: (f64, f64), width: f64) -> Shape {
    Shape::Segment { a, b, width }
}

const LEFT_TRIANGLE: [(f64, f64); 3] = [(37.0, 12.0), (37.0, 44.0), (11.0, 28.0)];
const DIAMOND: [(f64, f64); 4] = [(28.0, 10.0), (46.0, 28.0), (28.0, 46.0), (10.0, 28.0)];
const HALF_DISC: [(f64, f64); 8] = [
    (10.0, 38.0),
    (12.0, 29.0),
    (17.0, 21.0),
    (24.0, 16.0),
    (32.0, 16.0),
    (39.0, 21.0),
    (44.0, 29.0),
    (46.0, 38.0),
];
const CENTRE: (f64, f64) = (28.0, 28.0);
const DOT_GRID: [Shape; 9] = [
    rect(12.0, 12.0, 19.0, 19.0),
    rect(25.0, 12.0, 32.0, 19.0),
    rect(38.0, 12.0, 45.0, 19.0),
    rect(12.0, 25.0, 19.0, 32.0),
    rect(25.0, 25.0, 32.0, 32.0),
    rect(38.0, 25.0, 45.0, 32.0),
    rect(12.0, 38.0, 19.0, 45.0),
    rect(25.0, 38.0, 32.0, 45.0),
    rect(38.0, 38.0, 45.0, 45.0),
];

/// Eight near misses: each is one of the strip's classes changed a little.
fn near_misses() -> [Vec<Shape>; 8] {
    [
        vec![rect(29.0, 12.0, 35.0, 44.0), rect(12.0, 22.0, 44.0, 28.0)],
        vec![Shape::Ring {
            c: CENTRE,
            r: 21.0,
            width: 3.0,
        }],
        vec![Shape::Poly(&LEFT_TRIANGLE)],
        vec![
            rect(12.0, 12.0, 44.0, 17.0),
            rect(12.0, 17.0, 17.0, 44.0),
            rect(39.0, 17.0, 44.0, 44.0),
        ],
        vec![seg((14.0, 14.0), (42.0, 42.0), 6.0)],
        vec![
            seg((12.0, 28.0), (22.0, 40.0), 6.0),
            seg((22.0, 40.0), (30.0, 30.0), 6.0),
        ],
        vec![
            rect(12.0, 12.0, 20.0, 44.0),
            rect(24.0, 12.0, 32.0, 44.0),
            rect(36.0, 12.0, 44.0, 44.0),
        ],
        vec![
            seg((36.0, 12.0), (18.0, 28.0), 6.0),
            seg((18.0, 28.0), (36.0, 44.0), 6.0),
        ],
    ]
}

/// Eight shapes unlike any class.
fn other_shapes() -> [Vec<Shape>; 8] {
    [
        vec![rect(14.0, 12.0, 22.0, 44.0), rect(14.0, 36.0, 44.0, 44.0)],
        vec![rect(12.0, 12.0, 44.0, 19.0), rect(25.0, 19.0, 31.0, 44.0)],
        vec![
            rect(13.0, 12.0, 19.0, 44.0),
            rect(37.0, 12.0, 43.0, 44.0),
            rect(13.0, 25.0, 43.0, 31.0),
        ],
        vec![rect(25.0, 12.0, 31.0, 44.0)],
        vec![Shape::Poly(&DIAMOND)],
        vec![
            Shape::Disc {
                c: (19.0, 28.0),
                r: 7.0,
            },
            Shape::Disc {
                c: (37.0, 28.0),
                r: 7.0,
            },
        ],
        DOT_GRID.to_vec(),
        vec![Shape::Poly(&HALF_DISC)],
    ]
}

/// What the procedural out-of-distribution samples are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Procedural {
    Flat,
    Noise,
    TextLike,
    Clutter,
}

const PROCEDURAL: [Procedural; 4] = [
    Procedural::Flat,
    Procedural::Noise,
    Procedural::TextLike,
    Procedural::Clutter,
];

/// The shapes of one procedural sample, and the extra noise it asks for.
fn procedural_shapes(kind: Procedural, rng: &mut Rng) -> (Vec<Shape>, f64) {
    match kind {
        Procedural::Flat => (Vec::new(), 0.0),
        Procedural::Noise => (Vec::new(), 70.0),
        Procedural::TextLike => {
            let mut shapes = Vec::new();
            for row in 0..5 {
                let y = 14.0 + 7.0 * f64::from(row);
                let mut x = 12.0;
                while x < 40.0 {
                    let w = rng.between(4, 12) as f64;
                    shapes.push(rect(x, y, (x + w).min(44.0), y + 4.0));
                    x += w + rng.between(3, 5) as f64;
                }
            }
            (shapes, 0.0)
        }
        Procedural::Clutter => {
            let mut shapes = Vec::new();
            for _ in 0..rng.between(3, 6) {
                let (x, y) = (rng.between(10, 36) as f64, rng.between(10, 36) as f64);
                let size = rng.between(4, 14) as f64;
                shapes.push(if rng.below(2) == 0 {
                    rect(x, y, x + size, y + size)
                } else {
                    Shape::Disc {
                        c: (x + size / 2.0, y + size / 2.0),
                        r: size / 2.0,
                    }
                });
            }
            (shapes, 0.0)
        }
    }
}

// ---- the corpus ----

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Group {
    NearMiss,
    OtherShape,
    Procedural,
}

impl Group {
    const ALL: [Self; 3] = [Self::NearMiss, Self::OtherShape, Self::Procedural];

    const fn name(self) -> &'static str {
        match self {
            Self::NearMiss => "near_miss",
            Self::OtherShape => "other_shape",
            Self::Procedural => "procedural",
        }
    }
}

/// The grids of one set, before any threshold.
struct Corpus {
    positives: Vec<(usize, [u8; GRID_LEN])>,
    ood: Vec<(Group, [u8; GRID_LEN])>,
}

fn build_corpus(seed: u64, sizes: Sizes) -> Corpus {
    let mut rng = Rng::new(seed);
    let mut positives = Vec::new();
    for class in 0..CLASSES {
        for _ in 0..sizes.positives_per_class {
            let (view, style) = (random_view(&mut rng, glyph(class)), random_style(&mut rng));
            let frame = render(&view, &style, glyph(class), &mut rng);
            positives.push((class, grid_of(&frame)));
        }
    }
    let mut ood = Vec::new();
    for (group, shapes) in near_misses()
        .into_iter()
        .map(|s| (Group::NearMiss, s))
        .chain(other_shapes().into_iter().map(|s| (Group::OtherShape, s)))
    {
        for _ in 0..sizes.per_shape {
            let (view, style) = (random_view(&mut rng, &shapes), random_style(&mut rng));
            let frame = render(&view, &style, &shapes, &mut rng);
            ood.push((group, grid_of(&frame)));
        }
    }
    for kind in PROCEDURAL {
        for _ in 0..sizes.per_procedural {
            let (shapes, sigma) = procedural_shapes(kind, &mut rng);
            let (view, mut style) = (random_view(&mut rng, &shapes), random_style(&mut rng));
            style.sigma = style.sigma.max(sigma);
            let frame = render(&view, &style, &shapes, &mut rng);
            ood.push((Group::Procedural, grid_of(&frame)));
        }
    }
    Corpus { positives, ood }
}

// ---- the prototypes ----

/// The four renderings of each class kept as prototypes: the glyph's box with a two-pixel margin
/// at 100 %, the glyph's own box with half a pixel of offset, and the two-pixel margin at 125 %
/// and 150 %.
fn prototype_views(shapes: &[Shape]) -> [View; 4] {
    [
        view_around(shapes, 1.0, [2; 4], (0.0, 0.0)),
        view_around(shapes, 1.0, [0; 4], (0.5, 0.5)),
        view_around(shapes, 1.25, [2; 4], (0.0, 0.0)),
        view_around(shapes, 1.5, [2; 4], (0.0, 0.0)),
    ]
}

/// `(class, grid)` of every prototype, in file order.
fn prototype_grids() -> Vec<(usize, [u8; GRID_LEN])> {
    let mut rng = Rng::new(1);
    let mut out = Vec::new();
    for class in 0..CLASSES {
        for view in prototype_views(glyph(class)) {
            let frame = render(&view, &NOMINAL, glyph(class), &mut rng);
            out.push((class, grid_of(&frame)));
        }
    }
    out
}

/// The JSON-lines file of `src/app/src/prototypes.rs`. Keys come out in alphabetical order,
/// so the bytes are the same on every run.
fn prototype_file() -> String {
    let mut text = String::new();
    for (class, grid) in prototype_grids() {
        let line = json!({ "label": LABELS[class], "context": CONTEXT, "grid": grid.to_vec() });
        text.push_str(&line.to_string());
        text.push('\n');
    }
    text
}

// ---- the decision rule, over precomputed distances ----

#[derive(Debug, Clone, Copy)]
struct Raw {
    /// The query has the support `IconCache::lookup` asks for.
    support_ok: bool,
    /// The closest label and its distance.
    best: Option<(f64, usize)>,
    /// The distance of the closest *other* label.
    next: Option<f64>,
}

fn raw_of(grid: &[u8; GRID_LEN], tau: u8, protos: &[(usize, GradientDescriptor)]) -> Raw {
    let query = gradient_descriptor(grid, tau).expect("a 9x8 grid and tau >= 1");
    let policy = SupportPolicy::default();
    let mut best: Vec<(f64, usize)> = Vec::new();
    for (class, proto) in protos {
        let Some(d) = descriptor_distance(&query, proto, policy).expect("the default policy")
        else {
            continue;
        };
        match best.iter_mut().find(|(_, c)| c == class) {
            Some(entry) => entry.0 = entry.0.min(d),
            None => best.push((d, *class)),
        }
    }
    best.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    Raw {
        support_ok: query.support() >= policy.min_support,
        best: best.first().copied(),
        next: best.get(1).map(|b| b.0),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reason {
    Support,
    TooFar,
    Ambiguous,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Decision {
    Known(usize),
    Unknown(Reason),
}

/// `IconCache::lookup` over the distances already computed.
fn decide(raw: &Raw, max_distance: f64, min_margin: f64) -> Decision {
    if !raw.support_ok {
        return Decision::Unknown(Reason::Support);
    }
    let Some((d, class)) = raw.best else {
        return Decision::Unknown(Reason::Support);
    };
    if d > max_distance {
        return Decision::Unknown(Reason::TooFar);
    }
    if let Some(next) = raw.next
        && next - d < min_margin
    {
        return Decision::Unknown(Reason::Ambiguous);
    }
    Decision::Known(class)
}

fn step_value(step: usize) -> f64 {
    step as f64 / 100.0
}

/// What the rule did on a set at one setting.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Tally {
    positives: u64,
    correct: u64,
    wrong: u64,
    unknown_support: u64,
    unknown_far: u64,
    unknown_ambiguous: u64,
    ood: u64,
    ood_accepted: u64,
}

fn tally(positives: &[(usize, Raw)], ood: &[(Group, Raw)], distance: f64, margin: f64) -> Tally {
    let mut t = Tally::default();
    for (class, raw) in positives {
        t.positives += 1;
        match decide(raw, distance, margin) {
            Decision::Known(c) if c == *class => t.correct += 1,
            Decision::Known(_) => t.wrong += 1,
            Decision::Unknown(Reason::Support) => t.unknown_support += 1,
            Decision::Unknown(Reason::TooFar) => t.unknown_far += 1,
            Decision::Unknown(Reason::Ambiguous) => t.unknown_ambiguous += 1,
        }
    }
    for (_, raw) in ood {
        t.ood += 1;
        t.ood_accepted += u64::from(matches!(decide(raw, distance, margin), Decision::Known(_)));
    }
    t
}

/// The rule's two hard constraints: no wrong label among the accepted positives, and the
/// out-of-distribution acceptance within [`FAR_LIMIT_PERCENT`].
const fn constraints_hold(t: &Tally) -> bool {
    t.wrong == 0 && t.ood_accepted * 100 <= t.ood * FAR_LIMIT_PERCENT
}

/// The validation gate: the constraints and the coverage floor.
fn gate(t: &Tally) -> Result<(), String> {
    if t.wrong != 0 {
        return Err(format!("{} wrong labels on the validation set", t.wrong));
    }
    if t.ood_accepted * 100 > t.ood * FAR_LIMIT_PERCENT {
        return Err(format!(
            "{} of {} out-of-distribution samples accepted on the validation set (limit {FAR_LIMIT_PERCENT} %)",
            t.ood_accepted, t.ood
        ));
    }
    if t.correct * 100 < t.positives * MIN_COVERAGE_PERCENT {
        return Err(format!(
            "coverage {} of {} on the validation set is below {MIN_COVERAGE_PERCENT} %",
            t.correct, t.positives
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Params {
    tau: u8,
    distance_step: usize,
    margin_step: usize,
}

/// The selection rule: among the settings whose constraints hold on the calibration set, the one
/// with the most correct positives; ties: the smaller `max_distance`, then the larger
/// `min_margin`, then the smaller `tau`. A setting that accepts nothing is not a proposal.
fn select(candidates: &[(Params, Tally)]) -> Option<(Params, Tally)> {
    let key = |(p, t): &(Params, Tally)| {
        (
            t.correct,
            std::cmp::Reverse(p.distance_step),
            p.margin_step,
            std::cmp::Reverse(p.tau),
        )
    };
    candidates
        .iter()
        .filter(|(_, t)| constraints_hold(t) && t.correct > 0)
        .max_by_key(|c| key(c))
        .copied()
}

// ---- evaluation ----

/// Distances of a whole corpus at one `tau`.
struct Evaluated {
    positives: Vec<(usize, Raw)>,
    inverted: Vec<(usize, Raw)>,
    ood: Vec<(Group, Raw)>,
}

fn evaluate(corpus: &Corpus, tau: u8, protos: &[(usize, GradientDescriptor)]) -> Evaluated {
    Evaluated {
        positives: corpus
            .positives
            .iter()
            .map(|(c, g)| (*c, raw_of(g, tau, protos)))
            .collect(),
        inverted: corpus
            .positives
            .iter()
            .map(|(c, g)| (*c, raw_of(&invert(g), tau, protos)))
            .collect(),
        ood: corpus
            .ood
            .iter()
            .map(|(group, g)| (*group, raw_of(g, tau, protos)))
            .collect(),
    }
}

fn descriptors(grids: &[(usize, [u8; GRID_LEN])], tau: u8) -> Vec<(usize, GradientDescriptor)> {
    grids
        .iter()
        .map(|(c, g)| {
            (
                *c,
                gradient_descriptor(g, tau).expect("a 9x8 grid and tau >= 1"),
            )
        })
        .collect()
}

/// Wilson 95 % interval of `k` in `n`.
fn wilson(k: u64, n: u64) -> (f64, f64) {
    if n == 0 {
        return (0.0, 1.0);
    }
    let (z, n, p) = (1.96_f64, n as f64, k as f64 / n as f64);
    let denominator = 1.0 + z * z / n;
    let centre = (p + z * z / (2.0 * n)) / denominator;
    let half = z * (p * (1.0 - p) / n + z * z / (4.0 * n * n)).sqrt() / denominator;
    ((centre - half).max(0.0), (centre + half).min(1.0))
}

fn rate(k: u64, n: u64) -> Value {
    let (lo, hi) = wilson(k, n);
    json!({ "k": k, "n": n, "rate": if n == 0 { 0.0 } else { k as f64 / n as f64 },
            "wilson95": [lo, hi] })
}

/// The metrics of a set at a setting.
fn metrics(ev: &Evaluated, distance: f64, margin: f64) -> Value {
    let t = tally(&ev.positives, &ev.ood, distance, margin);
    let accepted = t.correct + t.wrong;
    let mut per_class = serde_json::Map::new();
    for (class, label) in LABELS.iter().enumerate() {
        let mine: Vec<_> = ev.positives.iter().filter(|(c, _)| *c == class).collect();
        let hits = mine
            .iter()
            .filter(|(_, r)| decide(r, distance, margin) == Decision::Known(class))
            .count() as u64;
        per_class.insert((*label).to_owned(), rate(hits, mine.len() as u64));
    }
    let mut per_group = serde_json::Map::new();
    for group in Group::ALL {
        let mine: Vec<_> = ev.ood.iter().filter(|(g, _)| *g == group).collect();
        let hits = mine
            .iter()
            .filter(|(_, r)| matches!(decide(r, distance, margin), Decision::Known(_)))
            .count() as u64;
        per_group.insert(group.name().to_owned(), rate(hits, mine.len() as u64));
    }
    let inverted = tally(&ev.inverted, &[], distance, margin);
    json!({
        "positives": t.positives,
        "coverage": rate(t.correct, t.positives),
        "wrong_labels": rate(t.wrong, t.positives),
        "precision_of_accepted": if accepted == 0 { Value::Null } else { rate(t.correct, accepted) },
        "unknown": { "insufficient_support": t.unknown_support, "too_far": t.unknown_far,
                     "ambiguous": t.unknown_ambiguous },
        "out_of_distribution_acceptance": rate(t.ood_accepted, t.ood),
        "out_of_distribution_by_group": per_group,
        "coverage_by_class": per_class,
        "ideal_inversion": { "coverage": rate(inverted.correct, inverted.positives),
                             "wrong_labels": rate(inverted.wrong, inverted.positives) },
    })
}

// ---- the report ----

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

fn command_output(program: &str, args: &[&str]) -> Option<String> {
    let out = std::process::Command::new(program)
        .args(args)
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

/// The whole calibration, as the report.
fn calibrate(smoke: bool) -> Value {
    let (seeds, sizes) = if smoke {
        (SMOKE_SEEDS, SMOKE_SIZES)
    } else {
        (SEEDS, FULL_SIZES)
    };
    let sets: Vec<Corpus> = seeds.iter().map(|s| build_corpus(*s, sizes)).collect();
    let grids = prototype_grids();
    let file = prototype_file();

    // Selection reads the calibration set only; the holdouts wait for a chosen setting.
    let mut candidates: Vec<(Params, Tally)> = Vec::new();
    let mut calibration: Vec<(u8, Evaluated)> = Vec::new();
    for tau in TAUS {
        let ev = evaluate(&sets[0], tau, &descriptors(&grids, tau));
        for distance_step in 0..=STEPS {
            for margin_step in 0..=STEPS {
                let t = tally(
                    &ev.positives,
                    &ev.ood,
                    step_value(distance_step),
                    step_value(margin_step),
                );
                candidates.push((
                    Params {
                        tau,
                        distance_step,
                        margin_step,
                    },
                    t,
                ));
            }
        }
        calibration.push((tau, ev));
    }

    let (mut validation_evaluations, mut test_evaluations) = (0, 0);
    let chosen = select(&candidates);
    let selection = match chosen {
        None => json!({ "proposal": null,
            "reason": "no setting has both zero wrong labels and out-of-distribution acceptance within the limit on the calibration set with at least one correct label" }),
        Some((p, _)) => {
            let cal = &calibration
                .iter()
                .find(|(tau, _)| *tau == p.tau)
                .expect("the chosen tau was evaluated")
                .1;
            let (d, m) = (step_value(p.distance_step), step_value(p.margin_step));
            let protos = descriptors(&grids, p.tau);
            let validation = evaluate(&sets[1], p.tau, &protos);
            validation_evaluations += 1;
            if let Err(reason) = gate(&tally(&validation.positives, &validation.ood, d, m)) {
                json!({ "proposal": null,
                    "reason": format!("the chosen setting fails the validation gate: {reason}"),
                    "rejected_setting": { "gradient_tau": p.tau, "max_distance": d, "min_margin": m } })
            } else {
                let test = evaluate(&sets[2], p.tau, &protos);
                test_evaluations += 1;
                json!({
                    "proposal": { "gradient_tau": p.tau, "max_distance": d, "min_margin": m },
                    "calibration": metrics(cal, d, m),
                    "validation": metrics(&validation, d, m),
                    "test": metrics(&test, d, m),
                })
            }
        }
    };

    json!({
        "schema": 1,
        "protocol_version": 2,
        "evaluations": { "calibration": TAUS.len(), "validation": validation_evaluations,
                         "test": test_evaluations },
        "tool": "src/acceptance/examples/icon_calibration.rs",
        "smoke": smoke,
        "synthetic_offline": true,
        "live_capture": false,
        "input_delivered": false,
        "commit": command_output("git", &["rev-parse", "HEAD"]),
        "working_tree_dirty": command_output("git", &["status", "--porcelain"]).map(|s| !s.is_empty()),
        "binary_sha256": std::env::current_exe().ok().and_then(|p| std::fs::read(p).ok()).map(|b| sha256_hex(&b)),
        "spec_sha256": sha256_hex(include_bytes!("../../../tools/s4/icon_spec.rs")),
        "prototypes": { "file": "icons.jsonl", "sha256": sha256_hex(file.as_bytes()),
                        "count": grids.len(), "classes": CLASSES, "context": CONTEXT,
                        "max_entries": MAX_ENTRIES, "labels": LABELS },
        "protocol": {
            "seeds": seeds, "positives_per_class": sizes.positives_per_class,
            "ood_per_shape": sizes.per_shape, "ood_shapes": 16,
            "ood_per_procedural_kind": sizes.per_procedural, "ood_procedural_kinds": 4,
            "scales": SCALES, "subpixel_offsets": OFFSETS, "detector_box_margin_px": [MARGIN_RANGE.0, MARGIN_RANGE.1],
            "taus": TAUS, "distance_and_margin_steps": [0.0, step_value(STEPS), 0.01],
            "support_policy": { "min_support": SupportPolicy::default().min_support,
                                "min_overlap": SupportPolicy::default().min_overlap },
            "constraints": { "wrong_labels": 0, "ood_acceptance_percent_max": FAR_LIMIT_PERCENT,
                             "validation_coverage_percent_min": MIN_COVERAGE_PERCENT },
            "rule": "on the calibration set: the setting with the most correct labels among those with zero wrong labels and out-of-distribution acceptance <= the limit; ties: smaller max_distance, larger min_margin, smaller tau; then the validation gate; the test set is reported once",
        },
        "sizes": { "calibration": [sets[0].positives.len(), sets[0].ood.len()],
                   "validation": [sets[1].positives.len(), sets[1].ood.len()],
                   "test": [sets[2].positives.len(), sets[2].ood.len()] },
        "selection": selection,
        "limits": "synthetic corpus drawn by this tool's own rasteriser, not the test application's real rendering; one descriptor version; ideal inversion only; a proposal for the owner, never a default",
    })
}

/// Exploratory, on the development seed only: what each constraint costs on its own. It does not
/// choose anything and never touches the validation or the test set; its output informs a
/// change of protocol, which then needs new seeds and its own pre-registration.
fn diagnose() -> Value {
    let corpus = build_corpus(DEV_SEED, FULL_SIZES);
    let grids = prototype_grids();
    let mut rows = Vec::new();
    for tau in TAUS {
        let ev = evaluate(&corpus, tau, &descriptors(&grids, tau));
        let mut all: Vec<(Params, Tally)> = Vec::new();
        for distance_step in 0..=STEPS {
            for margin_step in 0..=STEPS {
                let params = Params {
                    tau,
                    distance_step,
                    margin_step,
                };
                let t = tally(
                    &ev.positives,
                    &ev.ood,
                    step_value(distance_step),
                    step_value(margin_step),
                );
                all.push((params, t));
            }
        }
        let best = |filter: &dyn Fn(&Tally) -> bool| {
            all.iter()
                .filter(|(_, t)| filter(t) && t.correct > 0)
                .max_by_key(|(p, t)| (t.correct, std::cmp::Reverse(p.distance_step), p.margin_step))
                .map(|(p, _)| {
                    let (d, m) = (step_value(p.distance_step), step_value(p.margin_step));
                    json!({ "max_distance": d, "min_margin": m, "metrics": metrics(&ev, d, m) })
                })
        };
        rows.push(json!({
            "tau": tau,
            "no_wrong_labels_only": best(&|t| t.wrong == 0),
            "ood_limit_only": best(&|t| t.ood_accepted * 100 <= t.ood * FAR_LIMIT_PERCENT),
            "both_constraints": best(&constraints_hold),
            "no_wrong_labels_and_half_coverage": all.iter()
                .filter(|(_, t)| t.wrong == 0 && t.correct * 100 >= t.positives * MIN_COVERAGE_PERCENT)
                .min_by_key(|(_, t)| t.ood_accepted)
                .map(|(p, _)| {
                    let (d, m) = (step_value(p.distance_step), step_value(p.margin_step));
                    json!({ "max_distance": d, "min_margin": m, "metrics": metrics(&ev, d, m) })
                }),
        }));
    }
    json!({ "exploratory": true, "set": "development seed only", "rows": rows })
}

fn proposal_toml(report: &Value) -> Option<String> {
    let p = report["selection"]["proposal"].as_object()?;
    Some(format!(
        "# PROPOSAL for the owner: calibrated offline on a synthetic corpus (S0 section 19).\n\
         # Nothing in the repository carries these numbers; the thesis calibrates them.\n\
         [cache]\n\
         max_entries = {MAX_ENTRIES}\n\
         prototypes = \"icons.jsonl\"\n\
         prototypes_sha256 = \"{}\"\n\
         context = \"{CONTEXT}\"\n\
         gradient_tau = {}\n\
         max_distance = {}\n\
         min_margin = {}\n",
        report["prototypes"]["sha256"].as_str()?,
        p["gradient_tau"],
        p["max_distance"],
        p["min_margin"],
    ))
}

// ---- a PNG writer (no dependency) ----

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFF_u32;
    for &b in bytes {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

fn adler32(bytes: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &x in bytes {
        a = (a + u32::from(x)) % 65_521;
        b = (b + a) % 65_521;
    }
    (b << 16) | a
}

fn chunk(out: &mut Vec<u8>, kind: [u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let start = out.len();
    out.extend_from_slice(&kind);
    out.extend_from_slice(data);
    let crc = crc32(&out[start..]);
    out.extend_from_slice(&crc.to_be_bytes());
}

/// An 8-bit greyscale PNG whose zlib stream is stored (not compressed): large, but valid and
/// byte-for-byte reproducible, and it needs no image crate.
fn png_gray(width: usize, height: usize, grey: &[u8]) -> Vec<u8> {
    assert_eq!(grey.len(), width * height);
    let mut raw = Vec::with_capacity(height * (width + 1));
    for row in grey.chunks(width) {
        raw.push(0); // filter: none
        raw.extend_from_slice(row);
    }
    let mut zlib = vec![0x78, 0x01];
    let blocks: Vec<&[u8]> = raw.chunks(65_535).collect();
    for (i, block) in blocks.iter().enumerate() {
        zlib.push(u8::from(i + 1 == blocks.len()));
        zlib.extend_from_slice(&(block.len() as u16).to_le_bytes());
        zlib.extend_from_slice(&(!(block.len() as u16)).to_le_bytes());
        zlib.extend_from_slice(block);
    }
    zlib.extend_from_slice(&adler32(&raw).to_be_bytes());
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    let mut header = Vec::new();
    header.extend_from_slice(&(width as u32).to_be_bytes());
    header.extend_from_slice(&(height as u32).to_be_bytes());
    header.extend_from_slice(&[8, 0, 0, 0, 0]); // 8 bits, greyscale, deflate, filter 0, no interlace
    chunk(&mut out, *b"IHDR", &header);
    chunk(&mut out, *b"IDAT", &zlib);
    chunk(&mut out, *b"IEND", &[]);
    out
}

// ---- the strip frames for the detector check ----

const FRAME: (usize, usize) = (1920, 1080);
/// Where the strip starts, as a fraction of the free space: centred, near the bottom.
const STRIP_TOP: f64 = 780.0;

fn strip_grey(scale: f64, x: usize, y: usize) -> u8 {
    let x0 = (FRAME.0 as f64 - strip_width() * scale) / 2.0;
    let pitch = (CELL + GAP) * scale;
    pixel(x, y, &|px, py| {
        let rel = px - x0;
        let (py_rel, i) = (py - STRIP_TOP, (rel / pitch).floor());
        if rel < 0.0 || !(0.0..CLASSES as f64).contains(&i) {
            return WINDOW_GREY;
        }
        let local = rel - i * pitch;
        if local >= CELL * scale {
            return WINDOW_GREY;
        }
        glyph_sample(&NOMINAL, glyph(i as usize), local / scale, py_rel / scale)
    })
}

fn write_strips(out: &Path) -> Result<(), Box<dyn std::error::Error>> {
    for (name, scale) in [("seq_100", 1.0), ("seq_125", 1.25), ("seq_150", 1.5)] {
        let dir = out.join(name);
        std::fs::create_dir_all(&dir)?;
        let x0 = ((FRAME.0 as f64 - strip_width() * scale) / 2.0).floor() as usize - 2;
        let x1 = x0 + (strip_width() * scale).ceil() as usize + 4;
        let (y0, y1) = (
            STRIP_TOP as usize - 2,
            STRIP_TOP as usize + (CELL * scale).ceil() as usize + 2,
        );
        let mut grey = Vec::with_capacity(FRAME.0 * FRAME.1);
        for y in 0..FRAME.1 {
            for x in 0..FRAME.0 {
                let v = if (x0..x1).contains(&x) && (y0..y1).contains(&y) {
                    strip_grey(scale, x, y)
                } else {
                    WINDOW_GREY
                };
                grey.push(v);
            }
        }
        let png = png_gray(FRAME.0, FRAME.1, &grey);
        for frame in ["000.png", "001.png"] {
            std::fs::write(dir.join(frame), &png)?;
        }
    }
    Ok(())
}

// ---- the command line ----

fn run() -> Result<(), Box<dyn std::error::Error>> {
    if cfg!(debug_assertions) {
        return Err("icon_calibration requires --release".into());
    }
    let mut args = std::env::args().skip(1);
    let (mut out, mut smoke, mut strips_only, mut diagnosing) = (None, false, false, false);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--out-dir" => out = Some(args.next().ok_or("--out-dir needs a path")?),
            "--smoke" => smoke = true,
            "--strips-only" => strips_only = true,
            "--diagnose" => diagnosing = true,
            _ => return Err(format!("unknown argument: {arg}").into()),
        }
    }
    let out = Path::new(&out.ok_or("--out-dir DIR is required")?).to_path_buf();
    std::fs::create_dir_all(&out)?;
    if diagnosing {
        std::fs::write(
            out.join("diagnosis.json"),
            serde_json::to_vec_pretty(&diagnose())?,
        )?;
        return Ok(());
    }
    write_strips(&out)?;
    if strips_only {
        return Ok(());
    }
    std::fs::write(out.join("icons.jsonl"), prototype_file())?;
    let report = calibrate(smoke);
    std::fs::write(
        out.join("calibration.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    if !smoke && let Some(toml) = proposal_toml(&report) {
        std::fs::write(out.join("cache-proposal.toml"), toml)?;
    }
    println!(
        "icons.jsonl sha256 {}\nselection: {}",
        report["prototypes"]["sha256"].as_str().unwrap_or("?"),
        report["selection"]["proposal"]
    );
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("icon_calibration: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::icon_spec::tile_origin;
    use super::*;
    use ommatix_domain::appearance::{
        CachePolicy, DESCRIPTOR_VERSION, IconCache, IconMatch, Prototype, PrototypeSource,
        UnknownReason,
    };

    fn cache_at(tau: u8) -> IconCache {
        let mut cache = IconCache::new();
        for (class, grid) in prototype_grids() {
            cache.insert(Prototype {
                label: LABELS[class].to_owned(),
                context: CONTEXT.to_owned(),
                descriptor_version: DESCRIPTOR_VERSION,
                descriptor: gradient_descriptor(&grid, tau).unwrap(),
                source: PrototypeSource::Curated,
            });
        }
        cache
    }

    fn tiny() -> Sizes {
        Sizes {
            positives_per_class: 6,
            per_shape: 2,
            per_procedural: 2,
        }
    }

    #[test]
    fn the_rasteriser_paints_ink_under_the_glyph_and_the_window_elsewhere() {
        let mut rng = Rng::new(7);
        // The plus: arms 12..44 and 24..32 in design units; a two-pixel margin at 100 %.
        let view = view_around(glyph(0), 1.0, [2; 4], (0.0, 0.0));
        assert_eq!((view.width, view.height), (37, 37));
        let frame = render(&view, &NOMINAL, glyph(0), &mut rng);
        let at = |x: usize, y: usize| frame.pixels()[(y * view.width + x) * 4];
        assert_eq!(at(0, 0), WINDOW_GREY, "the corner is window");
        assert_eq!(at(18, 18), INK_GREY, "the plus covers the centre");
        assert_eq!(at(18, 2), INK_GREY, "and the top of its vertical arm");
        assert_eq!(at(4, 4), WINDOW_GREY, "but not the corner of its box");
    }

    #[test]
    fn a_detector_box_follows_the_glyph_and_its_margins() {
        let tight = view_around(glyph(0), 1.0, [0; 4], (0.0, 0.0));
        let loose = view_around(glyph(0), 1.0, [3, 1, 2, 4], (0.0, 0.0));
        assert_eq!((tight.width, tight.height), (33, 33));
        assert_eq!((loose.width, loose.height), (38, 38));
        // The chevron is narrower than the plus.
        assert!(view_around(glyph(7), 1.0, [0; 4], (0.0, 0.0)).width < tight.width);
        // A cut into the ink shrinks the crop, but never below the minimum.
        assert_eq!(
            view_around(glyph(0), 1.0, [-20; 4], (0.0, 0.0)).width,
            MIN_CROP as usize
        );
        let mut rng = Rng::new(3);
        for _ in 0..200 {
            let v = random_view(&mut rng, glyph(3));
            assert!(SCALES.contains(&v.scale));
            assert!(v.width >= MIN_CROP as usize && v.height >= MIN_CROP as usize);
        }
    }

    #[test]
    fn every_class_has_a_distinct_glyph_and_label() {
        for (a, label) in LABELS.iter().enumerate() {
            assert_ne!(glyph(a), [], "class {a} has shapes");
            for (b, other) in LABELS.iter().enumerate().skip(a + 1) {
                assert_ne!(glyph(a), glyph(b));
                assert_ne!(label, other);
            }
        }
        assert_eq!(glyph(CLASSES), []);
    }

    #[test]
    fn the_prototype_file_is_deterministic_and_in_the_operators_format() {
        let file = prototype_file();
        assert_eq!(file, prototype_file());
        let lines: Vec<&str> = file.lines().collect();
        assert_eq!(lines.len(), CLASSES * 4);
        assert!(lines.len() as u32 <= MAX_ENTRIES);
        assert!(file.ends_with('\n') && !file.contains("\n\n"));
        for line in lines {
            let v: Value = serde_json::from_str(line).unwrap();
            let object = v.as_object().unwrap();
            assert_eq!(object.len(), 3, "exactly label, context and grid");
            assert!(LABELS.contains(&object["label"].as_str().unwrap()));
            assert_eq!(object["context"], CONTEXT);
            let grid = object["grid"].as_array().unwrap();
            assert_eq!(grid.len(), GRID_LEN);
            assert!(grid.iter().all(|g| g.as_u64().is_some_and(|g| g <= 255)));
        }
    }

    /// The measurement configuration of S0 §19.4 (option C, not a calibration): `gradient_tau`
    /// 255, `max_distance` 0 and `min_margin` 1. A gradient is valid only between two cell means
    /// that differ by 255, so no prototype of the strip (ink 40 on 248) has any support, no crop
    /// of the corpus reaches the eight valid gradients the lookup needs, and the cache answers
    /// `unknown` before it compares anything. These are test values, not defaults of any build.
    #[test]
    fn the_measurement_configuration_accepts_nothing_in_the_corpus() {
        let (tau, policy) = (255, CachePolicy::new(0.0, 1.0).unwrap());
        for (_, grid) in prototype_grids() {
            assert_eq!(gradient_descriptor(&grid, tau).unwrap().support(), 0);
        }
        let cache = cache_at(tau);
        let corpus = build_corpus(0xABCD, tiny());
        let samples = corpus
            .positives
            .iter()
            .map(|(_, g)| *g)
            .chain(corpus.ood.iter().map(|(_, g)| *g))
            .chain(prototype_grids().into_iter().map(|(_, g)| g));
        for grid in samples {
            let query = gradient_descriptor(&grid, tau).unwrap();
            assert_eq!(
                cache.lookup(&query, CONTEXT, policy).unwrap(),
                IconMatch::Unknown(UnknownReason::InsufficientSupport)
            );
        }
    }

    /// R105-4: the guarantee of the measurement configuration does not rest on the query being
    /// low in contrast. At `gradient_tau` 255 every one of the 32 pinned prototypes has an empty
    /// mask, so its support in common with ANY query is zero: even a query with all 64 gradients
    /// valid (alternating black and white) passes the early query-support check, is compared
    /// with the eligible prototypes and is refused for want of common support. A different
    /// prototype file would be a different configuration, with another SHA-256.
    #[test]
    fn the_pinned_prototypes_refuse_every_query_at_the_measurement_settings() {
        let tau = 255;
        for (_, grid) in prototype_grids() {
            assert_eq!(gradient_descriptor(&grid, tau).unwrap().mask, 0);
        }
        let black_white: Vec<u8> = (0..GRID_LEN)
            .map(|i| if i % 2 == 0 { 0 } else { 255 })
            .collect();
        let query = gradient_descriptor(&black_white, tau).unwrap();
        assert_eq!(query.support(), 64, "every gradient of the query is valid");
        let policy = CachePolicy::new(0.0, 1.0).unwrap();
        assert_eq!(
            cache_at(tau).lookup(&query, CONTEXT, policy).unwrap(),
            IconMatch::Unknown(UnknownReason::InsufficientSupport)
        );
        // Whatever the thresholds, with nothing in common nothing is within any distance.
        for (d, m) in [(0.0, 0.0), (0.5, 0.0), (1.0, 0.0)] {
            assert_eq!(
                cache_at(tau)
                    .lookup(&query, CONTEXT, CachePolicy::new(d, m).unwrap())
                    .unwrap(),
                IconMatch::Unknown(UnknownReason::InsufficientSupport)
            );
        }
    }

    /// The bytes of the curated file are pinned: the operator pins their SHA-256, and an edit of
    /// `tools/s4/icon_spec.rs` or of the rasteriser that changed them would silently invalidate
    /// every pin. The value is the one of attempt 2 (S0 §19.3); change it only with the spec,
    /// in a commit that says so.
    #[test]
    fn the_prototype_file_has_the_pinned_digest() {
        assert_eq!(
            sha256_hex(prototype_file().as_bytes()),
            "d43a784c8376772f89d63c997cc99fa7df458cea5d78324c5727f2dec71d14a8"
        );
    }

    /// Every prototype is closest to its own class and to no other class at distance zero: two
    /// classes that a prototype cannot tell apart would be `Ambiguous` for every margin, so the
    /// strip would label neither. Checked at gradient thresholds across the searched range.
    #[test]
    fn each_prototype_is_closest_to_its_own_class_and_to_no_other_at_zero() {
        let grids = prototype_grids();
        for tau in [8, 16, 32] {
            let protos = descriptors(&grids, tau);
            for (class, grid) in &grids {
                let raw = raw_of(grid, tau, &protos);
                let (d, best) = raw.best.unwrap_or_else(|| {
                    panic!(
                        "tau {tau}: class {class} matches nothing, support {:?}",
                        gradient_descriptor(grid, tau).map(|d| d.support())
                    )
                });
                assert!(raw.support_ok, "tau {tau} class {class}");
                assert_eq!((d, best), (0.0, *class), "tau {tau}");
                // No other class within the support policy is better still.
                if let Some(next) = raw.next {
                    assert!(
                        next > 0.0,
                        "tau {tau}: class {class} ties with another class"
                    );
                }
            }
        }
    }

    /// The rule that the sweep applies is `IconCache::lookup`, not a copy that drifts: the same
    /// answer for every setting on a corpus that mixes classes and out-of-distribution shapes.
    #[test]
    fn the_sweeps_decision_is_what_icon_cache_lookup_answers() {
        let corpus = build_corpus(0xABCD, tiny());
        let grids = prototype_grids();
        let samples: Vec<[u8; GRID_LEN]> = corpus
            .positives
            .iter()
            .map(|(_, g)| *g)
            .chain(corpus.ood.iter().map(|(_, g)| *g))
            .collect();
        for tau in [4, 16, 48] {
            let protos = descriptors(&grids, tau);
            let cache = cache_at(tau);
            for (distance_step, margin_step) in
                [(0, 0), (5, 0), (12, 3), (25, 10), (50, 50), (30, 0)]
            {
                let (d, m) = (step_value(distance_step), step_value(margin_step));
                let policy = CachePolicy::new(d, m).unwrap();
                for grid in &samples {
                    let query = gradient_descriptor(grid, tau).unwrap();
                    let expected = match cache.lookup(&query, CONTEXT, policy).unwrap() {
                        IconMatch::Known { label, .. } => {
                            Decision::Known(LABELS.iter().position(|l| *l == label).unwrap())
                        }
                        IconMatch::Unknown(_) => Decision::Unknown(Reason::Support),
                    };
                    let got = decide(&raw_of(grid, tau, &protos), d, m);
                    // The reasons are compared as known or unknown here; the three kinds of
                    // unknown only feed the report.
                    assert_eq!(
                        matches!(got, Decision::Known(_)),
                        matches!(expected, Decision::Known(_)),
                        "tau {tau} distance {d} margin {m}"
                    );
                    if let (Decision::Known(a), Decision::Known(b)) = (got, expected) {
                        assert_eq!(a, b);
                    }
                }
            }
        }
    }

    fn t(correct: u64, wrong: u64, ood_accepted: u64) -> Tally {
        Tally {
            positives: 100,
            correct,
            wrong,
            ood: 200,
            ood_accepted,
            ..Tally::default()
        }
    }

    fn p(tau: u8, distance_step: usize, margin_step: usize) -> Params {
        Params {
            tau,
            distance_step,
            margin_step,
        }
    }

    #[test]
    fn the_selection_takes_the_most_correct_labels_that_satisfy_the_constraints() {
        let candidates = [
            (p(8, 10, 5), t(60, 0, 1)),
            (p(8, 20, 5), t(90, 1, 0)), // a wrong label: out
            (p(8, 30, 5), t(95, 0, 3)), // 3 of 200 is 1.5 %: out
            (p(8, 40, 5), t(80, 0, 2)), // exactly 1 %: allowed
        ];
        assert_eq!(select(&candidates).unwrap().0, p(8, 40, 5));
    }

    #[test]
    fn the_selection_breaks_ties_by_distance_then_margin_then_tau() {
        let tied = t(70, 0, 0);
        let best = |list: &[(Params, Tally)]| select(list).unwrap().0;
        assert_eq!(
            best(&[(p(8, 20, 5), tied), (p(8, 10, 5), tied)]),
            p(8, 10, 5)
        );
        assert_eq!(
            best(&[(p(8, 10, 5), tied), (p(8, 10, 9), tied)]),
            p(8, 10, 9)
        );
        assert_eq!(
            best(&[(p(12, 10, 9), tied), (p(6, 10, 9), tied)]),
            p(6, 10, 9)
        );
    }

    #[test]
    fn a_setting_that_accepts_nothing_is_not_a_proposal() {
        assert!(select(&[(p(8, 0, 0), t(0, 0, 0))]).is_none());
        assert!(select(&[]).is_none());
        assert!(select(&[(p(8, 5, 5), t(10, 1, 0))]).is_none());
    }

    #[test]
    fn the_validation_gate_names_what_fails() {
        assert!(gate(&t(50, 0, 2)).is_ok());
        assert!(gate(&t(49, 0, 0)).unwrap_err().contains("coverage"));
        assert!(gate(&t(80, 1, 0)).unwrap_err().contains("wrong"));
        assert!(
            gate(&t(80, 0, 3))
                .unwrap_err()
                .contains("out-of-distribution")
        );
    }

    #[test]
    fn wilson_brackets_the_rate() {
        let (lo, hi) = wilson(50, 100);
        assert!(lo < 0.5 && 0.5 < hi && (hi - lo) < 0.21);
        assert_eq!(wilson(0, 0), (0.0, 1.0));
        let (lo, hi) = wilson(0, 800);
        assert_eq!(lo, 0.0);
        assert!(hi < 0.005);
    }

    #[test]
    fn the_corpus_is_reproducible_and_the_sets_are_disjoint_by_seed() {
        let a = build_corpus(SMOKE_SEEDS[0], tiny());
        let b = build_corpus(SMOKE_SEEDS[0], tiny());
        let c = build_corpus(SMOKE_SEEDS[1], tiny());
        assert_eq!(a.positives, b.positives);
        assert_eq!(a.ood, b.ood);
        assert_ne!(a.positives, c.positives);
        assert_eq!(a.positives.len(), CLASSES * 6);
        assert_eq!(a.ood.len(), 16 * 2 + 4 * 2);
        assert_eq!(
            SEEDS.len(),
            SEEDS
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
        );
    }

    /// R105-3: the holdouts are touched only after a setting was chosen on the calibration set
    /// and only as far as the rule needs: the validation set for the chosen `tau`, the test set
    /// only once the validation gate passed. A calibration that finds no setting never
    /// evaluates either of them. (The two historical calibrations, `6e561f7` and `1fa369e`,
    /// computed their distances eagerly for every `tau` and consulted none of them; S0 §19.3
    /// says so.)
    #[test]
    fn a_calibration_without_a_setting_never_evaluates_the_holdouts() {
        let report = calibrate(true);
        assert!(report["selection"]["proposal"].is_null());
        assert_eq!(
            report["evaluations"],
            json!({ "calibration": TAUS.len(), "validation": 0, "test": 0 })
        );
        assert!(report["selection"].get("validation").is_none());
        assert!(report["selection"].get("test").is_none());
    }

    #[test]
    fn a_calibration_on_a_tiny_corpus_reports_its_protocol() {
        let report = calibrate(true);
        assert_eq!(report["smoke"], true);
        assert_eq!(report["prototypes"]["count"], CLASSES * 4);
        assert_eq!(report["protocol"]["constraints"]["wrong_labels"], 0);
        assert!(report["selection"].get("proposal").is_some());
        assert_eq!(report["live_capture"], false);
    }

    #[test]
    fn a_proposal_becomes_a_cache_block_and_no_proposal_becomes_nothing() {
        let with = json!({
            "prototypes": { "sha256": "ab" },
            "selection": { "proposal": { "gradient_tau": 16, "max_distance": 0.2, "min_margin": 0.05 } },
        });
        let toml = proposal_toml(&with).unwrap();
        assert!(toml.contains("gradient_tau = 16") && toml.contains("max_distance = 0.2"));
        assert!(toml.contains("PROPOSAL"));
        let without =
            json!({ "prototypes": { "sha256": "ab" }, "selection": { "proposal": null } });
        assert!(proposal_toml(&without).is_none());
    }

    /// The PNG writer's output is a valid PNG: signature, chunk CRCs, and a stored zlib stream
    /// that holds exactly the filtered scanlines.
    #[test]
    fn the_png_writer_makes_a_valid_stored_png() {
        let (w, h) = (300, 250); // more than one 65 535-byte block
        let grey: Vec<u8> = (0..w * h).map(|i| (i * 7 % 251) as u8).collect();
        let png = png_gray(w, h, &grey);
        assert_eq!(&png[..8], &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
        let (mut at, mut idat, mut kinds) = (8, Vec::new(), Vec::new());
        while at < png.len() {
            let len = u32::from_be_bytes(png[at..at + 4].try_into().unwrap()) as usize;
            let kind: [u8; 4] = png[at + 4..at + 8].try_into().unwrap();
            let data = &png[at + 8..at + 8 + len];
            let crc = u32::from_be_bytes(png[at + 8 + len..at + 12 + len].try_into().unwrap());
            assert_eq!(crc, crc32(&png[at + 4..at + 8 + len]), "CRC of {kind:?}");
            if &kind == b"IDAT" {
                idat.extend_from_slice(data);
            }
            kinds.push(kind);
            at += 12 + len;
        }
        assert_eq!(kinds, [*b"IHDR", *b"IDAT", *b"IEND"]);
        assert_eq!(&png[16..24], &[0, 0, 1, 44, 0, 0, 0, 250]);
        // Walk the stored blocks and rebuild the scanlines.
        assert_eq!(&idat[..2], &[0x78, 0x01]);
        let (mut i, mut raw) = (2, Vec::new());
        loop {
            let last = idat[i] == 1;
            let len = usize::from(u16::from_le_bytes([idat[i + 1], idat[i + 2]]));
            let nlen = u16::from_le_bytes([idat[i + 3], idat[i + 4]]);
            assert_eq!(nlen, !(len as u16));
            raw.extend_from_slice(&idat[i + 5..i + 5 + len]);
            i += 5 + len;
            if last {
                break;
            }
        }
        assert_eq!(idat[i..], adler32(&raw).to_be_bytes());
        let expected: Vec<u8> = grey
            .chunks(w)
            .flat_map(|row| std::iter::once(0).chain(row.iter().copied()))
            .collect();
        assert_eq!(raw, expected);
        // Known values of the checksums.
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
    }

    #[test]
    fn the_strip_frame_paints_every_glyph_and_leaves_the_rest_of_the_window() {
        let x0 = (FRAME.0 as f64 - strip_width()) / 2.0;
        let (px, py) = tile_origin(0, x0, STRIP_TOP, 1.0);
        // The centre of cell 0 (a plus) is ink; its corner and the window far from the strip are
        // the window grey.
        assert_eq!(
            strip_grey(1.0, (px + CELL / 2.0) as usize, (py + CELL / 2.0) as usize),
            INK_GREY
        );
        assert_eq!(
            strip_grey(1.0, (px + 2.0) as usize, (py + 2.0) as usize),
            WINDOW_GREY
        );
        assert_eq!(strip_grey(1.0, 10, 10), WINDOW_GREY);
    }
}
