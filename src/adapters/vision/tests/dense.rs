//! Screens packed with tiny segments: halftone dots, noise-like tiles. Grouping them must
//! stay near-linear in the number of segments. It took 15 s per 1080p frame when every
//! neighbour search scanned a 64-px bucket of some 450 boxes.

use ommatix_adapter_vision::FlatRegionDetector;
use ommatix_domain::{Frame, Rect};

const WHITE: [u8; 4] = [255; 4];
const INK: [u8; 4] = [0, 0, 0, 255];

fn detect(f: &Frame) -> Vec<Rect> {
    FlatRegionDetector::default()
        .detect_blobs(f, f.bounds())
        .unwrap()
        .into_iter()
        .map(|d| d.bounds)
        .collect()
}

/// Paints `cols` x `rows` ink dots of 2x2 px at a 3 px pitch (1 px apart) from `(x, y)`,
/// and returns the area they cover.
fn paint_dots(f: &mut Frame, x: u32, y: u32, cols: u32, rows: u32) -> Rect {
    for row in 0..rows {
        for col in 0..cols {
            f.fill_rect(Rect::new(x + col * 3, y + row * 3, 2, 2), INK);
        }
    }
    Rect::new(x, y, cols * 3 - 1, rows * 3 - 1)
}

/// A white `w` x `h` frame covered with 2x2 dots at a 3 px pitch.
fn dotted_screen(w: u32, h: u32) -> Frame {
    let mut f = Frame::solid(w, h, WHITE).unwrap();
    paint_dots(&mut f, 0, 0, w / 3, h / 3);
    f
}

/// A `w` x `h` frame of 3x3 tiles, each a colour from a fixed-seed xorshift generator.
fn random_tiles(w: u32, h: u32) -> Frame {
    let mut state = 0x9e37_79b9_7f4a_7c15_u64;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let cols = w.div_ceil(3) as usize;
    let tiles: Vec<[u8; 4]> = (0..cols * h.div_ceil(3) as usize)
        .map(|_| {
            let [r, g, b, ..] = next().to_le_bytes();
            [r, g, b, 255]
        })
        .collect();
    let mut rgba = Vec::with_capacity(w as usize * h as usize * 4);
    for y in 0..h as usize {
        for x in 0..w as usize {
            rgba.extend_from_slice(&tiles[y / 3 * cols + x / 3]);
        }
    }
    Frame::from_rgba(w, h, rgba).unwrap()
}

#[test]
fn a_dotted_area_is_one_element_and_keeps_its_neighbours_apart() {
    let mut f = Frame::solid(200, 120, WHITE).unwrap();
    let dots = paint_dots(&mut f, 20, 10, 40, 30);
    // A word 4 px right of the dots, one more than the merge gap: a separate element.
    for i in 0..3 {
        f.fill_rect(Rect::new(dots.right() + 4 + i * 8, 40, 6, 10), INK);
    }
    assert_eq!(dots, Rect::new(20, 10, 119, 89));
    assert_eq!(detect(&f), vec![dots, Rect::new(143, 40, 22, 10)]);
}

/// 57 600 dots, each its own segment: grouping took about 50 s in a debug build.
#[test]
fn a_dotted_screen_is_one_element() {
    let f = dotted_screen(960, 540);
    assert_eq!(detect(&f), vec![Rect::new(0, 0, 959, 539)]);
}

/// 57 600 tiles touching their neighbours; similar neighbours merge into bigger segments
/// whose boxes repeat the tiles inside them (about 56 000 boxes reach grouping). Grouping
/// took about 50 s in a debug build.
#[test]
fn random_tiles_are_grouped_deterministically() {
    let f = random_tiles(960, 540);
    let found = detect(&f);
    assert_eq!(found, vec![Rect::new(0, 0, 960, 540)]);
    assert_eq!(found, detect(&f));
}

/// Timing on full-HD dense screens: `cargo test -p ommatix-adapter-vision --release --
/// --ignored --nocapture`.
#[test]
#[ignore = "benchmark"]
fn bench_dense_screens() {
    let (w, h) = (1920, 1080);
    for (name, f) in [
        ("2x2 dots, 3 px pitch", dotted_screen(w, h)),
        ("3x3 random tiles", random_tiles(w, h)),
    ] {
        let start = std::time::Instant::now();
        let found = detect(&f);
        eprintln!(
            "{w}x{h} {name}: {} found in {:?}",
            found.len(),
            start.elapsed()
        );
    }
}
