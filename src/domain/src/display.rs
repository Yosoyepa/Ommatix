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

    /// OS input coordinates for a frame pixel, saturating at the `i32` range.
    pub fn to_os(&self, p: Point) -> (i32, i32) {
        let map = |v: u32, frame: u32, os: u32, origin: i32| -> i32 {
            let scaled = i64::from(v) * i64::from(os) / i64::from(frame);
            let value = i64::from(origin) + scaled;
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

    #[test]
    fn rejects_empty_sizes() {
        assert_eq!(
            DisplayMapping::new((0, 0), (0, 10), (10, 10)),
            Err(DomainError::EmptyDisplay)
        );
    }
}
