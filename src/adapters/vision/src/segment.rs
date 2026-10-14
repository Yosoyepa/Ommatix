//! Region growing: splits an area of a frame into flat-colour segments.

use ommatix_domain::{Frame, Point, Rect};

use crate::border::FrameBorder;
use crate::forest::Forest;

pub(crate) type Rgb = [u8; 3];

/// Largest per-channel difference between two colours.
pub(crate) fn distance(a: Rgb, b: Rgb) -> u8 {
    a[0].abs_diff(b[0])
        .max(a[1].abs_diff(b[1]))
        .max(a[2].abs_diff(b[2]))
}

pub(crate) fn rgb(px: [u8; 4]) -> Rgb {
    [px[0], px[1], px[2]]
}

/// A connected group of pixels in which neighbours differ by at most the tolerance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Segment {
    /// Bounding box, in frame coordinates.
    pub bounds: Rect,
    pub pixels: u32,
    pub mean: Rgb,
    /// An edge pixel blends into one of its 8 neighbours just outside the area: the
    /// segment is part of something larger than the area (the background, or a container
    /// around it).
    pub continues_outside: bool,
    /// Some pixel lies on the frame's own border, where there is no outside to look at.
    pub touches_frame_border: bool,
    /// Some pixel lies on a surface stretch of the frame border ([`FrameBorder`]).
    pub on_border_surface: bool,
}

/// Grows 8-connected segments over `area` (already clipped to the frame, at most
/// `u32::MAX` pixels). Segments are returned in scan order of their first pixel.
pub(crate) fn segments(
    frame: &Frame,
    area: Rect,
    tolerance: u8,
    border: &FrameBorder,
) -> Vec<Segment> {
    let (w, h) = (area.w as usize, area.h as usize);
    let px: Vec<Rgb> = (area.y..area.bottom())
        .flat_map(|y| {
            frame
                .row_slice(area.x, y, area.w)
                .as_chunks::<4>()
                .0
                .iter()
                .map(|p| rgb(*p))
        })
        .collect();
    let similar = |a: Rgb, b: Rgb| distance(a, b) <= tolerance;

    let mut forest = Forest::new(w * h);
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            if x + 1 < w && similar(px[i], px[i + 1]) {
                forest.union(i, i + 1);
            }
            if y + 1 < h {
                let below = i + w;
                if similar(px[i], px[below]) {
                    forest.union(i, below);
                }
                if x + 1 < w && similar(px[i], px[below + 1]) {
                    forest.union(i, below + 1);
                }
                if x > 0 && similar(px[i], px[below - 1]) {
                    forest.union(i, below - 1);
                }
            }
        }
    }

    let mut slot = vec![u32::MAX; w * h];
    let mut stats: Vec<Stats> = Vec::new();
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            let root = forest.find(i);
            if slot[root] == u32::MAX {
                slot[root] = stats.len() as u32;
                stats.push(Stats::new(x, y));
            }
            let s = &mut stats[slot[root] as usize];
            s.add(x, y, px[i]);
            if x == 0 || y == 0 || x + 1 == w || y + 1 == h {
                let at = Point::new(area.x + x as u32, area.y + y as u32);
                if !s.continues_outside {
                    s.continues_outside = continues_outside(frame, area, at, px[i], tolerance);
                }
                if let Some(surface) = border.at(at.x, at.y) {
                    s.touches_frame_border = true;
                    s.on_border_surface |= surface;
                }
            }
        }
    }
    stats.into_iter().map(|s| s.finish(area)).collect()
}

/// Whether the area-edge pixel `at` blends into one of its 8 neighbours outside the area
/// (8-connected like segments, so a diagonal line crossing the edge continues too).
fn continues_outside(frame: &Frame, area: Rect, at: Point, colour: Rgb, tol: u8) -> bool {
    const NEIGHBOURS: [(i32, i32); 8] = [
        (-1, -1),
        (0, -1),
        (1, -1),
        (-1, 0),
        (1, 0),
        (-1, 1),
        (0, 1),
        (1, 1),
    ];
    NEIGHBOURS.iter().any(|&(dx, dy)| {
        let (Some(x), Some(y)) = (at.x.checked_add_signed(dx), at.y.checked_add_signed(dy)) else {
            return false;
        };
        let outside = Point::new(x, y);
        frame.bounds().contains(outside)
            && !area.contains(outside)
            && distance(colour, rgb(frame.pixel(x, y))) <= tol
    })
}

