//! Letterboxing a crop into the model's input, and undoing it (thesis chapter 02 §2.8).
//!
//! The crop is scaled by one factor `scale` (the largest that fits, but never above 1),
//! centred, and the rest of the tensor is padded. A crop smaller than the input is padded
//! at its own resolution, never magnified: GUI detectors are trained on whole screens
//! scaled down, and in the S0 spike a magnified small region produced spurious detections
//! (docs/thesis/S0_RESULTADOS.md). The resampling is defined as the inverse map, so undoing it is
//! exact: a tensor coordinate `x_t` is the crop coordinate
//! `x_crop = (x_t - pad_x) / scale` (and likewise for Y), and the frame coordinate is
//! `x_crop` plus the crop's origin. Coordinates are continuous, with pixel `i` covering
//! `[i, i + 1)`.

use crate::crop::BPP;

/// Value of the padding, as YOLO-style exporters train with it (114 / 255 on each channel).
pub const PAD_VALUE: f32 = 114.0 / 255.0;

/// How a `crop_width × crop_height` crop sits in an `input_width × input_height` tensor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Letterbox {
    pub scale: f64,
    /// Left padding, in whole tensor pixels.
    pub pad_x: f64,
    /// Top padding, in whole tensor pixels.
    pub pad_y: f64,
    pub crop_width: u32,
    pub crop_height: u32,
    pub input_width: u32,
    pub input_height: u32,
}

impl Letterbox {
    /// The letterbox of a crop (scale at most 1); `None` if any side is zero.
    pub fn fit(
        crop_width: u32,
        crop_height: u32,
        input_width: u32,
        input_height: u32,
    ) -> Option<Self> {
        if crop_width == 0 || crop_height == 0 || input_width == 0 || input_height == 0 {
            return None;
        }
        let scale = (f64::from(input_width) / f64::from(crop_width))
            .min(f64::from(input_height) / f64::from(crop_height))
            .min(1.0);
        let content = |side: u32, input: u32| {
            (f64::from(side) * scale)
                .round()
                .clamp(1.0, f64::from(input))
        };
        let pad = |side: u32, input: u32| ((f64::from(input) - content(side, input)) / 2.0).floor();
        Some(Self {
            scale,
            pad_x: pad(crop_width, input_width),
            pad_y: pad(crop_height, input_height),
            crop_width,
            crop_height,
            input_width,
            input_height,
        })
    }

    /// Tensor coordinates to crop coordinates (SDD §2.8).
    pub fn to_crop(&self, x_t: f64, y_t: f64) -> (f64, f64) {
        (
            (x_t - self.pad_x) / self.scale,
            (y_t - self.pad_y) / self.scale,
        )
    }

    /// Crop coordinates to tensor coordinates (the forward map).
    pub fn to_tensor(&self, x_crop: f64, y_crop: f64) -> (f64, f64) {
        (
            x_crop * self.scale + self.pad_x,
            y_crop * self.scale + self.pad_y,
        )
    }

    /// The model input for a tightly packed BGRA crop: NCHW, RGB, values in `[0, 1]`,
    /// bilinear resampling through the inverse map, padding outside the crop's image.
    ///
    /// # Panics
    ///
    /// If `bgra` does not hold `crop_width × crop_height` pixels.
    pub fn tensor(&self, bgra: &[u8]) -> Vec<f32> {
        let (cw, ch) = (self.crop_width as usize, self.crop_height as usize);
        assert_eq!(bgra.len(), BPP * cw * ch, "the crop matches the letterbox");
        let (iw, ih) = (self.input_width as usize, self.input_height as usize);
        let plane = iw * ih;
        let mut out = vec![PAD_VALUE; 3 * plane];
        let cols = self.samples(iw, self.pad_x, cw);
        let rows = self.samples(ih, self.pad_y, ch);
        for (v, row) in rows.iter().enumerate() {
            let Some(row) = row else { continue };
            for (u, col) in cols.iter().enumerate() {
                let Some(col) = col else { continue };
                let px = |y: usize, x: usize, c: usize| f32::from(bgra[BPP * (y * cw + x) + c]);
                let at = v * iw + u;
                // BGRA → planes R, G, B.
                for (plane_index, channel) in [(0, 2), (1, 1), (2, 0)] {
                    let top = px(row.0, col.0, channel) * (1.0 - col.2)
                        + px(row.0, col.1, channel) * col.2;
                    let bottom = px(row.1, col.0, channel) * (1.0 - col.2)
                        + px(row.1, col.1, channel) * col.2;
                    out[plane_index * plane + at] = (top * (1.0 - row.2) + bottom * row.2) / 255.0;
                }
            }
        }
        out
    }

