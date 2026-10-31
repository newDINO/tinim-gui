//! Turning [`Shape`]s into triangles.
//!
//! Everything here is CPU-only and backend-agnostic: the output is plain vertices, indices and a
//! clip rectangle per mesh. There is no anti-aliasing for shapes; text gets its smoothness from
//! glyph coverage in the atlas.

use std::f32::consts::PI;

use crate::{
    CircleShape, ClippedPrimitive, ClippedShape, Color32, LineShape, Mesh, Pos2, Rect, RectShape,
    Shape, TextureId, Vec2, Vertex,
};

/// Segments per rounded-rectangle corner.
const CORNER_SEGMENTS: usize = 8;
const MIN_CIRCLE_SEGMENTS: usize = 16;
const MAX_CIRCLE_SEGMENTS: usize = 128;

/// Reusable tessellator. Keeps its buffers around so that steady-state frames do not allocate.
#[derive(Default)]
pub struct Tessellator {
    vertices: Vec<Vertex>,
    indices: Vec<u32>,
}

impl Tessellator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Tessellate one frame.
    ///
    /// `pixels_per_point` converts logical points to physical pixels.
    pub fn tessellate(
        &mut self,
        shapes: &[ClippedShape],
        pixels_per_point: f32,
    ) -> Vec<ClippedPrimitive> {
        let ppp = if pixels_per_point.is_finite() && pixels_per_point > 0.0 {
            pixels_per_point
        } else {
            1.0
        };

        let mut out = Vec::with_capacity(shapes.len());

        for clipped in shapes {
            self.vertices.clear();
            self.indices.clear();

            let texture_id = match &clipped.shape {
                Shape::Rect(shape) => {
                    self.add_rect(shape, ppp);
                    TextureId::FONT_ATLAS
                }
                Shape::Circle(shape) => {
                    self.add_circle(shape, ppp);
                    TextureId::FONT_ATLAS
                }
                Shape::Line(shape) => {
                    self.add_line(shape, ppp);
                    TextureId::FONT_ATLAS
                }
                // Custom geometry is already in physical pixels; pass it through untouched.
                Shape::Mesh(mesh) => {
                    if !mesh.is_empty() {
                        out.push(ClippedPrimitive {
                            clip_rect: clipped.clip_rect,
                            mesh: mesh.clone(),
                        });
                    }
                    continue;
                }
            };

            if self.indices.is_empty() {
                continue;
            }

            out.push(ClippedPrimitive {
                clip_rect: clipped.clip_rect,
                mesh: Mesh {
                    indices: self.indices.clone(),
                    vertices: self.vertices.clone(),
                    texture_id,
                },
            });
        }

        out
    }

    fn add_rect(&mut self, shape: &RectShape, ppp: f32) {
        if !shape.rect.is_positive() {
            return;
        }

        let rect =
            Rect::from_min_max(shape.rect.min * ppp, shape.rect.max * ppp).round_to_pixels(1.0);
        let radius = (shape.corner_radius as f32 * ppp).round();
        let rounded = radius >= 0.5;

        let outer = if rounded {
            rounded_rect_path(rect, radius, CORNER_SEGMENTS)
        } else {
            rect_corners(rect).to_vec()
        };

        if !shape.fill.is_invisible() {
            if rounded {
                self.add_convex_fill(&outer, rect.center(), shape.fill);
            } else {
                let c = rect_corners(rect);
                self.add_quad(c[0], c[1], c[2], c[3], shape.fill);
            }
        }

        if shape.stroke.is_visible() {
            let width = (shape.stroke.width * ppp).max(0.0);
            let inner = rect.shrink(width);
            if inner.is_positive() {
                let inner_path = if rounded {
                    rounded_rect_path(inner, (radius - width).max(0.0), CORNER_SEGMENTS)
                } else {
                    rect_corners(inner).to_vec()
                };
                self.add_ring(&outer, &inner_path, shape.stroke.color);
            }
        }
    }

    fn add_circle(&mut self, shape: &CircleShape, ppp: f32) {
        let center = shape.center * ppp;
        let radius = shape.radius * ppp;
        if radius <= 0.0 {
            return;
        }

        let segments = (radius as usize).clamp(MIN_CIRCLE_SEGMENTS, MAX_CIRCLE_SEGMENTS);
        let outer = circle_path(center, radius, segments);

        if !shape.fill.is_invisible() {
            self.add_convex_fill(&outer, center, shape.fill);
        }

        if shape.stroke.is_visible() {
            let width = (shape.stroke.width * ppp).max(0.0);
            let inner_radius = radius - width;
            if inner_radius > 0.0 {
                let inner = circle_path(center, inner_radius, segments);
                self.add_ring(&outer, &inner, shape.stroke.color);
            }
        }
    }

    fn add_line(&mut self, shape: &LineShape, ppp: f32) {
        let width = (shape.width * ppp).max(0.0);
        if width <= 0.0 || shape.color.is_invisible() || shape.points.len() < 2 {
            return;
        }

        for pair in shape.points.windows(2) {
            let a = pair[0] * ppp;
            let b = pair[1] * ppp;
            let delta = b - a;
            let length = delta.length();
            if length <= 0.0 {
                continue;
            }
            let normal = Vec2::new(-delta.y, delta.x) / length * (width * 0.5);
            self.add_quad(a + normal, b + normal, b - normal, a - normal, shape.color);
        }
    }

    /// Triangle fan from `center` to a convex polygon given in order.
    fn add_convex_fill(&mut self, path: &[Pos2], center: Pos2, color: Color32) {
        if path.len() < 3 {
            return;
        }
        let center_index = self.push_vertex(center, color);
        let base = self.vertices.len() as u32;
        for &p in path {
            self.push_vertex(p, color);
        }
        for i in 0..path.len() as u32 {
            let a = base + i;
            let b = base + (i + 1) % path.len() as u32;
            self.indices.extend_from_slice(&[center_index, a, b]);
        }
    }

    /// Quad strip between two closed paths with the same number of points.
    fn add_ring(&mut self, outer: &[Pos2], inner: &[Pos2], color: Color32) {
        debug_assert_eq!(outer.len(), inner.len());
        let n = outer.len().min(inner.len());
        if n < 2 {
            return;
        }
        for i in 0..n {
            let j = (i + 1) % n;
            self.add_quad(outer[i], outer[j], inner[j], inner[i], color);
        }
    }

    fn add_quad(&mut self, a: Pos2, b: Pos2, c: Pos2, d: Pos2, color: Color32) {
        let base = self.vertices.len() as u32;
        for p in [a, b, c, d] {
            self.push_vertex(p, color);
        }
        self.indices
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    fn push_vertex(&mut self, p: Pos2, color: Color32) -> u32 {
        let index = self.vertices.len() as u32;
        self.vertices.push(Vertex {
            pos: [p.x, p.y],
            uv: Vertex::WHITE_UV,
            color: color.0,
        });
        index
    }
}

