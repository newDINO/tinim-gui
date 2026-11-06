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
    /// Position that relative coordinates in this region are measured from.
    origin: Pos2,
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

/// The result of building a [`Ui::pane`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PaneResponse {
    /// The viewport rectangle allocated in the parent flow.
    pub rect: Rect,
    /// Extent of the in-flow content, independent of the pane's offset.
    ///
    /// Out-of-flow children such as [`Ui::place`] are not included.
    pub content_size: Vec2,
}

/// Caller-owned state for [`Ui::scrollable`].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ScrollState {
    /// Current scroll position. Zero shows the content origin.
    pub offset: Vec2,
    /// Content size measured on the previous frame, if any.
    ///
    /// When this is known, the scroll position and wheel delta are clamped before layout, so
    /// reaching a boundary takes effect in the same frame. It is `None` on the first frame.
    pub content_size: Option<Vec2>,
}

/// The result of building a [`Ui::scrollable`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollResponse {
    /// The viewport rectangle allocated in the parent flow.
    pub rect: Rect,
    /// Extent of the in-flow content.
    pub content_size: Vec2,
    /// Largest valid scroll position: `(content_size - rect.size()).max(0)`.
    pub max_offset: Vec2,
    /// Whether the pointer was over the viewport when the scrollable was built.
    pub hovered: bool,
}

/// Builds one frame's widgets.
///
/// Widgets normally follow the current region's flow, or start at an arbitrary position with
/// [`Ui::place`]. Overlapping widgets are painted in declaration order, but hit testing does not
/// resolve occlusion: a later widget does not suppress an earlier widget's `hovered`/`pressed`.
pub struct Ui<'a> {
    ctx: &'a mut Context,
    regions: Vec<Region>,
    /// Scroll delta not yet consumed by a scrollable.
    scroll_remaining: Vec2,
}

