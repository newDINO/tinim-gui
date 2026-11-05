//! The per-frame UI builder.
//!
//! v0.1 deliberately has no ids and no persistent widget state. Widgets are rebuilt every frame;
//! text layout is the only cached thing.

use tinim_paint::{Color32, Painter, Pos2, Rect, RectShape, Shape, Vec2};
use tinim_text::{TextLayout, TextSystem};

use crate::context::Context;
use crate::input::RawInput;

/// A basic vertical/horizontal flow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Direction {
    Vertical,
    Horizontal,
}

struct Region {
    /// Area available to this stack.
    rect: Rect,
    /// Clip inherited from the parent, narrowed to `rect`.
    clip: Rect,
    /// Where the next child starts.
    cursor: Pos2,
    /// Union of the rectangles allocated by children.
    used: Rect,
    direction: Direction,
    spacing: f32,
}

/// What a widget reports for this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Response {
    pub rect: Rect,
    pub hovered: bool,
    /// The primary button was pressed on this widget during this frame.
    pub pressed: bool,
    /// The primary button is currently held over this widget.
    pub down: bool,
}

/// Builds one frame's widgets.
///
/// Widgets normally follow the current region's flow, or start at an arbitrary position with
/// [`Ui::place`]. Overlapping widgets are painted in declaration order, but hit testing does not
/// resolve occlusion: a later widget does not suppress an earlier widget's `hovered`/`pressed`.
pub struct Ui<'a> {
    ctx: &'a mut Context,
    regions: Vec<Region>,
}

impl<'a> Ui<'a> {
    pub(crate) fn new(ctx: &'a mut Context) -> Self {
        let rect = Rect::from_min_size(Pos2::ZERO, ctx.size());
        let spacing = ctx.style().spacing.max(0.0);
        Self {
            ctx,
            regions: vec![Region {
                rect,
                clip: rect,
                cursor: rect.min,
                used: Rect::NOTHING,
                direction: Direction::Vertical,
                spacing,
            }],
        }
    }

    /// Draws a non-interactive line of text. Returns the rectangle it occupied.
    pub fn label(&mut self, text: &str) -> Rect {
        let font = self.ctx.style().font.clone();
        let max_width = Some(available_size(&self.regions).x);
        // Borrow only the fields involved. `layout` keeps `text_cache` borrowed, so everything
        // that runs while it is alive must use disjoint field borrows instead of `&mut self.ctx`.
        let layout = self
            .ctx
            .text_cache
            .layout(&self.ctx.text, text, &font, max_width);
        let size = layout.size();
        let rect = allocate_space(&mut self.regions, size);

        let clip = current_clip(&self.regions);
        paint_text(
            &mut self.ctx.text,
            &mut self.ctx.painter,
            self.ctx.pixels_per_point,
            layout,
            rect.min,
            font.color,
            clip,
        );
        rect
    }

    /// Draws a button.
    ///
    /// v0.1 only reports a press edge; there is no click state, id or press/release pairing.
    pub fn button(&mut self, text: &str) -> Response {
        let button = self.ctx.style().button.clone();
        let mut font = self.ctx.style().font.clone();
        font.color = button.text;

        let layout = self
            .ctx
            .text_cache
            .layout(&self.ctx.text, text, &font, None);
        let text_size = layout.size();
        let size = Vec2::new(
            (text_size.x + button.padding.x * 2.0).max(button.min_size.x),
            (text_size.y + button.padding.y * 2.0).max(button.min_size.y),
        );
        let rect = allocate_space(&mut self.regions, size);

        let hovered = pointer_over(&self.regions, &self.ctx.input, rect);
        let pressed = hovered && self.ctx.input.primary_pressed;
        let down = hovered && self.ctx.input.primary_down;

        let region_clip = current_clip(&self.regions);
        let button_clip = region_clip.intersect(rect);
        if button_clip.is_positive() {
            let fill = if down || pressed {
                button.pressed
            } else if hovered {
                button.hovered
            } else {
                button.normal
            };

            self.ctx.painter.set_clip_rect(button_clip);
            self.ctx.painter.add(Shape::Rect(
                RectShape::new(rect, fill).with_corner_radius(button.corner_radius),
            ));

            let origin = rect.center() - text_size * 0.5;
            let mesh = self
                .ctx
                .text
                .prepare(layout, origin, font.color, self.ctx.pixels_per_point);
            self.ctx.painter.add_mesh(mesh);
            self.ctx.painter.set_clip_rect(region_clip);
        }

        Response {
            rect,
            hovered,
            pressed,
            down,
        }
    }