/// Outline of a rounded rectangle, clockwise, in physical pixels.
///
/// Always returns `4 * (segments + 1)` points so that an outer path and an inner path can be
/// zipped into a ring. When `radius` is zero, the points of each corner collapse onto the corner
/// itself, which produces degenerate (zero-area) triangles but a correct outline.
fn rounded_rect_path(rect: Rect, radius: f32, segments: usize) -> Vec<Pos2> {
    let r = radius.min(rect.width().min(rect.height()) * 0.5).max(0.0);

    let corners = [
        (Pos2::new(rect.min.x + r, rect.min.y + r), PI, 1.5 * PI),
        (
            Pos2::new(rect.max.x - r, rect.min.y + r),
            1.5 * PI,
            2.0 * PI,
        ),
        (Pos2::new(rect.max.x - r, rect.max.y - r), 0.0, 0.5 * PI),
        (Pos2::new(rect.min.x + r, rect.max.y - r), 0.5 * PI, PI),
    ];

    let mut points = Vec::with_capacity(4 * (segments + 1));
    let segments = segments.max(1);
    for (center, start, end) in corners {
        for i in 0..=segments {
            let t = i as f32 / segments as f32;
            let angle = start + (end - start) * t;
            points.push(Pos2::new(
                center.x + r * angle.cos(),
                center.y + r * angle.sin(),
            ));
        }
    }
    points
}

/// Corners of an axis-aligned rectangle, clockwise, starting at the top-left.
fn rect_corners(rect: Rect) -> [Pos2; 4] {
    [
        rect.min,
        Pos2::new(rect.max.x, rect.min.y),
        rect.max,
        Pos2::new(rect.min.x, rect.max.y),
    ]
}