impl<'a> Ui<'a> {
    pub(crate) fn new(ctx: &'a mut Context) -> Self {
        let rect = Rect::from_min_size(Pos2::ZERO, ctx.size());
        let spacing = ctx.style().spacing.max(0.0);
        let scroll_remaining = if ctx.input.scroll.is_finite() {
            ctx.input.scroll
        } else {
            Vec2::ZERO
        };
        Self {
            ctx,
            regions: vec![Region {
                rect,
                clip: rect,
                origin: rect.min,
                cursor: rect.min,
                used: Rect::NOTHING,
                direction: Direction::Vertical,
                spacing,
            }],
            scroll_remaining,
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
            .layout(&self.ctx.text_system, text, &font, max_width);
        let size = layout.size();
        let rect = allocate_space(&mut self.regions, size);

        let clip = current_clip(&self.regions);
        paint_text(
            &mut self.ctx.text_system,
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
        self.draw_button(text, false)
    }

    /// Draws a selectable button and updates `current_value` when it is pressed.
    ///
    /// The widget is selected when `current_value == selected_value`.
    ///
    /// Like [`Ui::button`], this reports a press edge rather than a full click.
    pub fn selectable_value<Value: PartialEq>(
        &mut self,
        current_value: &mut Value,
        selected_value: Value,
        text: &str,
    ) -> Response {
        let selected = *current_value == selected_value;
        let response = self.draw_button(text, selected);
        if response.pressed {
            *current_value = selected_value;
        }
        response
    }

    pub fn selectable_label(&mut self, selected: bool, text: &str) -> Response {
        self.draw_button(text, selected)
    }

    fn draw_button(&mut self, text: &str, selected: bool) -> Response {
        let button = self.ctx.style().button.clone();
        let mut font = self.ctx.style().font.clone();
        font.color = button.text;

        let layout = self
            .ctx
            .text_cache
            .layout(&self.ctx.text_system, text, &font, None);
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
            let fill = if selected {
                if down || pressed {
                    button.pressed
                } else {
                    button.selected
                }
            } else if down || pressed {
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
            let mesh =
                self.ctx
                    .text_system
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

    /// Starts a vertical region at `pos`, relative to the current region's content origin.
    ///
    /// For ordinary regions the content origin is the region's top-left corner. Inside a
    /// [`Ui::pane`] it is shifted by the pane's offset, so placed content scrolls with the pane
    /// content.
    ///
    /// The region extends to the current region's bottom-right corner. It is out of flow: the
    /// parent cursor does not advance and the region does not contribute to the parent's used
    /// rectangle.
    pub fn place<R>(&mut self, pos: Pos2, build: impl FnOnce(&mut Ui<'_>) -> R) -> R {
        let start = self.regions.last().expect("root region").origin + pos;
        self.with_region_at(Direction::Vertical, start, false, build)
    }

    /// Builds content in an in-flow viewport of a fixed `size`.
    ///
    /// `offset` translates the content: the content origin is `rect.min + offset`, so a positive
    /// offset moves content right and down. Content is clipped to the viewport, and the parent
    /// flow advances by `size` rather than by the content's measured size.
    ///
    /// Returns the closure's result, the viewport rectangle and the measured extent of the
    /// in-flow content. The extent does not depend on `offset`. Out-of-flow children such as
    /// [`Ui::place`] are not included.
    ///
    /// To create an out-of-flow pane, wrap it in [`Ui::place`].
    pub fn pane<R>(
        &mut self,
        size: Vec2,
        offset: Vec2,
        build: impl FnOnce(&mut Ui<'_>) -> R,
    ) -> (R, PaneResponse) {
        let rect = allocate_space(&mut self.regions, size);
        let clip = current_clip(&self.regions).intersect(rect);
        let spacing = self.regions.last().expect("root region").spacing;
        let origin = rect.min + offset;

        self.regions.push(Region {
            rect,
            clip,
            origin,
            cursor: origin,
            used: Rect::NOTHING,
            direction: Direction::Vertical,
            spacing,
        });

        let result = build(self);
        let child = self.regions.pop().expect("child region");

        let content_size = if child.used.is_positive() {
            (child.used.max - child.origin).max(Vec2::ZERO)
        } else {
            Vec2::ZERO
        };

        (result, PaneResponse { rect, content_size })
    }

    /// Builds scrollable content in an in-flow viewport of a fixed `size`.
    ///
    /// `state.offset` is the scroll position: zero shows the content origin, and positive values
    /// reveal content to the right/below, moving the content left/up.
    ///
    /// When `state.content_size` is known from a previous frame, the offset and wheel delta are
    /// clamped before layout, so reaching a boundary takes effect in the same frame. After layout
    /// the new content size is stored in `state`, and the offset is clamped again as a fallback
    /// for content that changed size. On the first frame, and when content changes size, the
    /// fallback clamp is only visible on the next frame.
    ///
    /// When the pointer is over the viewport, this consumes the frame's scroll input. Nested
    /// scrollables are not prioritized: the outermost scrollable under the pointer consumes it.
    pub fn scrollable<R>(
        &mut self,
        size: Vec2,
        state: &mut ScrollState,
        build: impl FnOnce(&mut Ui<'_>) -> R,
    ) -> (R, ScrollResponse) {
        let size = sanitize_size(size);
        let rect = next_rect(&self.regions, size);
        let hovered = pointer_over(&self.regions, &self.ctx.input, rect);
        let known_max_offset = state
            .content_size
            .map(|content_size| (content_size - size).max(Vec2::ZERO));

        if !state.offset.is_finite() {
            state.offset = Vec2::ZERO;
        }
        state.offset = state.offset.max(Vec2::ZERO);
        if let Some(max_offset) = known_max_offset {
            state.offset = state.offset.min(max_offset);
        }

        // Inner scrollable consumes input first.
        let (result, pane) = self.pane(size, -state.offset, build);

        if hovered && self.scroll_remaining != Vec2::ZERO {
            // RawInput uses screen axes (positive y scrolls up), while `offset` counts how far the
            // content has moved left/up.
            let delta = Vec2::new(self.scroll_remaining.x, -self.scroll_remaining.y);
            state.offset = (state.offset + delta).max(Vec2::ZERO);
            if let Some(max_offset) = known_max_offset {
                state.offset = state.offset.min(max_offset);
            }
            self.scroll_remaining = Vec2::ZERO;
        }

        let max_offset = (pane.content_size - size).max(Vec2::ZERO);
        state.content_size = Some(pane.content_size);
        state.offset = state.offset.clamp(Vec2::ZERO, max_offset);

        (
            result,
            ScrollResponse {
                rect,
                content_size: pane.content_size,
                max_offset,
                hovered,
            },
        )
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
            origin: start,
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

fn sanitize_size(size: Vec2) -> Vec2 {
    if size.is_finite() {
        Vec2::new(size.x.max(0.0), size.y.max(0.0))
    } else {
        Vec2::ZERO
    }
}

fn next_rect(regions: &[Region], size: Vec2) -> Rect {
    let region = regions.last().expect("root region");
    match region.direction {
        Direction::Vertical => {
            Rect::from_min_size(Pos2::new(region.origin.x, region.cursor.y), size)
        }
        Direction::Horizontal => {
            Rect::from_min_size(Pos2::new(region.cursor.x, region.origin.y), size)
        }
    }
}

fn allocate_space(regions: &mut [Region], size: Vec2) -> Rect {
    let size = sanitize_size(size);
    let rect = next_rect(regions, size);

    let region = regions.last_mut().expect("root region");
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
            .text_system_mut()
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
    fn selectable_value_updates_the_value_when_pressed() {
        let mut context = context();
        let mut value = 1;

        let input = RawInput {
            pointer: Some(pos2(1.0, 1.0)),
            primary_down: true,
            primary_pressed: true,
            ..Default::default()
        };
        let _ = context.run(vec2(200.0, 100.0), 1.0, input, |ui| {
            ui.selectable_value(&mut value, 2, "two");
        });

        assert_eq!(value, 2);
    }

    #[test]
    fn selectable_value_keeps_the_value_when_not_pressed() {
        let mut context = context();
        let mut value = 1;

        let _ = context.run(vec2(200.0, 100.0), 1.0, RawInput::default(), |ui| {
            ui.selectable_value(&mut value, 2, "two");
        });

        assert_eq!(value, 1);
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
    fn place_starts_a_region_at_a_position_relative_to_the_region_origin() {
        let mut context = context();
        let spacing = context.style().spacing;
        let mut first = Rect::NOTHING;
        let mut placed = Rect::NOTHING;
        let mut nested = Rect::NOTHING;
        let mut next = Rect::NOTHING;

        let _ = context.run(vec2(400.0, 300.0), 1.0, RawInput::default(), |ui| {
            first = ui.label("first");
            placed = ui.place(pos2(120.0, 80.0), |ui| ui.label("floating"));
            ui.place(pos2(200.0, 100.0), |ui| {
                nested = ui.place(pos2(5.0, 6.0), |ui| ui.label("nested"));
            });
            next = ui.label("next");
        });

        assert_eq!(placed.min, pos2(120.0, 80.0));
        assert_eq!(nested.min, pos2(205.0, 106.0));
        assert_eq!(next.min.x, first.min.x);
        assert_eq!(next.min.y, first.max.y + spacing);
    }

    #[test]
    fn pane_allocates_its_size_and_translates_content_by_offset() {
        let mut context = context();
        let spacing = context.style().spacing;
        let mut inside = Rect::NOTHING;
        let mut after = Rect::NOTHING;
        let mut pane = None;

        let _ = context.run(vec2(200.0, 200.0), 1.0, RawInput::default(), |ui| {
            let ((), response) = ui.pane(vec2(100.0, 50.0), vec2(10.0, 5.0), |ui| {
                inside = ui.allocate_space(vec2(20.0, 10.0));
            });
            pane = Some(response);
            after = ui.allocate_space(vec2(20.0, 10.0));
        });

        let pane = pane.unwrap();
        assert_eq!(
            pane.rect,
            Rect::from_min_size(pos2(0.0, 0.0), vec2(100.0, 50.0))
        );
        assert_eq!(pane.content_size, vec2(20.0, 10.0));
        assert_eq!(inside.min, pos2(10.0, 5.0));
        assert_eq!(after.min, pos2(0.0, 50.0 + spacing));
    }

    #[test]
    fn pane_measures_content_in_nested_regions() {
        let mut context = context();
        let spacing = context.style().spacing;
        let mut pane = None;

        let _ = context.run(vec2(200.0, 200.0), 1.0, RawInput::default(), |ui| {
            let ((), response) = ui.pane(vec2(100.0, 100.0), Vec2::ZERO, |ui| {
                ui.vertical(|ui| {
                    ui.allocate_space(vec2(10.0, 10.0));
                    ui.allocate_space(vec2(10.0, 10.0));
                });
            });
            pane = Some(response);
        });

        assert_eq!(pane.unwrap().content_size, vec2(10.0, 20.0 + spacing));
    }

    #[test]
    fn pane_content_size_does_not_depend_on_offset() {
        let mut context = context();
        let mut sizes = Vec::new();

        for offset in [vec2(0.0, 0.0), vec2(7.0, 13.0)] {
            let _ = context.run(vec2(200.0, 200.0), 1.0, RawInput::default(), |ui| {
                let ((), response) = ui.pane(vec2(100.0, 100.0), offset, |ui| {
                    ui.allocate_space(vec2(30.0, 40.0));
                });
                sizes.push(response.content_size);
            });
        }

        assert_eq!(sizes, vec![vec2(30.0, 40.0), vec2(30.0, 40.0)]);
    }

    #[test]
    fn pane_clips_hit_testing_to_its_viewport() {
        let mut context = context();
        let mut pressed = None;

        let input = RawInput {
            pointer: Some(pos2(1.0, 95.0)),
            primary_down: true,
            primary_pressed: true,
            ..Default::default()
        };
        let _ = context.run(vec2(200.0, 200.0), 1.0, input, |ui| {
            ui.place(pos2(0.0, 100.0), |ui| {
                ui.pane(vec2(100.0, 50.0), vec2(0.0, -20.0), |ui| {
                    pressed = Some(ui.button("go").pressed);
                });
            });
        });

        // The button spans y=80..108, so the pointer is over it, but the pane starts at y=100 and
        // must clip hit testing to its viewport.
        assert_eq!(pressed, Some(false));
    }

    #[test]
    fn place_inside_a_pane_uses_the_offset_content_origin_and_stays_out_of_flow() {
        let mut context = context();
        let mut placed = Rect::NOTHING;
        let mut next = Rect::NOTHING;

        let _ = context.run(vec2(200.0, 200.0), 1.0, RawInput::default(), |ui| {
            ui.pane(vec2(100.0, 100.0), vec2(0.0, 40.0), |ui| {
                placed = ui.place(pos2(10.0, 20.0), |ui| ui.allocate_space(vec2(5.0, 5.0)));
                next = ui.allocate_space(vec2(5.0, 5.0));
            });
        });

        assert_eq!(placed.min, pos2(10.0, 60.0));
        assert_eq!(next.min, pos2(0.0, 40.0));
    }

    #[test]
    fn placing_a_pane_keeps_it_out_of_the_parent_flow() {
        let mut context = context();
        let spacing = context.style().spacing;
        let mut first = Rect::NOTHING;
        let mut next = Rect::NOTHING;

        let _ = context.run(vec2(200.0, 200.0), 1.0, RawInput::default(), |ui| {
            first = ui.allocate_space(vec2(10.0, 10.0));
            ui.place(pos2(50.0, 50.0), |ui| {
                ui.pane(vec2(30.0, 30.0), Vec2::ZERO, |ui| {
                    ui.allocate_space(vec2(10.0, 10.0));
                });
            });
            next = ui.allocate_space(vec2(10.0, 10.0));
        });

        assert_eq!(next.min.y, first.max.y + spacing);
    }

    #[test]
    fn scrollable_translates_content_and_reports_limits() {
        let mut context = context();
        let mut state = ScrollState {
            offset: vec2(10.0, 20.0),
            content_size: None,
        };
        let mut inside = Rect::NOTHING;
        let mut response = None;

        let _ = context.run(vec2(200.0, 200.0), 1.0, RawInput::default(), |ui| {
            let ((), result) = ui.scrollable(vec2(100.0, 100.0), &mut state, |ui| {
                inside = ui.allocate_space(vec2(200.0, 300.0));
            });
            response = Some(result);
        });

        let response = response.unwrap();
        assert_eq!(
            response.rect,
            Rect::from_min_size(pos2(0.0, 0.0), vec2(100.0, 100.0))
        );
        assert_eq!(response.content_size, vec2(200.0, 300.0));
        assert_eq!(response.max_offset, vec2(100.0, 200.0));
        assert_eq!(inside.min, pos2(-10.0, -20.0));
        assert_eq!(state.offset, vec2(10.0, 20.0));
        assert_eq!(state.content_size, Some(vec2(200.0, 300.0)));
    }

    #[test]
    fn scrollable_consumes_wheel_while_hovered() {
        let mut context = context();
        let mut state = ScrollState {
            offset: vec2(0.0, 50.0),
            content_size: Some(vec2(200.0, 300.0)),
        };
        let input = RawInput {
            pointer: Some(pos2(5.0, 5.0)),
            scroll: vec2(10.0, -20.0),
            ..Default::default()
        };

        let _ = context.run(vec2(200.0, 200.0), 1.0, input, |ui| {
            ui.scrollable(vec2(100.0, 100.0), &mut state, |ui| {
                ui.allocate_space(vec2(200.0, 300.0));
            });
        });

        // Wheel down (negative y) increases offset.y; wheel right increases offset.x.
        assert_eq!(state.offset, vec2(10.0, 70.0));
    }

    #[test]
    fn scrollable_ignores_wheel_outside_its_viewport() {
        let mut context = context();
        let mut state = ScrollState {
            offset: vec2(0.0, 50.0),
            content_size: Some(vec2(200.0, 300.0)),
        };
        let input = RawInput {
            pointer: Some(pos2(500.0, 500.0)),
            scroll: vec2(0.0, -20.0),
            ..Default::default()
        };

        let _ = context.run(vec2(200.0, 200.0), 1.0, input, |ui| {
            ui.scrollable(vec2(100.0, 100.0), &mut state, |ui| {
                ui.allocate_space(vec2(200.0, 300.0));
            });
        });

        assert_eq!(state.offset, vec2(0.0, 50.0));
    }

    #[test]
    fn scrollable_preclamps_wheel_with_previous_content_size() {
        let mut context = context();
        let mut state = ScrollState {
            offset: vec2(0.0, 190.0),
            content_size: Some(vec2(200.0, 300.0)),
        };
        let mut inside = Rect::NOTHING;
        let input = RawInput {
            pointer: Some(pos2(5.0, 5.0)),
            scroll: vec2(0.0, -20.0),
            ..Default::default()
        };

        let _ = context.run(vec2(200.0, 200.0), 1.0, input, |ui| {
            ui.scrollable(vec2(100.0, 100.0), &mut state, |ui| {
                inside = ui.allocate_space(vec2(200.0, 300.0));
            });
        });

        // The wheel asks for 210, but the previous content size clamps it to 200 before layout.
        assert_eq!(state.offset, vec2(0.0, 200.0));
        assert_eq!(inside.min, pos2(0.0, -200.0));
    }

    #[test]
    fn scrollable_clamps_first_frame_without_a_previous_content_size() {
        let mut context = context();
        let mut state = ScrollState {
            offset: vec2(1000.0, 1000.0),
            content_size: None,
        };
        let mut inside = Rect::NOTHING;

        let _ = context.run(vec2(200.0, 200.0), 1.0, RawInput::default(), |ui| {
            ui.scrollable(vec2(100.0, 100.0), &mut state, |ui| {
                inside = ui.allocate_space(vec2(200.0, 300.0));
            });
        });

        // Without a previous measurement the content is laid out at the old position, then clamped.
        assert_eq!(state.offset, vec2(100.0, 200.0));
        assert_eq!(inside.min, pos2(-1000.0, -1000.0));
        assert_eq!(state.content_size, Some(vec2(200.0, 300.0)));
    }

    #[test]
    fn scrollable_still_clamps_after_content_shrinks() {
        let mut context = context();
        let mut state = ScrollState {
            offset: vec2(100.0, 200.0),
            content_size: Some(vec2(200.0, 300.0)),
        };

        let _ = context.run(vec2(200.0, 200.0), 1.0, RawInput::default(), |ui| {
            ui.scrollable(vec2(100.0, 100.0), &mut state, |ui| {
                ui.allocate_space(vec2(50.0, 50.0));
            });
        });

        // The previous max still allows the old offset for this frame; the fallback clamp fixes it.
        assert_eq!(state.offset, Vec2::ZERO);
        assert_eq!(state.content_size, Some(vec2(50.0, 50.0)));
    }

    #[test]
    fn scrollable_resets_scroll_when_content_fits() {
        let mut context = context();
        let mut state = ScrollState {
            offset: vec2(10.0, 10.0),
            content_size: None,
        };
        let mut max_offset = None;

        let _ = context.run(vec2(200.0, 200.0), 1.0, RawInput::default(), |ui| {
            let ((), response) = ui.scrollable(vec2(100.0, 100.0), &mut state, |ui| {
                ui.allocate_space(vec2(50.0, 50.0));
            });
            max_offset = Some(response.max_offset);
        });

        assert_eq!(max_offset, Some(Vec2::ZERO));
        assert_eq!(state.offset, Vec2::ZERO);
    }

    #[test]
    fn nested_scrollables_let_the_outer_one_consume_the_wheel() {
        let mut context = context();
        let mut outer = ScrollState::default();
        let mut inner = ScrollState::default();
        let input = RawInput {
            pointer: Some(pos2(5.0, 5.0)),
            scroll: vec2(0.0, -20.0),
            ..Default::default()
        };

        let _ = context.run(vec2(200.0, 200.0), 1.0, input, |ui| {
            ui.scrollable(vec2(100.0, 100.0), &mut outer, |ui| {
                ui.scrollable(vec2(50.0, 50.0), &mut inner, |ui| {
                    ui.allocate_space(vec2(10.0, 200.0));
                });
                ui.allocate_space(vec2(10.0, 200.0));
            });
        });

        assert_eq!(outer.offset, vec2(0.0, 20.0));
        assert_eq!(inner.offset, Vec2::ZERO);
    }
}
