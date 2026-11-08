//! A stateful shape builder that emits geometry into a shared draw list.

use crate::tessellate::{add_circle, add_line, add_rect};
use crate::{
    CircleShape, Color32, DrawCommand, Mesh, MeshSpace, PaintList, Pos2, Rect, RectShape, Shape,
    TextureId, Vec2,
};

/// Builds a frame's geometry while tracking the current clip rectangle.
///
/// The painter works in logical points and writes physical-pixel vertices directly into a
/// [`PaintList`]. Consecutive shapes that share a clip rectangle and texture are appended to the
/// same vertex/index buffers; only clip or texture changes start a new [`DrawCommand`].
///
/// [`Painter::set_clip_rect`] is absolute: it replaces the current clip (other than the canvas
/// clamp) rather than intersecting with it. Call [`Painter::reset_clip_rect`] to clip to the whole
/// canvas again.
///
/// `pixels_per_point` is captured when the painter is created. Recreate the painter when the DPI
/// scale changes; existing vertices are already in physical pixels.
///
/// Untextured shapes sample [`TextureId::WHITE`] by default. [`Painter::set_solid_texture`] can
/// point them at another texture that reserves an opaque white texel at `(0, 0)`, which lets them
/// batch with meshes that use that texture (for example a glyph atlas).
#[derive(Clone, Debug)]
pub struct Painter {
    canvas_size: Vec2,
    pixels_per_point: f32,
    clip_rect: Rect,
    texture_id: TextureId,
    /// Texture sampled by untextured shapes. Defaults to [`TextureId::WHITE`].
    solid_texture: TextureId,
    run_start: u32,
    list: PaintList,
}

impl Painter {
    /// Create a painter for a canvas of the given size, in logical points.
    pub fn new(width: f32, height: f32, pixels_per_point: f32) -> Self {
        let mut painter = Self {
            canvas_size: Vec2::ZERO,
            pixels_per_point: sanitize_ppp(pixels_per_point),
            clip_rect: Rect::NOTHING,
            texture_id: TextureId::WHITE,
            solid_texture: TextureId::WHITE,
            run_start: 0,
            list: PaintList::new(),
        };
        painter.set_size(width, height);
        painter.reset_clip_rect();
        painter
    }

    /// Create a painter from a size, in logical points.
    ///
    /// This avoids accidentally swapping the width and height arguments of [`Painter::new`].
    pub fn from_size(size: Vec2, pixels_per_point: f32) -> Self {
        Self::new(size.x, size.y, pixels_per_point)
    }

    /// The canvas size, in logical points.
    pub fn size(&self) -> Vec2 {
        self.canvas_size
    }

    /// The pixels-per-point scale captured at construction time.
    pub fn pixels_per_point(&self) -> f32 {
        self.pixels_per_point
    }

    /// Resize the canvas.
    ///
    /// The current clip is intersected with the new canvas; it is never expanded automatically.
    /// Call [`Painter::reset_clip_rect`] to clip to the whole canvas again.
    pub fn set_size(&mut self, width: f32, height: f32) {
        self.canvas_size = sanitize_size(width, height);
        let clip = self.clip_rect.intersect(self.canvas_rect());
        if clip != self.clip_rect {
            self.flush();
            self.clip_rect = clip;
        }
    }

    /// The canvas rectangle, starting at the origin.
    pub fn canvas_rect(&self) -> Rect {
        Rect::from_min_max(
            Pos2::ZERO,
            Pos2::new(self.canvas_size.x, self.canvas_size.y),
        )
    }

    /// Set the clip rectangle for subsequently added shapes.
    ///
    /// The rectangle is absolute, not intersected with the previous clip (other than the canvas).
    /// It is automatically clamped to the canvas. Non-finite rectangles clip everything.
    pub fn set_clip_rect(&mut self, rect: Rect) {
        let clip = sanitize_rect(rect).intersect(self.canvas_rect());
        if clip != self.clip_rect {
            self.flush();
            self.clip_rect = clip;
        }
    }