    /// For each tensor index along one axis: `None` in the padding, else the two source
    /// indices and the weight of the second, sampling at the pixel centre through the
    /// inverse map.
    fn samples(&self, input: usize, pad: f64, side: usize) -> Vec<Option<(usize, usize, f32)>> {
        let content = (side as f64 * self.scale).round().clamp(1.0, input as f64) as usize;
        let start = pad as usize;
        (0..input)
            .map(|t| {
                if t < start || t >= start + content {
                    return None;
                }
                let source =
                    ((t as f64 + 0.5 - pad) / self.scale - 0.5).clamp(0.0, (side - 1) as f64);
                let lo = source.floor() as usize;
                let hi = (lo + 1).min(side - 1);
                Some((lo, hi, (source - lo as f64) as f32))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn a_wide_crop_is_padded_vertically_and_centred() {
        let lb = Letterbox::fit(200, 100, 64, 64).unwrap();
        assert!(close(lb.scale, 0.32));
        assert!(close(lb.pad_x, 0.0));
        assert!(close(lb.pad_y, 16.0));
        // The crop's corners map to the content's corners and back exactly.
        assert_eq!(lb.to_tensor(0.0, 0.0), (0.0, 16.0));
        let (x, y) = lb.to_tensor(200.0, 100.0);
        assert!(close(x, 64.0) && close(y, 48.0));
        let (x, y) = lb.to_crop(64.0, 48.0);
        assert!(close(x, 200.0) && close(y, 100.0));
    }

    #[test]
    fn the_inverse_undoes_the_forward_map() {
        for (cw, ch, iw, ih) in [(37, 91, 64, 64), (640, 480, 320, 320), (5, 5, 640, 384)] {
            let lb = Letterbox::fit(cw, ch, iw, ih).unwrap();
            for (x, y) in [(0.0, 0.0), (1.25, 3.5), (f64::from(cw), f64::from(ch))] {
                let (xt, yt) = lb.to_tensor(x, y);
                let (xc, yc) = lb.to_crop(xt, yt);
                assert!(
                    close(xc, x) && close(yc, y),
                    "{cw}x{ch} → {iw}x{ih} at ({x}, {y})"
                );
            }
        }
    }

    #[test]
    fn empty_sides_have_no_letterbox() {
        assert!(Letterbox::fit(0, 10, 64, 64).is_none());
        assert!(Letterbox::fit(10, 10, 0, 64).is_none());
    }

    #[test]
    fn the_tensor_is_rgb_planes_with_padding() {
        // A 2x1 crop: blue then red, into a 4x4 input → scale 1, content 2x1 at (1, 1).
        let crop = [255, 0, 0, 255, 0, 0, 255, 255];
        let lb = Letterbox::fit(2, 1, 4, 4).unwrap();
        assert!(close(lb.scale, 1.0) && close(lb.pad_x, 1.0) && close(lb.pad_y, 1.0));
        let t = lb.tensor(&crop);
        assert_eq!(t.len(), 3 * 16);
        let (r, b) = (&t[0..16], &t[32..48]);
        // Everything but pixels 5 and 6 is padding.
        for (i, v) in r.iter().enumerate() {
            if i != 5 && i != 6 {
                assert!((*v - PAD_VALUE).abs() < 1e-6, "pixel {i}");
            }
        }
        // Row 1: blue then red, BGRA → RGB planes.
        assert!((b[5] - 1.0).abs() < 1e-6 && r[5].abs() < 1e-6);
        assert!((r[6] - 1.0).abs() < 1e-6 && b[6].abs() < 1e-6);
    }

    #[test]
    fn a_downscaled_crop_is_resampled_bilinearly() {
        // A 4x2 crop: two blue columns then two red ones, into 2x2 → scale 0.5.
        let blue = [255, 0, 0, 255];
        let red = [0, 0, 255, 255];
        let crop: Vec<u8> = [blue, blue, red, red, blue, blue, red, red].concat();
        let lb = Letterbox::fit(4, 2, 2, 2).unwrap();
        assert!(close(lb.scale, 0.5) && close(lb.pad_x, 0.0) && close(lb.pad_y, 0.0));
        let t = lb.tensor(&crop);
        let (r, b) = (&t[0..4], &t[8..12]);
        // Content row 0: each tensor pixel averages two source pixels of one colour.
        assert!((b[0] - 1.0).abs() < 1e-6 && r[0].abs() < 1e-6);
        assert!((r[1] - 1.0).abs() < 1e-6 && b[1].abs() < 1e-6);
        // Row 1 is padding.
        assert!(r[2..4].iter().all(|v| (*v - PAD_VALUE).abs() < 1e-6));
    }

    /// S0 regression: a small dirty region letterboxed into a 1280 input was magnified
    /// more than ten times, and the detector reported spurious regions inside it.
    #[test]
    fn a_small_crop_is_not_magnified() {
        let lb = Letterbox::fit(128, 96, 1280, 1280).unwrap();
        assert!(close(lb.scale, 1.0));
        assert!(close(lb.pad_x, 576.0) && close(lb.pad_y, 592.0));
        assert_eq!(lb.to_tensor(0.0, 0.0), (576.0, 592.0));
        assert_eq!(lb.to_crop(704.0, 688.0), (128.0, 96.0));
        // A whole 1920x1080 frame is still scaled down to fit.
        let full = Letterbox::fit(1920, 1080, 1280, 1280).unwrap();
        assert!(close(full.scale, 2.0 / 3.0) && close(full.pad_y, 280.0));
    }

    #[test]
    fn identity_letterbox_copies_pixels() {
        let crop: Vec<u8> = (0..4u8)
            .flat_map(|i| [i * 10, i * 20, i * 30, 255])
            .collect();
        let lb = Letterbox::fit(2, 2, 2, 2).unwrap();
        assert!(close(lb.scale, 1.0) && close(lb.pad_x, 0.0) && close(lb.pad_y, 0.0));
        let t = lb.tensor(&crop);
        for i in 0..4 {
            let f = |v: u8| f32::from(v) / 255.0;
            let p = &crop[4 * i..4 * i + 4];
            let expected = [f(p[2]), f(p[1]), f(p[0])];
            for (actual, expected) in [t[i], t[4 + i], t[8 + i]].into_iter().zip(expected) {
                assert!((actual - expected).abs() < 1e-6, "pixel {i}");
            }
        }
    }
}
