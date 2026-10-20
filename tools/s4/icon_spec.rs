//! The synthetic icon strip of the icon-cache soak (dossier chapter 06 §6.7, box 7): one
//! description of eight text-less icons, drawn by code, shared by two programs through
//! `#[path]`: the test application (`tools/s4/testapp`, behind its hidden `--icon-strip` flag),
//! which paints them with egui, and the calibration tool (`src/acceptance/examples/
//! icon_calibration.rs`), which rasterises the same shapes in software to generate the
//! operator's prototypes and the offline corpus.
//!
//! Plain data and plain functions with no dependency, so both workspaces can include the file.
//! Coordinates are design units of a [`TILE`]-sized tile (one unit is one pixel at 100 %
//! scaling). Nothing here comes from a screen capture.
// Each program uses a different part of this file.
#![allow(dead_code)]

/// Side of one icon tile, in design units.
pub const TILE: f64 = 56.0;
/// Gap between two tiles of the strip, in design units.
pub const GAP: f64 = 24.0;
/// How many icons the strip shows, and how many classes the prototypes label.
pub const CLASSES: usize = 8;
/// [`CLASSES`] as a number of design units.
const CLASSES_F: f64 = 8.0;
/// Width of the tile border, in design units.
pub const BORDER: f64 = 2.0;

/// Grey levels of the nominal rendering (the prototypes use these).
pub const WINDOW_GREY: u8 = 248;
pub const TILE_GREY: u8 = 236;
pub const BORDER_GREY: u8 = 140;
pub const INK_GREY: u8 = 40;

/// The synthetic label of each class: a closed vocabulary, nothing read from a screen.
pub const LABELS: [&str; CLASSES] = [
    "cache-icon-01",
    "cache-icon-02",
    "cache-icon-03",
    "cache-icon-04",
    "cache-icon-05",
    "cache-icon-06",
    "cache-icon-07",
    "cache-icon-08",
];

/// One filled primitive of a glyph.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Shape {
    /// A line with round caps.
    Segment {
        a: (f64, f64),
        b: (f64, f64),
        width: f64,
    },
    /// A ring: outer radius `r`, thickness `width`.
    Ring { c: (f64, f64), r: f64, width: f64 },
    /// A filled disc.
    Disc { c: (f64, f64), r: f64 },
    /// A filled axis-aligned rectangle.
    Rect { x0: f64, y0: f64, x1: f64, y1: f64 },
    /// A filled convex polygon, in order.
    Poly(&'static [(f64, f64)]),
}

const PLAY: [(f64, f64); 3] = [(19.0, 12.0), (19.0, 44.0), (45.0, 28.0)];

const PLUS: [Shape; 2] = [
    Shape::Rect {
        x0: 24.0,
        y0: 12.0,
        x1: 32.0,
        y1: 44.0,
    },
    Shape::Rect {
        x0: 12.0,
        y0: 24.0,
        x1: 44.0,
        y1: 32.0,
    },
];
const RING: [Shape; 1] = [Shape::Ring {
    c: (28.0, 28.0),
    r: 16.0,
    width: 5.0,
}];
const TRIANGLE: [Shape; 1] = [Shape::Poly(&PLAY)];
const SQUARE: [Shape; 4] = [
    Shape::Rect {
        x0: 12.0,
        y0: 12.0,
        x1: 44.0,
        y1: 17.0,
    },
    Shape::Rect {
        x0: 12.0,
        y0: 39.0,
        x1: 44.0,
        y1: 44.0,
    },
    Shape::Rect {
        x0: 12.0,
        y0: 17.0,
        x1: 17.0,
        y1: 39.0,
    },
    Shape::Rect {
        x0: 39.0,
        y0: 17.0,
        x1: 44.0,
        y1: 39.0,
    },
];
const CROSS: [Shape; 2] = [
    Shape::Segment {
        a: (14.0, 14.0),
        b: (42.0, 42.0),
        width: 6.0,
    },
    Shape::Segment {
        a: (42.0, 14.0),
        b: (14.0, 42.0),
        width: 6.0,
    },
];
const TARGET: [Shape; 2] = [
    Shape::Ring {
        c: (28.0, 28.0),
        r: 18.0,
        width: 4.0,
    },
    Shape::Disc {
        c: (28.0, 28.0),
        r: 7.0,
    },
];
const BARS: [Shape; 3] = [
    Shape::Rect {
        x0: 12.0,
        y0: 14.0,
        x1: 44.0,
        y1: 20.0,
    },
    Shape::Rect {
        x0: 12.0,
        y0: 25.0,
        x1: 44.0,
        y1: 31.0,
    },
    Shape::Rect {
        x0: 12.0,
        y0: 36.0,
        x1: 44.0,
        y1: 42.0,
    },
];
const CHEVRON: [Shape; 2] = [
    Shape::Segment {
        a: (20.0, 12.0),
        b: (38.0, 28.0),
        width: 6.0,
    },
    Shape::Segment {
        a: (38.0, 28.0),
        b: (20.0, 44.0),
        width: 6.0,
    },
];

/// The shapes of class `class` (0..[`CLASSES`]), in a [`TILE`]-sized tile. Empty for an index
/// out of range.
#[must_use]
pub fn glyph(class: usize) -> &'static [Shape] {
    match class {
        0 => &PLUS,
        1 => &RING,
        2 => &TRIANGLE,
        3 => &SQUARE,
        4 => &CROSS,
        5 => &TARGET,
        6 => &BARS,
        7 => &CHEVRON,
        _ => &[],
    }
}

/// Top-left corner of tile `i` of a strip whose first tile starts at `x0`, in design units
/// scaled by `scale` (1.0 at 100 %).
#[must_use]
pub fn tile_origin(i: u8, x0: f64, y0: f64, scale: f64) -> (f64, f64) {
    (x0 + (TILE + GAP) * scale * f64::from(i), y0)
}

/// Width of the whole strip, in design units.
#[must_use]
pub fn strip_width() -> f64 {
    TILE * CLASSES_F + GAP * (CLASSES_F - 1.0)
}