    /// The effective clip rectangle, already clamped to the canvas.
    pub fn clip_rect(&self) -> Rect {
        self.clip_rect
    }

    /// Clip to the whole canvas.
    pub fn reset_clip_rect(&mut self) {
        let clip = self.canvas_rect();
        if clip != self.clip_rect {
            self.flush();
            self.clip_rect = clip;
        }
    }

    /// Remove all geometry and draw commands. The clip rectangle and solid texture are left
    /// unchanged.
    pub fn clear(&mut self) {
        self.list.clear();
        self.run_start = 0;
        self.texture_id = self.solid_texture;
    }

    /// Set the texture sampled by untextured shapes.
    ///
    /// Untextured shapes sample [`Vertex::WHITE_UV`](crate::Vertex::WHITE_UV), so the texture
    /// must have an opaque white texel at `(0, 0)`. The default is [`TextureId::WHITE`]. A glyph
    /// atlas that reserves a white texel can be used instead, which lets shapes and text share a
    /// draw call.
    ///
    /// Changing the texture affects only subsequently added shapes; the next shape flushes the
    /// current run if the texture differs. The setting survives [`Painter::clear`].
    pub fn set_solid_texture(&mut self, texture_id: TextureId) {
        self.solid_texture = texture_id;
    }

    /// The texture sampled by untextured shapes.
    pub fn solid_texture(&self) -> TextureId {
        self.solid_texture
    }

    /// Add a shape using the current clip.
    ///
    /// Shapes added while the clip is empty are dropped.
    pub fn add(&mut self, shape: Shape) {
        if !self.clip_rect.is_positive() {
            return;
        }

        match shape {
            Shape::Rect(shape) => {
                let texture = self.solid_texture;
                self.use_texture(texture);
                add_rect(&mut self.list, &shape, self.pixels_per_point);
            }
            Shape::Circle(shape) => {
                let texture = self.solid_texture;
                self.use_texture(texture);
                add_circle(&mut self.list, &shape, self.pixels_per_point);
            }
            Shape::Line(shape) => {
                let texture = self.solid_texture;
                self.use_texture(texture);
                add_line(
                    &mut self.list,
                    &shape.points,
                    shape.width,
                    shape.color,
                    self.pixels_per_point,
                );
            }
            Shape::Mesh(mesh) => self.add_mesh(mesh),
        }
    }

    /// Add a filled rectangle using the current clip.
    pub fn rect(&mut self, rect: Rect, fill: Color32) {
        if !self.clip_rect.is_positive() {
            return;
        }
        let texture = self.solid_texture;
        self.use_texture(texture);
        add_rect(
            &mut self.list,
            &RectShape::new(rect, fill),
            self.pixels_per_point,
        );
    }

    /// Add a filled circle using the current clip.
    pub fn circle(&mut self, center: Pos2, radius: f32, fill: Color32) {
        if !self.clip_rect.is_positive() {
            return;
        }
        let texture = self.solid_texture;
        self.use_texture(texture);
        add_circle(
            &mut self.list,
            &CircleShape::new(center, radius, fill),
            self.pixels_per_point,
        );
    }

    /// Add a polyline using the current clip.
    pub fn line(&mut self, points: &[Pos2], width: f32, color: Color32) {
        if !self.clip_rect.is_positive() {
            return;
        }
        let texture = self.solid_texture;
        self.use_texture(texture);
        add_line(&mut self.list, points, width, color, self.pixels_per_point);
    }

    /// Append custom geometry using the current clip.
    ///
    /// Logical-point meshes (the default from [`Mesh::with_texture`]) are scaled by
    /// `pixels_per_point`. Physical-pixel meshes (from [`Mesh::physical`]) are appended unchanged.
    pub fn add_mesh(&mut self, mut mesh: Mesh) {
        if !self.clip_rect.is_positive() || mesh.is_empty() || mesh.vertices.is_empty() {
            return;
        }
        if mesh.space == MeshSpace::Logical {
            let scale = self.pixels_per_point;
            if scale != 1.0 {
                for vertex in &mut mesh.vertices {
                    vertex.pos[0] *= scale;
                    vertex.pos[1] *= scale;
                }
            }
        }
        self.use_texture(mesh.texture_id);
        self.list.append_geometry(&mesh.vertices, &mesh.indices);
    }

