use crate::bbox::BBox;
use crate::{DomainError, Point};

/// Maps frame pixel coordinates to the coordinate space of the OS input system.
///
/// They differ on HiDPI screens (frames are physical pixels, macOS input is in points)
/// and on multi-monitor setups (the captured monitor has an origin). The mapping is
/// `os = origin + p * os_size / frame_size`, in integer arithmetic so it is exact and
/// deterministic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisplayMapping {
    origin_x: i32,
    origin_y: i32,
    frame_w: u32,
    frame_h: u32,
    os_w: u32,
    os_h: u32,
}

impl DisplayMapping {
    /// `frame_*`: size of the captured image; `os_*`: size of the same monitor in the input
    /// coordinate space; `origin_*`: position of the monitor in that space.
    pub fn new(
        origin: (i32, i32),
        frame_size: (u32, u32),
        os_size: (u32, u32),
    ) -> Result<Self, DomainError> {
        if frame_size.0 == 0 || frame_size.1 == 0 || os_size.0 == 0 || os_size.1 == 0 {
            return Err(DomainError::EmptyDisplay);
        }
        Ok(Self {
            origin_x: origin.0,
            origin_y: origin.1,
            frame_w: frame_size.0,
            frame_h: frame_size.1,
            os_w: os_size.0,
            os_h: os_size.1,
        })
    }

    /// Frame pixels are OS coordinates (single monitor at the origin, no scaling).
    pub fn identity(width: u32, height: u32) -> Result<Self, DomainError> {
        Self::new((0, 0), (width, height), (width, height))
    }

    /// The frame-pixel box of an OS rectangle `[left, right) × [top, bottom)` (the
    /// inverse of [`to_os`](Self::to_os)), clipped to the frame; `None` when nothing of it
    /// lies on this monitor.
    pub fn frame_box(&self, left: i32, top: i32, right: i32, bottom: i32) -> Option<BBox> {
        let map = |v: i32, origin: i32, frame: u32, os: u32| -> f64 {
            (f64::from(v) - f64::from(origin)) * f64::from(frame) / f64::from(os)
        };
        let raw = BBox::new(
            map(left, self.origin_x, self.frame_w, self.os_w),
            map(top, self.origin_y, self.frame_h, self.os_h),
            map(right, self.origin_x, self.frame_w, self.os_w),
            map(bottom, self.origin_y, self.frame_h, self.os_h),
        )
        .ok()?;
        let frame = BBox::new(0.0, 0.0, f64::from(self.frame_w), f64::from(self.frame_h)).ok()?;
        raw.clip(&frame)
    }

    /// OS input coordinates for a frame pixel, saturating at the `i32` range.
    pub fn to_os(&self, p: Point) -> (i32, i32) {
        let map = |v: u32, frame: u32, os: u32, origin: i32| -> i32 {
            // u32 · u32 fits in u64; the sum with any i32 origin fits in i128.
            let scaled = u64::from(v) * u64::from(os) / u64::from(frame);
            let value = i128::from(origin) + i128::from(scaled);
            i32::try_from(value).unwrap_or(if value < 0 { i32::MIN } else { i32::MAX })
        };
        (
            map(p.x, self.frame_w, self.os_w, self.origin_x),
            map(p.y, self.frame_h, self.os_h, self.origin_y),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_keeps_coordinates() {
        let m = DisplayMapping::identity(1920, 1080).unwrap();
        assert_eq!(m.to_os(Point::new(540, 316)), (540, 316));
    }

    #[test]
    fn retina_frame_maps_to_points() {
        // 2880x1800 physical frame of a 1440x900-point display.
        let m = DisplayMapping::new((0, 0), (2880, 1800), (1440, 900)).unwrap();
        assert_eq!(m.to_os(Point::new(1080, 632)), (540, 316));
    }

    #[test]
    fn secondary_monitor_left_of_primary_has_negative_origin() {
        let m = DisplayMapping::new((-1920, 0), (1920, 1080), (1920, 1080)).unwrap();
        assert_eq!(m.to_os(Point::new(10, 20)), (-1910, 20));
    }

    /// Verification #18: extreme values saturate instead of overflowing.
    #[test]
    fn extreme_coordinates_saturate() {
        let huge = DisplayMapping::new((i32::MAX, i32::MIN), (1, 1), (u32::MAX, u32::MAX)).unwrap();
        assert_eq!(huge.to_os(Point::new(u32::MAX, 0)), (i32::MAX, i32::MIN));
        let shrink = DisplayMapping::new((0, 0), (u32::MAX, u32::MAX), (1, 1)).unwrap();
        assert_eq!(shrink.to_os(Point::new(u32::MAX, u32::MAX)), (1, 1));
    }

    #[test]
    fn os_rectangles_map_back_to_clipped_frame_boxes() {
        let m = DisplayMapping::new((-1920, 0), (1920, 1080), (1920, 1080)).unwrap();
        assert_eq!(
            m.frame_box(-1910, 20, -1810, 60),
            Some(BBox::new(10.0, 20.0, 110.0, 60.0).unwrap())
        );
        // Partly on the primary monitor: clipped to this one.
        assert_eq!(
            m.frame_box(-20, 0, 40, 10),
            Some(BBox::new(1900.0, 0.0, 1920.0, 10.0).unwrap())
        );
        assert_eq!(m.frame_box(0, 0, 100, 100), None);
        assert_eq!(m.frame_box(-1900, 10, -1900, 50), None);
        let retina = DisplayMapping::new((0, 0), (2880, 1800), (1440, 900)).unwrap();
        assert_eq!(
            retina.frame_box(10, 10, 20, 30),
            Some(BBox::new(20.0, 20.0, 40.0, 60.0).unwrap())
        );
    }

    #[test]
    fn rejects_empty_sizes() {
        assert_eq!(
            DisplayMapping::new((0, 0), (0, 10), (10, 10)),
            Err(DomainError::EmptyDisplay)
        );
    }
}
