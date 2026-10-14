//! The perception engine with the real detector, on screens built to break it. Each test
//! is a regression test for a finding of the 2026-10-14 review or of its verification
//! pass (docs/reviews).

use std::collections::VecDeque;
use std::sync::Arc;

use ommatix_adapter_vision::FlatRegionDetector;
use ommatix_application::{
    ElementDetector, FrameSource, ManualClock, PerceptionConfig, PerceptionEngine, PortError,
};
use ommatix_domain::{Frame, Point, Rect};

const WHITE: [u8; 4] = [255; 4];
const BLUE: [u8; 4] = [30, 90, 200, 255];
const RED: [u8; 4] = [220, 20, 20, 255];
const INK: [u8; 4] = [0, 0, 0, 255];
const WALLPAPER: [u8; 4] = [120, 140, 170, 255];
const DARK: [u8; 4] = [40, 42, 48, 255];
const HOVER: [u8; 4] = [90, 96, 110, 255];

struct Script(VecDeque<Frame>);

impl FrameSource for Script {
    fn name(&self) -> &'static str {
        "script"
    }
    fn next_frame(&mut self) -> Result<Option<Frame>, PortError> {
        Ok(self.0.pop_front())
    }
}

fn engine(frames: Vec<Frame>) -> PerceptionEngine {
    PerceptionEngine::new(
        Box::new(Script(frames.into())),
        Box::new(FlatRegionDetector::default()),
        Arc::new(ManualClock::new()),
        PerceptionConfig::default(),
    )
    .unwrap()
}

fn bounds(e: &PerceptionEngine) -> Vec<Rect> {
    e.elements().iter().map(|e| e.bounds).collect()
}

fn sorted(mut boxes: Vec<Rect>) -> Vec<Rect> {
    boxes.sort_by_key(|r| (r.y, r.x, r.w, r.h));
    boxes
}

/// What the detector finds when it analyses the whole frame at once.
fn full_analysis(frame: &Frame) -> Vec<Rect> {
    let found = FlatRegionDetector::default()
        .detect(frame, frame.bounds())
        .unwrap();
    sorted(found.into_iter().map(|d| d.bounds).collect())
}

/// What the scene holds after perceiving `frames` one by one.
fn incremental(frames: Vec<Frame>) -> Vec<Rect> {
    let n = frames.len();
    let mut e = engine(frames);
    for _ in 0..n {
        e.tick().unwrap();
    }
    sorted(bounds(&e))
}

/// Review #5: growing a region over one element must also take in the neighbours the
/// grown region now touches, or they are re-detected as fragments.
#[test]
fn neighbours_of_a_grown_region_are_re_detected_whole() {
    let mut f1 = Frame::solid(128, 128, WHITE).unwrap();
    f1.fill_rect(Rect::new(2, 2, 38, 38), BLUE); // A
    f1.fill_rect(Rect::new(2, 50, 18, 40), BLUE); // C, below A
    let mut f2 = f1.clone();
    f2.fill_rect(Rect::new(36, 36, 2, 2), RED); // a change inside A
    let mut e = engine(vec![f1, f2.clone(), f2]);

    e.tick().unwrap();
    let r = e.tick().unwrap().unwrap();
    assert_eq!(r.analysed_regions, vec![Rect::new(2, 2, 62, 88)]);
    e.tick().unwrap();
    assert_eq!(
        bounds(&e),
        vec![
            Rect::new(2, 2, 38, 38),
            Rect::new(36, 36, 2, 2),
            Rect::new(2, 50, 18, 40)
        ]
    );
}

/// Review #1 and #10: a panel the change happens on is a container, not something to
/// re-detect, and must survive a change on its surface.
#[test]
fn a_solid_panel_survives_a_change_on_its_surface() {
    let mut f1 = Frame::solid(256, 256, WHITE).unwrap();
    f1.fill_rect(Rect::new(32, 32, 192, 192), BLUE);
    let mut f2 = f1.clone();
    f2.fill_rect(Rect::new(110, 110, 6, 12), WHITE); // a glyph appears on the panel
    let mut e = engine(vec![f1, f2]);

    let panel = e.tick().unwrap().unwrap().delta.added[0];
    let r = e.tick().unwrap().unwrap();
    assert_eq!(r.analysed_regions, vec![Rect::new(96, 96, 32, 32)]);
    assert_eq!(r.delta.removed.len(), 0);
    assert_eq!(e.element(panel.id), Some(panel));
    assert_eq!(
        e.elements_at(Point::new(112, 112))[0].bounds,
        Rect::new(110, 110, 6, 12)
    );
}

