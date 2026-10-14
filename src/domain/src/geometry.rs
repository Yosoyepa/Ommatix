use serde::{Deserialize, Serialize};

/// A pixel coordinate in screen space (origin at the top-left corner).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Point {
    pub x: u32,
    pub y: u32,
}

impl Point {
    pub const fn new(x: u32, y: u32) -> Self {
        Self { x, y }
    }
}

/// An axis-aligned rectangle in screen space. `w`/`h` of zero means an empty rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

impl Rect {
    pub const fn new(x: u32, y: u32, w: u32, h: u32) -> Self {
        Self { x, y, w, h }
    }

    /// Exclusive right edge.
    pub const fn right(&self) -> u32 {
        self.x + self.w
    }

    /// Exclusive bottom edge.
    pub const fn bottom(&self) -> u32 {
        self.y + self.h
    }

    pub const fn is_empty(&self) -> bool {
        self.w == 0 || self.h == 0
    }

    pub const fn area(&self) -> u64 {
        self.w as u64 * self.h as u64
    }

    /// Geometric centre, rounded down. This is the click target handed to agents.
    pub const fn center(&self) -> Point {
        Point::new(self.x + self.w / 2, self.y + self.h / 2)
    }

    pub const fn contains(&self, p: Point) -> bool {
        p.x >= self.x && p.x < self.right() && p.y >= self.y && p.y < self.bottom()
    }

    pub fn intersection(&self, other: &Rect) -> Option<Rect> {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let r = self.right().min(other.right());
        let b = self.bottom().min(other.bottom());
        (r > x && b > y).then(|| Rect::new(x, y, r - x, b - y))
    }

    pub fn intersects(&self, other: &Rect) -> bool {
        self.intersection(other).is_some()
    }

    /// Smallest rectangle containing both.
    pub fn union(&self, other: &Rect) -> Rect {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        let r = self.right().max(other.right());
        let b = self.bottom().max(other.bottom());
        Rect::new(x, y, r - x, b - y)
    }

    /// Intersection over union in `[0, 1]`.
    pub fn iou(&self, other: &Rect) -> f32 {
        let inter = self.intersection(other).map_or(0, |r| r.area());
        let union = self.area() + other.area() - inter;
        if union == 0 {
            0.0
        } else {
            inter as f32 / union as f32
        }
    }

    /// Chebyshev gap between two rectangles (0 when they touch or overlap).
    pub fn gap(&self, other: &Rect) -> u32 {
        let dx = other
            .x
            .saturating_sub(self.right())
            .max(self.x.saturating_sub(other.right()));
        let dy = other
            .y
            .saturating_sub(self.bottom())
            .max(self.y.saturating_sub(other.bottom()));
        dx.max(dy)
    }

    /// Clips the rectangle to `bounds`, returning an empty rect at `bounds` origin if disjoint.
    pub fn clip(&self, bounds: &Rect) -> Rect {
        self.intersection(bounds)
            .unwrap_or(Rect::new(bounds.x, bounds.y, 0, 0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intersection_and_union() {
        let a = Rect::new(0, 0, 10, 10);
        let b = Rect::new(5, 5, 10, 10);
        assert_eq!(a.intersection(&b), Some(Rect::new(5, 5, 5, 5)));
        assert_eq!(a.union(&b), Rect::new(0, 0, 15, 15));
        assert!(
            a.intersection(&Rect::new(10, 0, 5, 5)).is_none(),
            "edges are exclusive"
        );
    }

    #[test]
    fn iou_is_symmetric_and_bounded() {
        let a = Rect::new(0, 0, 10, 10);
        let b = Rect::new(5, 0, 10, 10);
        assert!((a.iou(&b) - 50.0 / 150.0).abs() < 1e-6);
        assert!((a.iou(&b) - b.iou(&a)).abs() < f32::EPSILON);
        assert!((a.iou(&a) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn gap_between_rects() {
        let a = Rect::new(0, 0, 10, 10);
        assert_eq!(a.gap(&Rect::new(13, 0, 5, 5)), 3);
        assert_eq!(a.gap(&Rect::new(10, 10, 5, 5)), 0);
        assert_eq!(a.gap(&Rect::new(2, 2, 2, 2)), 0);
    }

    #[test]
    fn contains_uses_exclusive_edges() {
        let r = Rect::new(2, 2, 3, 3);
        assert!(r.contains(Point::new(2, 2)));
        assert!(r.contains(Point::new(4, 4)));
        assert!(!r.contains(Point::new(5, 4)));
        assert_eq!(r.center(), Point::new(3, 3));
    }
}
