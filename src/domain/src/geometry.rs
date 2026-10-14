/// A pixel coordinate in screen space (origin at the top-left corner).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Point {
    pub x: u32,
    pub y: u32,
}

impl Point {
    pub const fn new(x: u32, y: u32) -> Self {
        Self { x, y }
    }
}

/// An axis-aligned rectangle in screen space: the pixels `[x, right()) × [y, bottom())`.
///
/// Edges saturate at `u32::MAX`, so no input (however large) can overflow the arithmetic.
/// A rectangle that covers no pixel (zero size, or pushed past the coordinate range) is
/// empty, and every method agrees on that.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
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
        self.x.saturating_add(self.w)
    }

    /// Exclusive bottom edge.
    pub const fn bottom(&self) -> u32 {
        self.y.saturating_add(self.h)
    }

    /// `true` when the rectangle covers no pixel.
    pub const fn is_empty(&self) -> bool {
        self.right() == self.x || self.bottom() == self.y
    }

    /// Number of pixels covered.
    pub const fn area(&self) -> u64 {
        (self.right() - self.x) as u64 * (self.bottom() - self.y) as u64
    }

    /// Geometric centre of the covered pixels, rounded down. This is the click target
    /// handed to agents; it lies inside every non-empty rectangle.
    pub const fn center(&self) -> Point {
        Point::new(
            self.x + (self.right() - self.x) / 2,
            self.y + (self.bottom() - self.y) / 2,
        )
    }

    pub const fn contains(&self, p: Point) -> bool {
        p.x >= self.x && p.x < self.right() && p.y >= self.y && p.y < self.bottom()
    }

    /// `true` when `other` lies entirely inside `self` (an empty `other` never does).
    pub const fn contains_rect(&self, other: &Rect) -> bool {
        !other.is_empty()
            && other.x >= self.x
            && other.y >= self.y
            && other.right() <= self.right()
            && other.bottom() <= self.bottom()
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

    /// Smallest rectangle containing both. Empty rectangles are ignored.
    pub fn union(&self, other: &Rect) -> Rect {
        if other.is_empty() {
            return *self;
        }
        if self.is_empty() {
            return *other;
        }
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        let r = self.right().max(other.right());
        let b = self.bottom().max(other.bottom());
        Rect::new(x, y, r - x, b - y)
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

    /// The part of `self` inside `bounds`. When they are disjoint the result is an empty
    /// rectangle (check with [`Rect::is_empty`]); its position is not meaningful.
    pub fn clip(&self, bounds: &Rect) -> Rect {
        self.intersection(bounds)
            .unwrap_or(Rect::new(bounds.x, bounds.y, 0, 0))
    }

    /// Grows the rectangle by `margin` on every side, saturating at the coordinate range.
    pub fn inflate(&self, margin: u32) -> Rect {
        let x = self.x.saturating_sub(margin);
        let y = self.y.saturating_sub(margin);
        let r = self.right().saturating_add(margin);
        let b = self.bottom().saturating_add(margin);
        Rect::new(x, y, r - x, b - y)
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
    fn union_ignores_empty_rects() {
        let a = Rect::new(10, 10, 5, 5);
        assert_eq!(a.union(&Rect::new(500, 500, 0, 0)), a);
        assert_eq!(Rect::new(0, 0, 0, 3).union(&a), a);
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
        assert!(r.contains_rect(&r));
        assert!(!r.contains_rect(&Rect::new(3, 3, 3, 3)));
        assert!(!r.contains_rect(&Rect::new(3, 3, 0, 0)));
    }

    #[test]
    fn extreme_coordinates_saturate_instead_of_overflowing() {
        let r = Rect::new(u32::MAX, 0, 1, 1);
        assert_eq!(r.right(), u32::MAX);
        assert!(r.is_empty(), "covers no pixel once the edge saturates");
        assert_eq!(r.area(), 0);
        assert!(!r.contains(Point::new(u32::MAX, 0)));
        let huge = Rect::new(10, 10, u32::MAX, u32::MAX);
        let mid = 10 + (u32::MAX - 10) / 2;
        assert_eq!(huge.center(), Point::new(mid, mid));
        assert!(huge.contains(huge.center()));
        assert_eq!(
            huge.clip(&Rect::new(0, 0, 100, 100)),
            Rect::new(10, 10, 90, 90)
        );
        assert_eq!(Rect::new(1, 1, 2, 2).inflate(5), Rect::new(0, 0, 8, 8));
    }
}