    /// Stacks children vertically.
    pub fn vertical<R>(&mut self, build: impl FnOnce(&mut Ui<'_>) -> R) -> R {
        self.with_region(Direction::Vertical, build)
    }

    /// Stacks children horizontally.
    pub fn horizontal<R>(&mut self, build: impl FnOnce(&mut Ui<'_>) -> R) -> R {
        self.with_region(Direction::Horizontal, build)
    }

    /// Starts a vertical region at an arbitrary position.
    ///
    /// The region extends to the current region's bottom-right corner. It is out of flow: the
    /// parent cursor does not advance and the region does not contribute to the parent's used
    /// rectangle.
    pub fn place<R>(&mut self, pos: Pos2, build: impl FnOnce(&mut Ui<'_>) -> R) -> R {
        self.with_region_at(Direction::Vertical, pos, false, build)
    }

    /// Reserves a rectangle in the current stack and advances the cursor.
    pub fn allocate_space(&mut self, size: Vec2) -> Rect {
        allocate_space(&mut self.regions, size)
    }

    /// Space left in the current stack.
    pub fn available_size(&self) -> Vec2 {
        available_size(&self.regions)
    }

    fn with_region<R>(&mut self, direction: Direction, build: impl FnOnce(&mut Ui<'_>) -> R) -> R {
        let start = self.regions.last().expect("root region").cursor;
        self.with_region_at(direction, start, true, build)
    }

    fn with_region_at<R>(
        &mut self,
        direction: Direction,
        start: Pos2,
        advance_parent: bool,
        build: impl FnOnce(&mut Ui<'_>) -> R,
    ) -> R {
        let (max, clip, spacing) = {
            let parent = self.regions.last().expect("root region");
            (parent.rect.max, parent.clip, parent.spacing)
        };
        let rect = Rect::from_min_max(start, max);

        self.regions.push(Region {
            rect,
            clip: clip.intersect(rect),
            cursor: start,
            used: Rect::NOTHING,
            direction,
            spacing,
        });

        let result = build(self);
        let child = self.regions.pop().expect("child region");

        if advance_parent && child.used.is_positive() {
            let parent = self.regions.last_mut().expect("root region");
            match parent.direction {
                Direction::Vertical => parent.cursor.y += child.used.height() + parent.spacing,
                Direction::Horizontal => parent.cursor.x += child.used.width() + parent.spacing,
            }
            parent.used = if parent.used.is_positive() {
                parent.used.union(child.used)
            } else {
                child.used
            };
        }

        result
    }
}

fn allocate_space(regions: &mut [Region], size: Vec2) -> Rect {
    let size = if size.is_finite() {
        Vec2::new(size.x.max(0.0), size.y.max(0.0))
    } else {
        Vec2::ZERO
    };

    let region = regions.last_mut().expect("root region");
    let rect = match region.direction {
        Direction::Vertical => {
            Rect::from_min_size(Pos2::new(region.rect.min.x, region.cursor.y), size)
        }
        Direction::Horizontal => {
            Rect::from_min_size(Pos2::new(region.cursor.x, region.rect.min.y), size)
        }
    };

    match region.direction {
        Direction::Vertical => region.cursor.y += size.y + region.spacing,
        Direction::Horizontal => region.cursor.x += size.x + region.spacing,
    }
    region.used = if region.used.is_positive() {
        region.used.union(rect)
    } else {
        rect
    };
    rect
}

fn available_size(regions: &[Region]) -> Vec2 {
    let region = regions.last().expect("root region");
    match region.direction {
        Direction::Vertical => Vec2::new(
            region.rect.width(),
            (region.rect.max.y - region.cursor.y).max(0.0),
        ),
        Direction::Horizontal => Vec2::new(
            (region.rect.max.x - region.cursor.x).max(0.0),
            region.rect.height(),
        ),
    }
}

fn current_clip(regions: &[Region]) -> Rect {
    regions.last().expect("root region").clip
}

fn pointer_over(regions: &[Region], input: &RawInput, rect: Rect) -> bool {
    let clip = current_clip(regions);
    input
        .pointer
        .is_some_and(|pointer| rect.contains(pointer) && clip.contains(pointer))
}

fn paint_text(
    text: &mut TextSystem,
    painter: &mut Painter,
    pixels_per_point: f32,
    layout: &TextLayout,
    origin: Pos2,
    color: Color32,
    clip: Rect,
) {
    if !clip.is_positive() {
        return;
    }
    painter.set_clip_rect(clip);
    let mesh = text.prepare(layout, origin, color, pixels_per_point);
    painter.add_mesh(mesh);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Context, RawInput, TextureId, pos2, vec2};

    fn context() -> Context {
        let mut context = Context::new(TextureId(1));
        context
            .text_mut()
            .add_font(font_test_data::AHEM.to_vec())
            .unwrap();
        context
    }

    #[test]
    fn vertical_stack_advances_by_widget_height_and_spacing() {
        let mut context = context();
        let spacing = context.style().spacing;
        let mut rects = Vec::new();

        let _ = context.run(vec2(200.0, 200.0), 1.0, RawInput::default(), |ui| {
            ui.vertical(|ui| {
                rects.push(ui.button("one").rect);
                rects.push(ui.button("two").rect);
            });
        });

        assert_eq!(rects.len(), 2);
        assert_eq!(rects[0].min, pos2(0.0, 0.0));
        assert_eq!(rects[1].min.y, rects[0].max.y + spacing);
        assert_eq!(rects[1].min.x, rects[0].min.x);
    }

    #[test]
    fn horizontal_stack_advances_by_widget_width_and_spacing() {
        let mut context = context();
        let spacing = context.style().spacing;
        let mut rects = Vec::new();

        let _ = context.run(vec2(300.0, 200.0), 1.0, RawInput::default(), |ui| {
            ui.horizontal(|ui| {
                rects.push(ui.button("one").rect);
                rects.push(ui.button("two").rect);
            });
        });

        assert_eq!(rects.len(), 2);
        assert_eq!(rects[0].min, pos2(0.0, 0.0));
        assert_eq!(rects[1].min.x, rects[0].max.x + spacing);
        assert_eq!(rects[1].min.y, rects[0].min.y);
    }

    #[test]
    fn button_reports_press_only_when_pointer_is_over_it() {
        let mut context = context();
        let mut pressed = None;

        let input = RawInput {
            pointer: Some(pos2(1.0, 1.0)),
            primary_down: true,
            primary_pressed: true,
            ..Default::default()
        };
        let _ = context.run(vec2(200.0, 100.0), 1.0, input, |ui| {
            pressed = Some(ui.button("go").pressed);
        });

        assert_eq!(pressed, Some(true));
    }

    #[test]
    fn button_does_not_press_when_pointer_is_outside() {
        let mut context = context();
        let mut pressed = None;

        let input = RawInput {
            pointer: Some(pos2(500.0, 500.0)),
            primary_down: true,
            primary_pressed: true,
            ..Default::default()
        };
        let _ = context.run(vec2(200.0, 100.0), 1.0, input, |ui| {
            pressed = Some(ui.button("go").pressed);
        });

        assert_eq!(pressed, Some(false));
    }

    #[test]
    fn label_returns_the_allocated_rectangle() {
        let mut context = context();
        let mut label_rect = Rect::NOTHING;

        let _ = context.run(vec2(200.0, 200.0), 1.0, RawInput::default(), |ui| {
            label_rect = ui.label("hello");
        });

        assert!(label_rect.is_positive());
        assert_eq!(label_rect.min, pos2(0.0, 0.0));
    }

    #[test]
    fn place_starts_a_region_at_an_arbitrary_position_without_advancing_the_parent() {
        let mut context = context();
        let spacing = context.style().spacing;
        let mut first = Rect::NOTHING;
        let mut placed = Rect::NOTHING;
        let mut next = Rect::NOTHING;

        let _ = context.run(vec2(400.0, 300.0), 1.0, RawInput::default(), |ui| {
            first = ui.label("first");
            placed = ui.place(pos2(120.0, 80.0), |ui| ui.label("floating"));
            next = ui.label("next");
        });

        assert_eq!(placed.min, pos2(120.0, 80.0));
        assert_eq!(next.min.x, first.min.x);
        assert_eq!(next.min.y, first.max.y + spacing);
    }
}
