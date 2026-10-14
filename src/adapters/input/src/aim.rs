//! Where an action aims, shared by the live actuators.

use ommatix_domain::Point;

/// The frame pixel an action aims at; `None` when the point is not a valid pixel.
pub(crate) fn frame_pixel((x, y): (f64, f64)) -> Option<Point> {
    let px =
        |v: f64| (v.is_finite() && v >= 0.0 && v < f64::from(u32::MAX)).then(|| v.floor() as u32);
    Some(Point::new(px(x)?, px(y)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn points_must_be_frame_pixels() {
        assert_eq!(frame_pixel((10.5, 3.9)), Some(Point::new(10, 3)));
        assert_eq!(frame_pixel((-1.0, 3.0)), None);
        assert_eq!(frame_pixel((f64::NAN, 3.0)), None);
        assert_eq!(frame_pixel((1e12, 3.0)), None);
    }
}