/// Review #10: typing one glyph inside a big window re-analyses one cell, not the window.
#[test]
fn a_change_inside_a_window_costs_one_cell() {
    let mut f1 = Frame::solid(1024, 768, WHITE).unwrap();
    f1.fill_rect(Rect::new(10, 10, 1000, 740), BLUE); // window frame (2 px ring)
    f1.fill_rect(Rect::new(12, 12, 996, 736), WHITE);
    let mut f2 = f1.clone();
    f2.fill_rect(Rect::new(500, 400, 6, 12), INK);
    let mut e = engine(vec![f1, f2]);

    e.tick().unwrap();
    let r = e.tick().unwrap().unwrap();
    assert_eq!(r.analysed_regions, vec![Rect::new(480, 384, 32, 32)]);
    assert_eq!(r.analysed_px, 32 * 32);
    assert_eq!(r.delta.added.len(), 1);
    assert_eq!(r.element_count, 2, "window and glyph");
}

/// Review #1: a window covering most of a contrasting wallpaper is an element, the
/// wallpaper is not, and a change inside the window stays local.
#[test]
fn a_contrasting_wallpaper_yields_no_phantom_and_stays_incremental() {
    let mut f1 = Frame::solid(640, 400, [120, 140, 170, 255]).unwrap();
    f1.fill_rect(Rect::new(40, 40, 560, 320), WHITE); // window
    f1.fill_rect(Rect::new(40, 40, 560, 28), [46, 52, 64, 255]); // title bar
    f1.fill_rect(Rect::new(500, 300, 80, 32), BLUE); // Save
    let mut f2 = f1.clone();
    f2.fill_rect(Rect::new(100, 200, 6, 12), INK);
    let mut e = engine(vec![f1, f2]);

    e.tick().unwrap();
    assert_eq!(
        bounds(&e),
        vec![
            Rect::new(40, 40, 560, 28),
            Rect::new(40, 68, 560, 292),
            Rect::new(500, 300, 80, 32)
        ]
    );
    let r = e.tick().unwrap().unwrap();
    assert_eq!(r.analysed_regions, vec![Rect::new(96, 192, 32, 32)]);
}

/// Verification N1: whether something on the screen edge is background must not depend on
/// which region is analysed. A bar along the top edge and a taskbar are surfaces in any
/// region, and a hover inside the taskbar removes nothing.
#[test]
fn the_screen_edge_is_judged_the_same_in_every_region() {
    let wallpaper = Frame::solid(640, 400, WALLPAPER).unwrap();
    let mut bar = wallpaper.clone();
    bar.fill_rect(Rect::new(0, 0, 640, 24), DARK);
    bar.fill_rect(Rect::new(8, 6, 40, 12), WHITE); // its label
    assert_eq!(
        incremental(vec![wallpaper.clone(), bar.clone()]),
        full_analysis(&bar)
    );
    assert_eq!(full_analysis(&bar), vec![Rect::new(8, 6, 40, 12)]);

    let mut taskbar = wallpaper;
    taskbar.fill_rect(Rect::new(0, 360, 640, 40), DARK);
    taskbar.fill_rect(Rect::new(104, 368, 24, 24), BLUE); // an icon
    let mut hover = taskbar.clone();
    hover.fill_rect(Rect::new(96, 362, 40, 36), HOVER);
    hover.fill_rect(Rect::new(104, 368, 24, 24), BLUE);
    assert_eq!(
        incremental(vec![taskbar, hover.clone()]),
        full_analysis(&hover)
    );
    assert_eq!(
        full_analysis(&hover),
        vec![Rect::new(96, 362, 40, 36), Rect::new(104, 368, 24, 24)]
    );
}

/// Review #1, verification pass: a window snapped to one side shares the screen border
/// with the wallpaper. Both are surfaces: neither becomes a phantom element, and what is
/// drawn on them is found.
#[test]
fn a_snapped_window_and_the_wallpaper_are_both_surfaces() {
    for width in [320, 426, 540] {
        let mut f = Frame::solid(640, 400, WALLPAPER).unwrap();
        f.fill_rect(Rect::new(0, 0, width, 400), WHITE);
        f.fill_rect(Rect::new(100, 300, 80, 32), BLUE); // button in the window
        f.fill_rect(Rect::new(580, 40, 32, 32), BLUE); // icon on the desktop
        assert_eq!(
            full_analysis(&f),
            vec![Rect::new(580, 40, 32, 32), Rect::new(100, 300, 80, 32)],
            "window {width} px wide"
        );
    }
}

