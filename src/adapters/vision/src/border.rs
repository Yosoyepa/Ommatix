//! The frame's own border, where there is no outside to look at.
//!
//! A flat stretch covering at least half of a screen edge is a surface that spans the
//! screen (the wallpaper, a maximised or snapped window, a taskbar, a dock), not an
//! element. Stretches are measured on the whole frame, so whether a segment is background
//! never depends on which part of the frame is being analysed.

use ommatix_domain::Frame;

use crate::segment::{Rgb, distance, rgb};

#[derive(Debug, Clone)]
pub(crate) struct FrameBorder {
    width: u32,
    height: u32,
    /// Per border pixel: lies on a surface stretch.
    top: Vec<bool>,
    bottom: Vec<bool>,
    left: Vec<bool>,
    right: Vec<bool>,
    /// Mean colour of every surface stretch.
    colours: Vec<Rgb>,
}

impl FrameBorder {
    pub(crate) fn new(frame: &Frame, tolerance: u8) -> Self {
        let (w, h) = (frame.width(), frame.height());
        let mut colours = Vec::new();
        let mut edge = |line: Vec<Rgb>| surface_stretches(&line, tolerance, &mut colours);
        let top = edge((0..w).map(|x| rgb(frame.pixel(x, 0))).collect());
        let bottom = edge((0..w).map(|x| rgb(frame.pixel(x, h - 1))).collect());
        let left = edge((0..h).map(|y| rgb(frame.pixel(0, y))).collect());
        let right = edge((0..h).map(|y| rgb(frame.pixel(w - 1, y))).collect());
        Self {
            width: w,
            height: h,
            top,
            bottom,
            left,
            right,
            colours,
        }
    }

    /// `None` when the frame pixel `(x, y)` is not on the frame border, otherwise whether
    /// it lies on a surface stretch.
    pub(crate) fn at(&self, x: u32, y: u32) -> Option<bool> {
        let (xi, yi) = (x as usize, y as usize);
        [
            (y == 0).then(|| self.top[xi]),
            (y + 1 == self.height).then(|| self.bottom[xi]),
            (x == 0).then(|| self.left[yi]),
            (x + 1 == self.width).then(|| self.right[yi]),
        ]
        .into_iter()
        .flatten()
        .reduce(|a, b| a || b)
    }

    /// Whether `colour` looks like one of the surface stretches.
    pub(crate) fn has_surface_colour(&self, colour: Rgb, tolerance: u8) -> bool {
        self.colours
            .iter()
            .any(|&c| distance(c, colour) <= tolerance)
    }
}

/// Marks the pixels of `line` lying on a flat run (neighbours within `tolerance`) that
/// covers at least half of the line, and records each such run's mean colour.
fn surface_stretches(line: &[Rgb], tolerance: u8, colours: &mut Vec<Rgb>) -> Vec<bool> {
    let mut mask = vec![false; line.len()];
    let mut start = 0;
    for end in 1..=line.len() {
        if end == line.len() || distance(line[end - 1], line[end]) > tolerance {
            if 2 * (end - start) >= line.len() {
                mask[start..end].fill(true);
                colours.push(mean(&line[start..end]));
            }
            start = end;
        }
    }
    mask
}

fn mean(pixels: &[Rgb]) -> Rgb {
    let mut sum = [0u64; 3];
    for p in pixels {
        for (s, c) in sum.iter_mut().zip(p) {
            *s += u64::from(*c);
        }
    }
    let n = pixels.len().max(1) as u64;
    sum.map(|s| (s / n) as u8)
}

#[cfg(test)]
mod tests {
    use ommatix_domain::Rect;

    use super::*;

    const WALLPAPER: [u8; 4] = [120, 140, 170, 255];
    const WHITE: [u8; 4] = [255; 4];
    const DARK: [u8; 4] = [40, 42, 48, 255];

    #[test]
    fn surfaces_cover_half_an_edge_and_buttons_do_not() {
        let mut f = Frame::solid(640, 400, WALLPAPER).unwrap();
        f.fill_rect(Rect::new(0, 360, 640, 40), DARK); // taskbar
        f.fill_rect(Rect::new(0, 360, 48, 40), WHITE); // start button in the corner
        let b = FrameBorder::new(&f, 24);
        assert_eq!(b.at(320, 0), Some(true), "wallpaper along the top");
        assert_eq!(b.at(0, 100), Some(true), "wallpaper along the left");
        assert_eq!(b.at(320, 399), Some(true), "taskbar along the bottom");
        assert_eq!(b.at(10, 399), Some(false), "start button");
        assert_eq!(b.at(0, 380), Some(false), "start button on the left edge");
        assert_eq!(b.at(320, 200), None, "not on the border");
        assert!(b.has_surface_colour([42, 42, 50], 24));
        assert!(!b.has_surface_colour([255, 255, 255], 24));
    }

    #[test]
    fn a_snapped_window_and_the_wallpaper_are_both_surfaces() {
        for width in [960, 1280, 1620] {
            let mut f = Frame::solid(1920, 1080, WALLPAPER).unwrap();
            f.fill_rect(Rect::new(0, 0, width, 1080), WHITE);
            let b = FrameBorder::new(&f, 24);
            assert_eq!(
                b.at(0, 540),
                Some(true),
                "window along the left edge, {width}"
            );
            assert_eq!(
                b.at(1919, 540),
                Some(true),
                "wallpaper along the right, {width}"
            );
        }
    }
}
