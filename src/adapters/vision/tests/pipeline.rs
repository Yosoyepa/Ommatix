//! The perception engine with the real detector, on screens built to break it. Each test
//! is a regression test for a finding of the 2026-10-14 review (docs/reviews).

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