struct Stats {
    x0: usize,
    y0: usize,
    x1: usize,
    y1: usize,
    pixels: u32,
    sum: [u64; 3],
    continues_outside: bool,
    touches_frame_border: bool,
    on_border_surface: bool,
}

impl Stats {
    fn new(x: usize, y: usize) -> Self {
        Self {
            x0: x,
            y0: y,
            x1: x,
            y1: y,
            pixels: 0,
            sum: [0; 3],
            continues_outside: false,
            touches_frame_border: false,
            on_border_surface: false,
        }
    }

    fn add(&mut self, x: usize, y: usize, c: Rgb) {
        self.x0 = self.x0.min(x);
        self.y0 = self.y0.min(y);
        self.x1 = self.x1.max(x);
        self.y1 = self.y1.max(y);
        self.pixels += 1;
        for (sum, channel) in self.sum.iter_mut().zip(c) {
            *sum += u64::from(channel);
        }
    }

    fn finish(self, area: Rect) -> Segment {
        let n = u64::from(self.pixels.max(1));
        Segment {
            bounds: Rect::new(
                area.x + self.x0 as u32,
                area.y + self.y0 as u32,
                (self.x1 - self.x0 + 1) as u32,
                (self.y1 - self.y0 + 1) as u32,
            ),
            pixels: self.pixels,
            mean: self.sum.map(|s| (s / n) as u8),
            continues_outside: self.continues_outside,
            touches_frame_border: self.touches_frame_border,
            on_border_surface: self.on_border_surface,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WHITE: [u8; 4] = [255; 4];
    const INK: [u8; 4] = [0, 0, 0, 255];

    #[test]
    fn splits_flat_areas_and_joins_diagonals() {
        let mut f = Frame::solid(10, 10, WHITE).unwrap();
        f.fill_rect(Rect::new(2, 2, 1, 1), INK);
        f.fill_rect(Rect::new(3, 3, 1, 1), INK); // diagonal neighbour: same segment
        f.fill_rect(Rect::new(7, 7, 2, 2), [250, 250, 250, 255]); // within tolerance of white
        let segs = segments(&f, f.bounds(), 24, &FrameBorder::new(&f, 24));
        assert_eq!(segs.len(), 2);
        assert_eq!(segs[0].bounds, Rect::new(0, 0, 10, 10));
        assert!(segs[0].touches_frame_border && segs[0].on_border_surface);
        assert!(!segs[1].touches_frame_border);
        assert_eq!(segs[1].bounds, Rect::new(2, 2, 2, 2));
        assert_eq!(segs[1].pixels, 2);
        assert_eq!(segs[1].mean, [0, 0, 0]);
    }

    #[test]
    fn knows_what_continues_outside_the_area() {
        let mut f = Frame::solid(20, 20, WHITE).unwrap();
        f.fill_rect(Rect::new(0, 0, 20, 5), INK); // a bar crossing the area's top edge
        f.fill_rect(Rect::new(8, 10, 3, 3), INK); // an island inside the area
        let segs = segments(&f, Rect::new(5, 3, 10, 12), 24, &FrameBorder::new(&f, 24));
        let bar = segs.iter().find(|s| s.bounds.y == 3).unwrap();
        let island = segs
            .iter()
            .find(|s| s.bounds == Rect::new(8, 10, 3, 3))
            .unwrap();
        let white = segs.iter().find(|s| s.mean == [255, 255, 255]).unwrap();
        assert!(bar.continues_outside && white.continues_outside);
        assert!(!island.continues_outside);
        assert!(!bar.touches_frame_border && !island.touches_frame_border);
    }

    /// Review N7: a 1-px diagonal line crossing the area's edge continues outside it,
    /// although its straight-across neighbour does not match.
    #[test]
    fn a_diagonal_line_continues_outside_the_area() {
        let mut f = Frame::solid(20, 20, WHITE).unwrap();
        for i in 0..20 {
            f.fill_rect(Rect::new(i, i, 1, 1), INK);
        }
        let segs = segments(&f, Rect::new(8, 8, 6, 6), 24, &FrameBorder::new(&f, 24));
        let line = segs.iter().find(|s| s.mean == [0, 0, 0]).unwrap();
        assert!(line.continues_outside);
    }
}