    /// Flush the current run and return the completed draw list.
    pub fn finish(&mut self) -> &PaintList {
        self.flush();
        &self.list
    }

    fn use_texture(&mut self, texture_id: TextureId) {
        if self.texture_id != texture_id {
            self.flush();
            self.texture_id = texture_id;
        }
    }

    fn flush(&mut self) {
        let end = self.list.indices.len() as u32;
        if end == self.run_start {
            return;
        }
        self.list.draws.push(DrawCommand {
            clip_rect: self.clip_rect.scale(self.pixels_per_point),
            texture_id: self.texture_id,
            index_range: self.run_start..end,
        });
        self.run_start = end;
    }
}

fn sanitize_size(width: f32, height: f32) -> Vec2 {
    if width.is_finite() && height.is_finite() {
        Vec2::new(width.max(0.0), height.max(0.0))
    } else {
        Vec2::ZERO
    }
}

fn sanitize_rect(rect: Rect) -> Rect {
    if rect.min.is_finite() && rect.max.is_finite() {
        rect
    } else {
        Rect::NOTHING
    }
}

fn sanitize_ppp(pixels_per_point: f32) -> f32 {
    if pixels_per_point.is_finite() && pixels_per_point > 0.0 {
        pixels_per_point
    } else {
        1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Mesh, pos2, vec2};

    fn check_draws(painter: &mut Painter) -> PaintList {
        painter.finish().clone()
    }

    #[test]
    fn clip_is_clamped_to_the_canvas() {
        let mut painter = Painter::new(100.0, 50.0, 1.0);
        painter.set_clip_rect(Rect::from_min_max(pos2(-10.0, -20.0), pos2(200.0, 80.0)));
        assert_eq!(
            painter.clip_rect(),
            Rect::from_min_max(pos2(0.0, 0.0), pos2(100.0, 50.0))
        );
    }

    #[test]
    fn reset_clip_rect_covers_the_canvas() {
        let mut painter = Painter::new(100.0, 50.0, 1.0);
        painter.set_clip_rect(Rect::from_min_size(pos2(10.0, 10.0), vec2(5.0, 5.0)));
        painter.reset_clip_rect();
        assert_eq!(
            painter.clip_rect(),
            Rect::from_min_max(pos2(0.0, 0.0), pos2(100.0, 50.0))
        );
    }

    #[test]
    fn empty_clip_drops_shapes() {
        let mut painter = Painter::new(10.0, 10.0, 1.0);
        painter.set_clip_rect(Rect::from_min_max(pos2(20.0, 20.0), pos2(30.0, 30.0)));
        painter.rect(
            Rect::from_min_size(pos2(0.0, 0.0), vec2(10.0, 10.0)),
            Color32::WHITE,
        );
        let list = check_draws(&mut painter);
        assert!(list.draws.is_empty());
        assert!(list.vertices.is_empty());
    }

    #[test]
    fn non_finite_clip_drops_shapes() {
        let mut painter = Painter::new(10.0, 10.0, 1.0);
        painter.set_clip_rect(Rect::from_min_max(pos2(f32::NAN, 0.0), pos2(10.0, 10.0)));
        painter.rect(
            Rect::from_min_size(pos2(0.0, 0.0), vec2(10.0, 10.0)),
            Color32::WHITE,
        );
        assert_eq!(painter.clip_rect(), Rect::NOTHING);
        let list = check_draws(&mut painter);
        assert!(list.draws.is_empty());
    }

    #[test]
    fn resizing_never_expands_the_current_clip() {
        let mut painter = Painter::new(100.0, 100.0, 1.0);
        let clip = Rect::from_min_max(pos2(0.0, 0.0), pos2(20.0, 20.0));
        painter.set_clip_rect(clip);

        painter.set_size(200.0, 200.0);
        assert_eq!(painter.clip_rect(), clip);
        assert_eq!(painter.size(), vec2(200.0, 200.0));

        painter.set_size(10.0, 10.0);
        assert_eq!(
            painter.clip_rect(),
            Rect::from_min_max(pos2(0.0, 0.0), pos2(10.0, 10.0))
        );
    }

    #[test]
    fn clear_keeps_the_clip_and_clears_geometry() {
        let mut painter = Painter::new(100.0, 100.0, 1.0);
        let clip = Rect::from_min_max(pos2(10.0, 10.0), pos2(50.0, 50.0));
        painter.set_clip_rect(clip);
        painter.rect(
            Rect::from_min_size(pos2(0.0, 0.0), vec2(10.0, 10.0)),
            Color32::WHITE,
        );

        painter.clear();
        assert_eq!(painter.clip_rect(), clip);
        let list = check_draws(&mut painter);
        assert!(list.draws.is_empty());
        assert!(list.vertices.is_empty());
        assert!(list.indices.is_empty());

        painter.rect(
            Rect::from_min_size(pos2(0.0, 0.0), vec2(10.0, 10.0)),
            Color32::WHITE,
        );
        assert_eq!(check_draws(&mut painter).draws.len(), 1);
    }

    #[test]
    fn solid_texture_defaults_to_white() {
        let painter = Painter::new(100.0, 100.0, 1.0);
        assert_eq!(painter.solid_texture(), TextureId::WHITE);
    }

    #[test]
    fn solid_texture_batches_shapes_and_matching_meshes() {
        let mut painter = Painter::new(100.0, 100.0, 1.0);
        let texture = TextureId(7);
        painter.set_solid_texture(texture);

        painter.rect(
            Rect::from_min_size(pos2(0.0, 0.0), vec2(10.0, 10.0)),
            Color32::WHITE,
        );
        painter.circle(pos2(30.0, 30.0), 5.0, Color32::WHITE);

        let mut mesh = Mesh::with_texture(texture);
        mesh.add_rect_with_uv(
            Rect::from_min_size(pos2(50.0, 50.0), vec2(10.0, 10.0)),
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            Color32::WHITE,
        );
        painter.add_mesh(mesh);

        let list = check_draws(&mut painter);
        assert_eq!(list.draws.len(), 1);
        assert_eq!(list.draws[0].texture_id, texture);
    }

    #[test]
    fn solid_texture_survives_clear() {
        let mut painter = Painter::new(100.0, 100.0, 1.0);
        let texture = TextureId(7);
        painter.set_solid_texture(texture);
        painter.rect(
            Rect::from_min_size(pos2(0.0, 0.0), vec2(10.0, 10.0)),
            Color32::WHITE,
        );

        painter.clear();
        painter.rect(
            Rect::from_min_size(pos2(0.0, 0.0), vec2(10.0, 10.0)),
            Color32::WHITE,
        );

        let list = check_draws(&mut painter);
        assert_eq!(list.draws.len(), 1);
        assert_eq!(list.draws[0].texture_id, texture);
    }

    #[test]
    fn changing_the_solid_texture_starts_a_new_draw() {
        let mut painter = Painter::new(100.0, 100.0, 1.0);
        painter.rect(
            Rect::from_min_size(pos2(0.0, 0.0), vec2(10.0, 10.0)),
            Color32::WHITE,
        );
        painter.set_solid_texture(TextureId(7));
        painter.rect(
            Rect::from_min_size(pos2(20.0, 20.0), vec2(10.0, 10.0)),
            Color32::WHITE,
        );

        let list = check_draws(&mut painter);
        assert_eq!(list.draws.len(), 2);
        assert_eq!(list.draws[0].texture_id, TextureId::WHITE);
        assert_eq!(list.draws[1].texture_id, TextureId(7));
    }

    #[test]
    fn same_clip_appends_to_one_draw() {
        let mut painter = Painter::new(100.0, 100.0, 1.0);
        let clip = Rect::from_min_max(pos2(0.0, 0.0), pos2(50.0, 50.0));
        painter.set_clip_rect(clip);
        painter.rect(
            Rect::from_min_size(pos2(1.0, 1.0), vec2(10.0, 10.0)),
            Color32::WHITE,
        );
        // Setting the same clip again must not flush the current run.
        painter.set_clip_rect(clip);
        painter.rect(
            Rect::from_min_size(pos2(20.0, 20.0), vec2(10.0, 10.0)),
            Color32::WHITE,
        );

        let list = check_draws(&mut painter);
        assert_eq!(list.draws.len(), 1);
        assert_eq!(list.draws[0].clip_rect, clip);
        assert_eq!(list.draws[0].index_range, 0..12);
        assert_eq!(list.vertices.len(), 8);
        assert_eq!(list.indices.len(), 12);
    }

    #[test]
    fn different_clips_start_new_draws_in_order() {
        let mut painter = Painter::new(100.0, 100.0, 1.0);
        let clip_a = Rect::from_min_max(pos2(0.0, 0.0), pos2(50.0, 100.0));
        let clip_b = Rect::from_min_max(pos2(50.0, 0.0), pos2(100.0, 100.0));
        let rect = Rect::from_min_size(pos2(10.0, 10.0), vec2(10.0, 10.0));

        painter.set_clip_rect(clip_a);
        painter.rect(rect, Color32::WHITE);
        painter.set_clip_rect(clip_b);
        painter.rect(rect, Color32::WHITE);
        painter.set_clip_rect(clip_a);
        painter.rect(rect, Color32::WHITE);

        let list = check_draws(&mut painter);
        assert_eq!(list.draws.len(), 3);
        assert_eq!(list.draws[0].clip_rect, clip_a);
        assert_eq!(list.draws[1].clip_rect, clip_b);
        assert_eq!(list.draws[2].clip_rect, clip_a);
        assert_eq!(list.draws[0].index_range, 0..6);
        assert_eq!(list.draws[1].index_range, 6..12);
        assert_eq!(list.draws[2].index_range, 12..18);
    }

    #[test]
    fn texture_changes_start_new_draws() {
        let mut painter = Painter::new(100.0, 100.0, 1.0);
        let clip = Rect::from_min_max(pos2(0.0, 0.0), pos2(100.0, 100.0));
        painter.set_clip_rect(clip);

        for texture_id in [TextureId(1), TextureId(2)] {
            let mut mesh = Mesh::with_texture(texture_id);
            mesh.add_rect_with_uv(
                Rect::from_min_size(pos2(0.0, 0.0), vec2(10.0, 10.0)),
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                Color32::WHITE,
            );
            painter.add(Shape::Mesh(mesh));
        }

        let list = check_draws(&mut painter);
        assert_eq!(list.draws.len(), 2);
        assert_eq!(list.draws[0].texture_id, TextureId(1));
        assert_eq!(list.draws[1].texture_id, TextureId(2));
        assert_eq!(list.draws[0].index_range, 0..6);
        assert_eq!(list.draws[1].index_range, 6..12);
    }

    #[test]
    fn same_texture_appends_to_one_draw() {
        let mut painter = Painter::new(100.0, 100.0, 1.0);
        let clip = Rect::from_min_max(pos2(0.0, 0.0), pos2(100.0, 100.0));
        painter.set_clip_rect(clip);

        for _ in 0..2 {
            let mut mesh = Mesh::with_texture(TextureId(1));
            mesh.add_rect_with_uv(
                Rect::from_min_size(pos2(0.0, 0.0), vec2(10.0, 10.0)),
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                Color32::WHITE,
            );
            painter.add_mesh(mesh);
        }

        let list = check_draws(&mut painter);
        assert_eq!(list.draws.len(), 1);
        assert_eq!(list.vertices.len(), 8);
        assert_eq!(list.indices.len(), 12);
        assert!(list.indices.iter().all(|&index| index < 8));
    }

    #[test]
    fn logical_meshes_are_scaled_when_adding() {
        let mut painter = Painter::new(100.0, 100.0, 2.0);
        let mut mesh = Mesh::with_texture(TextureId(1));
        mesh.add_rect_with_uv(
            Rect::from_min_size(pos2(1.0, 1.0), vec2(10.0, 10.0)),
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            Color32::WHITE,
        );
        painter.add_mesh(mesh);

        let list = check_draws(&mut painter);
        assert_eq!(list.vertices[0].pos, [2.0, 2.0]);
        assert_eq!(list.vertices[2].pos, [22.0, 22.0]);
    }

    #[test]
    fn physical_meshes_are_not_scaled_when_adding() {
        let mut painter = Painter::new(100.0, 100.0, 2.0);
        let mut mesh = Mesh::physical(TextureId(1));
        mesh.add_rect_with_uv(
            Rect::from_min_size(pos2(1.0, 1.0), vec2(10.0, 10.0)),
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            Color32::WHITE,
        );
        painter.add_mesh(mesh);

        let list = check_draws(&mut painter);
        assert_eq!(list.vertices[0].pos, [1.0, 1.0]);
        assert_eq!(list.vertices[2].pos, [11.0, 11.0]);
    }

    #[test]
    fn shapes_respect_the_mesh_space() {
        let mut painter = Painter::new(100.0, 100.0, 2.0);
        let mut mesh = Mesh::physical(TextureId(1));
        mesh.add_rect_with_uv(
            Rect::from_min_size(pos2(1.0, 1.0), vec2(10.0, 10.0)),
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            Color32::WHITE,
        );
        painter.add(Shape::Mesh(mesh));

        let list = check_draws(&mut painter);
        assert_eq!(list.vertices[0].pos, [1.0, 1.0]);
    }

    #[test]
    fn pixels_per_point_is_applied_when_adding() {
        let mut painter = Painter::new(100.0, 100.0, 2.0);
        painter.rect(
            Rect::from_min_size(pos2(1.0, 1.0), vec2(10.0, 10.0)),
            Color32::WHITE,
        );
        let list = check_draws(&mut painter);
        assert_eq!(list.vertices[0].pos, [2.0, 2.0]);
        assert_eq!(list.vertices[2].pos, [22.0, 22.0]);
        // Vertices and clip rectangles share the same physical-pixel space.
        assert_eq!(
            list.draws[0].clip_rect,
            Rect::from_min_max(pos2(0.0, 0.0), pos2(200.0, 200.0))
        );
    }

    #[test]
    fn shrinking_the_canvas_starts_a_new_draw() {
        let mut painter = Painter::new(100.0, 100.0, 1.0);
        painter.rect(
            Rect::from_min_size(pos2(0.0, 0.0), vec2(10.0, 10.0)),
            Color32::WHITE,
        );
        painter.set_size(50.0, 50.0);
        painter.rect(
            Rect::from_min_size(pos2(0.0, 0.0), vec2(10.0, 10.0)),
            Color32::WHITE,
        );

        let list = check_draws(&mut painter);
        assert_eq!(list.draws.len(), 2);
        assert_eq!(
            list.draws[0].clip_rect,
            Rect::from_min_max(pos2(0.0, 0.0), pos2(100.0, 100.0))
        );
        assert_eq!(
            list.draws[1].clip_rect,
            Rect::from_min_max(pos2(0.0, 0.0), pos2(50.0, 50.0))
        );
    }

    #[test]
    fn degenerate_canvas_is_empty() {
        let mut painter = Painter::new(-1.0, f32::NAN, 1.0);
        assert!(!painter.canvas_rect().is_positive());
        painter.rect(
            Rect::from_min_size(pos2(0.0, 0.0), vec2(10.0, 10.0)),
            Color32::WHITE,
        );
        let list = check_draws(&mut painter);
        assert!(list.draws.is_empty());
    }
}
