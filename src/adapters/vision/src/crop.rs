//! Integer crops of a [`BgraFrame`], shared by the model-backed adapters.
//!
//! A region is a `f64` box; a model reads whole pixels. [`PixelRect::cover`] rounds the
//! region outwards and clips it to the frame, and [`crop_bgra`] copies those pixels into a
//! tightly packed buffer (`row_stride == 4 * width`), whatever the frame's stride.

use ommatix_domain::bbox::BBox;
use ommatix_domain::incremental::BgraFrame;

/// Bytes per BGRA pixel.
pub const BPP: usize = 4;

/// A non-empty rectangle of whole pixels, `[x, x + width) × [y, y + height)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PixelRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl PixelRect {
    /// The pixels `region` covers in a `frame_width × frame_height` frame, rounded outwards
    /// and clipped to the frame; `None` when nothing is left.
    pub fn cover(region: &BBox, frame_width: u32, frame_height: u32) -> Option<Self> {
        let clamp = |v: f64, max: u32| v.clamp(0.0, f64::from(max)) as u32;
        let x0 = clamp(region.x0().floor(), frame_width);
        let y0 = clamp(region.y0().floor(), frame_height);
        let x1 = clamp(region.x1().ceil(), frame_width);
        let y1 = clamp(region.y1().ceil(), frame_height);
        (x1 > x0 && y1 > y0).then(|| Self {
            x: x0,
            y: y0,
            width: x1 - x0,
            height: y1 - y0,
        })
    }

    /// The rectangle as a box in frame coordinates.
    pub fn to_bbox(self) -> BBox {
        BBox::new(
            f64::from(self.x),
            f64::from(self.y),
            f64::from(self.x) + f64::from(self.width),
            f64::from(self.y) + f64::from(self.height),
        )
        .expect("a pixel rectangle has positive width and height")
    }

    /// The rectangle split into tiles whose sides are at most `max_side`, row-major (top
    /// to bottom, then left to right). `max_side == 0` is treated as 1.
    pub fn tiles(self, max_side: u32) -> Vec<Self> {
        let max_side = max_side.max(1);
        let mut out = Vec::new();
        let mut dy = 0;
        while dy < self.height {
            let height = max_side.min(self.height - dy);
            let mut dx = 0;
            while dx < self.width {
                let width = max_side.min(self.width - dx);
                out.push(Self {
                    x: self.x + dx,
                    y: self.y + dy,
                    width,
                    height,
                });
                dx += width;
            }
            dy += height;
        }
        out
    }
}

/// The pixels of `rect` (which must lie inside `frame`), tightly packed BGRA.
///
/// # Panics
///
/// If `rect` is not inside the frame; [`PixelRect::cover`] only builds rectangles that are.
pub fn crop_bgra(frame: &BgraFrame, rect: PixelRect) -> Vec<u8> {
    assert!(
        rect.x + rect.width <= frame.width() && rect.y + rect.height <= frame.height(),
        "the crop lies inside the frame"
    );
    let row_bytes = BPP * rect.width as usize;
    let mut out = Vec::with_capacity(row_bytes * rect.height as usize);
    let pixels = frame.pixels();
    for y in rect.y..rect.y + rect.height {
        let start = y as usize * frame.row_stride() + BPP * rect.x as usize;
        out.extend_from_slice(&pixels[start..start + row_bytes]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bbox(x0: f64, y0: f64, x1: f64, y1: f64) -> BBox {
        BBox::new(x0, y0, x1, y1).unwrap()
    }

    #[test]
    fn regions_are_rounded_outwards_and_clipped_to_the_frame() {
        assert_eq!(
            PixelRect::cover(&bbox(1.5, 2.2, 3.1, 4.0), 10, 10),
            Some(PixelRect {
                x: 1,
                y: 2,
                width: 3,
                height: 2
            })
        );
        assert_eq!(
            PixelRect::cover(&bbox(-5.0, -5.0, 50.0, 3.0), 10, 10),
            Some(PixelRect {
                x: 0,
                y: 0,
                width: 10,
                height: 3
            })
        );
        assert_eq!(PixelRect::cover(&bbox(12.0, 0.0, 20.0, 5.0), 10, 10), None);
    }

    #[test]
    fn crops_ignore_the_row_padding() {
        // 3x2 frame with a stride of 16 bytes (4 bytes of padding per row).
        let mut pixels = vec![0xEE; 32];
        for y in 0..2 {
            for x in 0..3 {
                let i = y * 16 + x * 4;
                pixels[i..i + 4].copy_from_slice(&[(10 * y + x) as u8; 4]);
            }
        }
        let frame = BgraFrame::new(3, 2, 16, 0, pixels).unwrap();
        let rect = PixelRect {
            x: 1,
            y: 0,
            width: 2,
            height: 2,
        };
        assert_eq!(
            crop_bgra(&frame, rect),
            [[1u8; 4], [2; 4], [11; 4], [12; 4]].concat()
        );
    }

    #[test]
    fn tiles_cover_the_rectangle_without_overlap() {
        let rect = PixelRect {
            x: 5,
            y: 7,
            width: 25,
            height: 10,
        };
        let tiles = rect.tiles(10);
        assert_eq!(tiles.len(), 3);
        assert_eq!(
            tiles
                .iter()
                .map(|t| u64::from(t.width) * u64::from(t.height))
                .sum::<u64>(),
            250
        );
        assert_eq!(
            tiles[2],
            PixelRect {
                x: 25,
                y: 7,
                width: 5,
                height: 10
            }
        );
        assert!(tiles.iter().all(|t| t.width <= 10 && t.height <= 10));
        assert_eq!(rect.tiles(100), vec![rect]);
        assert_eq!(rect.tiles(0).len(), 250);
    }

    #[test]
    fn a_rectangle_converts_to_its_box() {
        let rect = PixelRect {
            x: 2,
            y: 3,
            width: 4,
            height: 5,
        };
        assert_eq!(rect.to_bbox(), bbox(2.0, 3.0, 6.0, 8.0));
    }
}