/// Verification N7: a 1-px diagonal line crossing a re-analysed region is part of the
/// line outside it, not a new fragment.
#[test]
fn a_diagonal_line_through_a_region_leaves_no_fragment() {
    let mut f1 = Frame::solid(320, 320, WHITE).unwrap();
    for i in 50..300 {
        f1.fill_rect(Rect::new(i, i, 1, 1), INK);
    }
    let mut f2 = f1.clone();
    f2.fill_rect(Rect::new(180, 165, 6, 12), BLUE);
    assert_eq!(incremental(vec![f1, f2.clone()]), full_analysis(&f2));
    assert_eq!(
        full_analysis(&f2),
        vec![Rect::new(50, 50, 250, 250), Rect::new(180, 165, 6, 12)]
    );
}

/// Verification N2: a container whose edge moves gets its new bounds, even when the
/// change lines up with its edges (on the grid, or on the screen edge).
#[test]
fn a_shrinking_container_gets_its_new_bounds() {
    for (before, after) in [
        (Rect::new(64, 64, 384, 256), Rect::new(64, 64, 384, 128)), // panel
        (Rect::new(0, 0, 192, 128), Rect::new(0, 0, 192, 64)),      // banner in a corner
    ] {
        let mut f1 = Frame::solid(640, 480, WHITE).unwrap();
        f1.fill_rect(before, BLUE);
        let mut f2 = Frame::solid(640, 480, WHITE).unwrap();
        f2.fill_rect(after, BLUE);
        assert_eq!(incremental(vec![f1, f2.clone()]), full_analysis(&f2));
        assert_eq!(full_analysis(&f2), vec![after]);
    }
}

/// Verification N5: whether glyphs form one word must not depend on the order in which
/// they appeared. A glyph typed next to an existing one joins it, whether the existing
/// glyph touches the changed cell or ends a few pixels before it.
#[test]
fn glyphs_typed_one_by_one_group_like_a_full_analysis() {
    for (first, second) in [
        (Rect::new(90, 200, 6, 12), Rect::new(99, 200, 6, 12)),
        (Rect::new(88, 200, 6, 12), Rect::new(97, 200, 6, 12)),
    ] {
        let mut f1 = Frame::solid(320, 320, WHITE).unwrap();
        f1.fill_rect(first, INK);
        let mut f2 = f1.clone();
        f2.fill_rect(second, INK);
        assert_eq!(incremental(vec![f1, f2.clone()]), full_analysis(&f2));
        assert_eq!(full_analysis(&f2), vec![first.union(&second)]);
    }
}

/// A page of text: 6x10 glyphs 2 px apart, words 10 px apart, lines 18 px apart.
fn text_page(w: u32, h: u32) -> (Frame, usize) {
    let mut f = Frame::solid(w, h, WHITE).unwrap();
    let mut words = 0;
    let mut y = 4;
    while y + 10 < h {
        let mut x = 4;
        while x + 40 < w {
            for g in 0..4 {
                f.fill_rect(Rect::new(x + g * 8, y, 6, 10), INK);
            }
            words += 1;
            x += 4 * 8 + 10;
        }
        y += 18;
    }
    (f, words)
}

/// Review #2: grouping thousands of glyphs must stay near-linear (it took minutes).
#[test]
fn a_page_of_text_is_grouped_into_words() {
    let (f, words) = text_page(960, 540);
    let found = FlatRegionDetector::default()
        .detect(&f, f.bounds())
        .unwrap();
    assert_eq!(found.len(), words);
    assert!(found.iter().all(|d| d.bounds.w == 30 && d.bounds.h == 10));
}

/// Timing on full-HD and 4K screens: `cargo test -p ommatix-adapter-vision --release
/// -- --ignored --nocapture`.
#[test]
#[ignore = "benchmark"]
fn bench_full_screen_detection() {
    for (w, h) in [(1920, 1080), (3840, 2160)] {
        let (f, words) = text_page(w, h);
        let start = std::time::Instant::now();
        let found = FlatRegionDetector::default()
            .detect(&f, f.bounds())
            .unwrap();
        eprintln!(
            "{w}x{h}: {words} words, {} found in {:?}",
            found.len(),
            start.elapsed()
        );
    }
}
