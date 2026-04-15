/// A point in logical (Wayland) coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

impl Point {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

impl std::ops::Add<(i32, i32)> for Point {
    type Output = Self;
    fn add(self, rhs: (i32, i32)) -> Self {
        Self { x: self.x + rhs.0, y: self.y + rhs.1 }
    }
}

/// A rectangle in logical (Wayland) coordinates.
///
/// Width and height are always non-negative after [`Rect::normalize`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }

    /// Ensure width and height are non-negative by flipping the origin.
    pub fn normalize(mut self) -> Self {
        if self.w < 0 {
            self.x += self.w;
            self.w = -self.w;
        }
        if self.h < 0 {
            self.y += self.h;
            self.h = -self.h;
        }
        self
    }

    pub fn left(&self) -> i32 { self.x }
    pub fn top(&self) -> i32 { self.y }
    pub fn right(&self) -> i32 { self.x + self.w }
    pub fn bottom(&self) -> i32 { self.y + self.h }

    pub fn contains(&self, p: Point) -> bool {
        p.x >= self.x && p.x < self.right() && p.y >= self.y && p.y < self.bottom()
    }

    /// Returns the intersection of two rectangles, or None if they don't overlap.
    pub fn intersect(&self, other: &Rect) -> Option<Rect> {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let right = self.right().min(other.right());
        let bottom = self.bottom().min(other.bottom());
        if right <= x || bottom <= y {
            return None;
        }
        Some(Rect { x, y, w: right - x, h: bottom - y })
    }

    pub fn translate(&self, dx: i32, dy: i32) -> Rect {
        Rect { x: self.x + dx, y: self.y + dy, w: self.w, h: self.h }
    }

    /// Clamp so the rect stays within `bounds`.
    pub fn clamp(&self, bounds: &Rect) -> Rect {
        let x = self.x.max(bounds.x);
        let y = self.y.max(bounds.y);
        let right = self.right().min(bounds.right());
        let bottom = self.bottom().min(bounds.bottom());
        Rect { x, y, w: (right - x).max(0), h: (bottom - y).max(0) }
    }

    pub fn is_empty(&self) -> bool {
        self.w <= 0 || self.h <= 0
    }
}
