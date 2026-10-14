//! Axis-aligned boxes in physical pixels, as the thesis algorithms use them (chapter 04).
//!
//! Coordinates are `f64` because detector and OCR boxes are not pixel-aligned. A [`BBox`] is
//! always finite and has positive width and height: invalid evidence is rejected when the box
//! is built, never later.

use thiserror::Error;

/// Why a box was rejected.
#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum BBoxError {
    #[error("box coordinates must be finite")]
    NonFinite,
    #[error("box must have positive width and height")]
    Empty,
}

/// A finite box `[x0, x1) × [y0, y1)` with `x1 > x0` and `y1 > y0`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BBox {
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
}

impl BBox {
    pub fn new(x0: f64, y0: f64, x1: f64, y1: f64) -> Result<Self, BBoxError> {
        if ![x0, y0, x1, y1].iter().all(|v| v.is_finite()) {
            return Err(BBoxError::NonFinite);
        }
        // Written so that a NaN difference would also be rejected.
        if !(x1 > x0 && y1 > y0) {
            return Err(BBoxError::Empty);
        }
        Ok(Self { x0, y0, x1, y1 })
    }

    pub const fn x0(&self) -> f64 {
        self.x0
    }

    pub const fn y0(&self) -> f64 {
        self.y0
    }

    pub const fn x1(&self) -> f64 {
        self.x1
    }

    pub const fn y1(&self) -> f64 {
        self.y1
    }

    pub fn width(&self) -> f64 {
        self.x1 - self.x0
    }

    pub fn height(&self) -> f64 {
        self.y1 - self.y0
    }

    pub fn area(&self) -> f64 {
        self.width() * self.height()
    }

    pub fn center(&self) -> (f64, f64) {
        (
            f64::midpoint(self.x0, self.x1),
            f64::midpoint(self.y0, self.y1),
        )
    }

    /// Area shared with `other`; boxes that only touch share none.
    pub fn intersection_area(&self, other: &Self) -> f64 {
        let w = (self.x1.min(other.x1) - self.x0.max(other.x0)).max(0.0);
        let h = (self.y1.min(other.y1) - self.y0.max(other.y0)).max(0.0);
        w * h
    }

    /// Whether the boxes share a positive area.
    pub fn overlaps(&self, other: &Self) -> bool {
        self.intersection_area(other) > 0.0
    }

    /// Intersection over the child's area: how much of `child` lies inside `self`.
    pub fn ioc(&self, child: &Self) -> f64 {
        self.intersection_area(child) / child.area()
    }

    /// Intersection over union.
    pub fn iou(&self, other: &Self) -> f64 {
        let inter = self.intersection_area(other);
        inter / (self.area() + other.area() - inter)
    }

    /// Smallest box containing both.
    pub fn union(&self, other: &Self) -> Self {
        Self {
            x0: self.x0.min(other.x0),
            y0: self.y0.min(other.y0),
            x1: self.x1.max(other.x1),
            y1: self.y1.max(other.y1),
        }
    }

    pub fn contains(&self, other: &Self) -> bool {
        self.x0 <= other.x0 && self.y0 <= other.y0 && self.x1 >= other.x1 && self.y1 >= other.y1
    }

    /// The part of `self` inside `bounds`, if any area is left.
    pub fn clip(&self, bounds: &Self) -> Option<Self> {
        Self::new(
            self.x0.max(bounds.x0),
            self.y0.max(bounds.y0),
            self.x1.min(bounds.x1),
            self.y1.min(bounds.y1),
        )
        .ok()
    }

    /// The box grown by `margin` on every side (`margin` must be finite and non-negative).
    pub fn pad(&self, margin: f64) -> Self {
        let margin = if margin.is_finite() {
            margin.max(0.0)
        } else {
            0.0
        };
        Self {
            x0: self.x0 - margin,
            y0: self.y0 - margin,
            x1: self.x1 + margin,
            y1: self.y1 + margin,
        }
    }

    /// Total order on the corners `(x0, y0, x1, y1)`, used to emit boxes deterministically.
    pub fn cmp_corners(&self, other: &Self) -> std::cmp::Ordering {
        self.x0
            .total_cmp(&other.x0)
            .then(self.y0.total_cmp(&other.y0))
            .then(self.x1.total_cmp(&other.x1))
            .then(self.y1.total_cmp(&other.y1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b(x0: f64, y0: f64, x1: f64, y1: f64) -> BBox {
        BBox::new(x0, y0, x1, y1).unwrap()
    }

    /// UT-16 (OR-01): small text fully inside a large button has IoC 1, although IoU is tiny.
    #[test]
    fn ut_16_ioc_of_small_text_inside_button_is_one() {
        let button = b(100.0, 100.0, 400.0, 140.0);
        let text = b(235.0, 112.0, 260.0, 126.0);
        assert!((button.ioc(&text) - 1.0).abs() < f64::EPSILON);
        assert!(text.area() / button.area() < 0.03);
    }

    /// UT-17 (OR-02, OR-03): NaN, infinity and non-positive areas are rejected.
    #[test]
    fn ut_17_invalid_geometry_is_rejected() {
        assert_eq!(
            BBox::new(f64::NAN, 0.0, 1.0, 1.0),
            Err(BBoxError::NonFinite)
        );
        assert_eq!(
            BBox::new(0.0, 0.0, f64::INFINITY, 1.0),
            Err(BBoxError::NonFinite)
        );
        assert_eq!(BBox::new(0.0, 0.0, 0.0, 2.0), Err(BBoxError::Empty));
        assert_eq!(BBox::new(0.0, 3.0, 2.0, 1.0), Err(BBoxError::Empty));
    }

    #[test]
    fn touching_boxes_do_not_overlap() {
        assert!(!b(0.0, 0.0, 10.0, 10.0).overlaps(&b(10.0, 0.0, 20.0, 10.0)));
        assert!(b(0.0, 0.0, 10.0, 10.0).overlaps(&b(9.0, 0.0, 20.0, 10.0)));
    }

    #[test]
    fn clip_keeps_only_the_part_inside_bounds() {
        let bounds = b(0.0, 0.0, 100.0, 50.0);
        assert_eq!(
            b(-32.0, 40.0, 20.0, 90.0).clip(&bounds),
            Some(b(0.0, 40.0, 20.0, 50.0))
        );
        assert_eq!(b(200.0, 0.0, 210.0, 10.0).clip(&bounds), None);
    }
}
