//! 2D math types for tinim.
//!
//! All lengths are `f32`. Y points down, like every other GUI toolkit.

pub use glam::Vec2;

/// A point in 2D space. Same type as [`Vec2`] on purpose: no conversion ceremony.
pub type Pos2 = Vec2;

#[inline]
pub const fn pos2(x: f32, y: f32) -> Pos2 {
    Vec2::new(x, y)
}

#[inline]
pub const fn vec2(x: f32, y: f32) -> Vec2 {
    Vec2::new(x, y)
}

/// An axis-aligned rectangle, stored as `min` and `max` corners.
///
/// `min <= max` is the normal case. [`Rect::NOTHING`] is the empty rectangle used as the
/// identity for [`Rect::union`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub min: Pos2,
    pub max: Pos2,
}

impl Rect {
    /// The empty rectangle, the identity element for [`Rect::union`].
    pub const NOTHING: Self = Self {
        min: Pos2::new(f32::INFINITY, f32::INFINITY),
        max: Pos2::new(f32::NEG_INFINITY, f32::NEG_INFINITY),
    };

    #[inline]
    pub fn from_min_max(min: Pos2, max: Pos2) -> Self {
        Self { min, max }
    }

    #[inline]
    pub fn from_min_size(min: Pos2, size: Vec2) -> Self {
        Self {
            min,
            max: min + size,
        }
    }

    #[inline]
    pub fn from_center_size(center: Pos2, size: Vec2) -> Self {
        Self {
            min: center - size * 0.5,
            max: center + size * 0.5,
        }
    }

    #[inline]
    pub fn width(&self) -> f32 {
        self.max.x - self.min.x
    }

    #[inline]
    pub fn height(&self) -> f32 {
        self.max.y - self.min.y
    }

    #[inline]
    pub fn size(&self) -> Vec2 {
        self.max - self.min
    }

    #[inline]
    pub fn center(&self) -> Pos2 {
        (self.min + self.max) * 0.5
    }

    /// Inclusive on all four edges, which is what you want for hit-testing.
    #[inline]
    pub fn contains(&self, p: Pos2) -> bool {
        self.min.x <= p.x && p.x <= self.max.x && self.min.y <= p.y && p.y <= self.max.y
    }

    #[inline]
    pub fn is_positive(&self) -> bool {
        self.min.x < self.max.x && self.min.y < self.max.y
    }

    /// May return a non-positive rect if the two rectangles do not overlap.
    #[inline]
    pub fn intersect(&self, other: Self) -> Self {
        Self {
            min: self.min.max(other.min),
            max: self.max.min(other.max),
        }
    }

    #[inline]
    pub fn union(&self, other: Self) -> Self {
        Self {
            min: self.min.min(other.min),
            max: self.max.max(other.max),
        }
    }

    #[inline]
    pub fn expand(&self, amount: f32) -> Self {
        Self {
            min: self.min - Vec2::splat(amount),
            max: self.max + Vec2::splat(amount),
        }
    }

    #[inline]
    pub fn shrink(&self, amount: f32) -> Self {
        self.expand(-amount)
    }

    #[inline]
    pub fn translate(&self, by: Vec2) -> Self {
        Self {
            min: self.min + by,
            max: self.max + by,
        }
    }

    #[inline]
    pub fn scale(&self, factor: f32) -> Self {
        Self {
            min: self.min * factor,
            max: self.max * factor,
        }
    }

    /// Snap to the physical pixel grid.
    ///
    /// `pixels_per_point` is the number of physical pixels per logical point. Rounding both
    /// corners the same way is what keeps adjacent rectangles from showing a seam.
    #[inline]
    pub fn round_to_pixels(&self, pixels_per_point: f32) -> Self {
        Self {
            min: (self.min * pixels_per_point).round() / pixels_per_point,
            max: (self.max * pixels_per_point).round() / pixels_per_point,
        }
    }
}

impl From<[f32; 4]> for Rect {
    fn from([x0, y0, x1, y1]: [f32; 4]) -> Self {
        Self::from_min_max(pos2(x0, y0), pos2(x1, y1))
    }
}

/// A range of `f32`, used for one axis of a layout.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rangef {
    pub min: f32,
    pub max: f32,
}

impl Rangef {
    pub const ZERO: Self = Self { min: 0.0, max: 0.0 };

    #[inline]
    pub fn new(min: f32, max: f32) -> Self {
        Self { min, max }
    }

    #[inline]
    pub fn span(&self) -> f32 {
        self.max - self.min
    }

    #[inline]
    pub fn contains(&self, x: f32) -> bool {
        self.min <= x && x <= self.max
    }

    #[inline]
    pub fn clamp(&self, x: f32) -> f32 {
        x.clamp(self.min, self.max)
    }

    #[inline]
    pub fn is_positive(&self) -> bool {
        self.min < self.max
    }
}

impl Default for Rangef {
    fn default() -> Self {
        Self::ZERO
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_to_pixels_is_idempotent() {
        let r = Rect::from_min_max(pos2(1.2, 2.7), pos2(10.6, 20.4));
        let rounded = r.round_to_pixels(2.0);
        assert_eq!(rounded, rounded.round_to_pixels(2.0));
        assert_eq!(rounded.min, pos2(1.0, 2.5));
        assert_eq!(rounded.max, pos2(10.5, 20.5));
    }

    #[test]
    fn intersect_and_union() {
        let a = Rect::from_min_size(pos2(0.0, 0.0), vec2(10.0, 10.0));
        let b = Rect::from_min_size(pos2(5.0, 5.0), vec2(10.0, 10.0));
        assert_eq!(
            a.intersect(b),
            Rect::from_min_max(pos2(5.0, 5.0), pos2(10.0, 10.0))
        );
        assert_eq!(
            a.union(b),
            Rect::from_min_max(pos2(0.0, 0.0), pos2(15.0, 15.0))
        );
        assert!(!a.intersect(Rect::NOTHING).is_positive());
        assert_eq!(a.union(Rect::NOTHING), a);
    }

    #[test]
    fn contains_is_inclusive() {
        let r = Rect::from_min_size(pos2(0.0, 0.0), vec2(10.0, 10.0));
        assert!(r.contains(pos2(0.0, 0.0)));
        assert!(r.contains(pos2(10.0, 10.0)));
        assert!(!r.contains(pos2(10.1, 5.0)));
    }
}
