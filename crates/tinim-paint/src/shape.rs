//! The shapes that make up a frame.

use crate::{Color32, Mesh, Pos2, Rect};

/// A border around a filled shape. Drawn **inside** the shape's outline.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stroke {
    pub width: f32,
    pub color: Color32,
}

impl Stroke {
    pub const NONE: Self = Self {
        width: 0.0,
        color: Color32::TRANSPARENT,
    };

    #[inline]
    pub const fn new(width: f32, color: Color32) -> Self {
        Self { width, color }
    }

    #[inline]
    pub fn is_visible(&self) -> bool {
        self.width > 0.0 && !self.color.is_invisible()
    }
}

impl Default for Stroke {
    fn default() -> Self {
        Self::NONE
    }
}

/// A filled (and optionally stroked) rectangle with a uniform corner radius.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RectShape {
    pub rect: Rect,
    /// In logical points; rounded after scaling to physical pixels.
    pub corner_radius: u8,
    pub fill: Color32,
    pub stroke: Stroke,
}

impl RectShape {
    #[inline]
    pub fn new(rect: Rect, fill: Color32) -> Self {
        Self {
            rect,
            corner_radius: 0,
            fill,
            stroke: Stroke::NONE,
        }
    }

    #[inline]
    pub fn with_corner_radius(mut self, corner_radius: u8) -> Self {
        self.corner_radius = corner_radius;
        self
    }

    #[inline]
    pub fn with_stroke(mut self, stroke: Stroke) -> Self {
        self.stroke = stroke;
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CircleShape {
    pub center: Pos2,
    pub radius: f32,
    pub fill: Color32,
    pub stroke: Stroke,
}

impl CircleShape {
    #[inline]
    pub fn new(center: Pos2, radius: f32, fill: Color32) -> Self {
        Self {
            center,
            radius,
            fill,
            stroke: Stroke::NONE,
        }
    }

    #[inline]
    pub fn with_stroke(mut self, stroke: Stroke) -> Self {
        self.stroke = stroke;
        self
    }
}

/// A polyline. Each segment becomes one quad; no miter joins (fine for debug lines).
#[derive(Clone, Debug, PartialEq)]
pub struct LineShape {
    pub points: Vec<Pos2>,
    pub width: f32,
    pub color: Color32,
}

impl LineShape {
    #[inline]
    pub fn new(points: Vec<Pos2>, width: f32, color: Color32) -> Self {
        Self {
            points,
            width,
            color,
        }
    }
}

/// Anything that can be tessellated into triangles.
#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    Rect(RectShape),
    Circle(CircleShape),
    Line(LineShape),
    /// Custom geometry. Its coordinate space is given by [`Mesh::space`].
    Mesh(Mesh),
}

impl Shape {
    #[inline]
    pub fn rect(rect: Rect, fill: Color32) -> Self {
        Self::Rect(RectShape::new(rect, fill))
    }

    #[inline]
    pub fn circle(center: Pos2, radius: f32, fill: Color32) -> Self {
        Self::Circle(CircleShape::new(center, radius, fill))
    }

    #[inline]
    pub fn line(points: Vec<Pos2>, width: f32, color: Color32) -> Self {
        Self::Line(LineShape::new(points, width, color))
    }

    #[inline]
    pub fn mesh(mesh: Mesh) -> Self {
        Self::Mesh(mesh)
    }
}