fn circle_path(center: Pos2, radius: f32, segments: usize) -> Vec<Pos2> {
    (0..segments)
        .map(|i| {
            let angle = i as f32 / segments as f32 * 2.0 * PI;
            Pos2::new(
                center.x + radius * angle.cos(),
                center.y + radius * angle.sin(),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{pos2, vec2};

    fn check_mesh_invariants(mesh: &Mesh) {
        assert!(
            mesh.indices.len().is_multiple_of(3),
            "indices must form triangles"
        );
        let vertex_count = mesh.vertices.len() as u32;
        for &i in &mesh.indices {
            assert!(
                i < vertex_count,
                "index {i} out of range ({vertex_count} vertices)"
            );
        }
        for v in &mesh.vertices {
            assert!(v.pos[0].is_finite() && v.pos[1].is_finite());
            // Untextured geometry always samples the white texel.
            assert_eq!(v.uv, Vertex::WHITE_UV);
        }
    }

    #[test]
    fn plain_rect_is_one_quad() {
        let mut tess = Tessellator::new();
        let shape = ClippedShape::new(
            Rect::from_min_max(pos2(0.0, 0.0), pos2(100.0, 100.0)),
            Shape::rect(
                Rect::from_min_size(pos2(10.0, 10.0), vec2(20.0, 10.0)),
                Color32::from_rgb(255, 0, 0),
            ),
        );
        let prims = tess.tessellate(&[shape], 1.0);
        assert_eq!(prims.len(), 1);
        let mesh = &prims[0].mesh;
        check_mesh_invariants(mesh);
        assert_eq!(mesh.indices.len(), 6);
        assert_eq!(mesh.vertices.len(), 4);
    }

    #[test]
    fn clip_rect_is_preserved() {
        let clip = Rect::from_min_max(pos2(5.0, 5.0), pos2(15.0, 15.0));
        let mut tess = Tessellator::new();
        let prims = tess.tessellate(
            &[ClippedShape::new(
                clip,
                Shape::rect(
                    Rect::from_min_size(pos2(0.0, 0.0), vec2(100.0, 100.0)),
                    Color32::WHITE,
                ),
            )],
            1.0,
        );
        assert_eq!(prims[0].clip_rect, clip);
    }

    #[test]
    fn pixels_per_point_scales_positions_and_rounds() {
        let mut tess = Tessellator::new();
        let prims = tess.tessellate(
            &[ClippedShape::new(
                Rect::from_min_max(pos2(0.0, 0.0), pos2(100.0, 100.0)),
                Shape::rect(
                    Rect::from_min_size(pos2(0.3, 0.3), vec2(10.0, 10.0)),
                    Color32::WHITE,
                ),
            )],
            2.0,
        );
        let xs: Vec<f32> = prims[0].mesh.vertices.iter().map(|v| v.pos[0]).collect();
        assert!(xs.iter().all(|x| x.is_finite()));
        assert!(xs.iter().all(|&x| (0.0..=21.0).contains(&x)));
        // 0.3 * 2 = 0.6 -> rounds to 1.0 physical pixel.
        assert!(xs.contains(&1.0));
    }

    #[test]
    fn rounded_rect_path_has_stable_point_count() {
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(100.0, 40.0));
        for radius in [0.0, 1.0, 8.0, 100.0] {
            assert_eq!(
                rounded_rect_path(rect, radius, CORNER_SEGMENTS).len(),
                4 * (CORNER_SEGMENTS + 1)
            );
        }
    }

    #[test]
    fn stroke_produces_a_ring() {
        let mut tess = Tessellator::new();
        let prims = tess.tessellate(
            &[ClippedShape::new(
                Rect::from_min_max(pos2(0.0, 0.0), pos2(100.0, 100.0)),
                Shape::Rect(
                    RectShape::new(
                        Rect::from_min_size(pos2(10.0, 10.0), vec2(50.0, 30.0)),
                        Color32::WHITE,
                    )
                    .with_stroke(crate::Stroke::new(2.0, Color32::BLACK)),
                ),
            )],
            1.0,
        );
        check_mesh_invariants(&prims[0].mesh);
        // 2 triangles for the fill, 4 quads (8 triangles) for the border.
        assert_eq!(prims[0].mesh.indices.len(), 6 + 24);
    }

    #[test]
    fn empty_shapes_are_skipped() {
        let mut tess = Tessellator::new();
        let prims = tess.tessellate(
            &[ClippedShape::new(
                Rect::from_min_max(pos2(0.0, 0.0), pos2(10.0, 10.0)),
                Shape::rect(
                    Rect::from_min_max(pos2(5.0, 5.0), pos2(5.0, 5.0)),
                    Color32::WHITE,
                ),
            )],
            1.0,
        );
        assert!(prims.is_empty());
    }
}
