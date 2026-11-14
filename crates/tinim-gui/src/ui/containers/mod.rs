//! Layout containers: stacks, placed regions, panes and scrollable viewports.

mod frame;
mod scroll;

pub use frame::FrameResponse;
pub use scroll::{ScrollResponse, ScrollState};

use tinim_paint::{Pos2, Rect, Vec2};

use super::Ui;
use super::layout::{FlowConfig, allocate_space};
use crate::style::Edges;

/// The result of building a [`Ui::pane`](crate::Ui::pane).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PaneResponse {
    /// The viewport rectangle allocated in the parent flow.
    pub rect: Rect,
    /// Extent of the in-flow content, independent of the pane's offset.
    ///
    /// Out-of-flow children such as [`Ui::place_at`](crate::Ui::place_at) and
    /// [`Ui::place_relative`](crate::Ui::place_relative) are not included.
    pub content_size: Vec2,
}

impl Ui<'_> {
    /// Stacks children vertically.
    ///
    /// This changes layout only. Painting and hit testing stay clipped to the enclosing viewport,
    /// if any; the stack itself is not a clipping region.
    ///
    /// The stack inherits the enclosing region's width. Use [`Ui::flow`] with a `cross_size` to
    /// constrain it.
    pub fn vertical<R>(&mut self, build: impl FnOnce(&mut Ui<'_>) -> R) -> R {
        self.flow(FlowConfig::vertical(), None, build)
    }

    /// Stacks children horizontally.
    ///
    /// This changes layout only. Painting and hit testing stay clipped to the enclosing viewport,
    /// if any; the stack itself is not a clipping region.
    ///
    /// The stack inherits the enclosing region's height. Use [`Ui::flow`] with a `cross_size` to
    /// constrain it.
    pub fn horizontal<R>(&mut self, build: impl FnOnce(&mut Ui<'_>) -> R) -> R {
        self.flow(FlowConfig::horizontal(), None, build)
    }

    /// Builds a content-sized flow with an explicit direction and cross-axis alignment.
    ///
    /// `cross_size` fixes the extent of the cross axis: the width of a vertical flow or the height
    /// of a horizontal flow. `None` inherits the enclosing region's cross-axis extent. Children see
    /// the extent through `Ui::size` and `Ui::available_size`, and cross-axis alignment uses it.
    /// The parent flow reserves the cross-axis extent even when the children are narrower, so a
    /// vertical flow with `Some(width)` acts as a fixed-width column and a horizontal flow with
    /// `Some(height)` acts as a fixed-height row. Children are not clipped and may still overflow
    /// the extent. Non-finite values are treated as zero.
    ///
    /// Only forward directions ([`FlowConfig::vertical`] and [`FlowConfig::horizontal`]) are
    /// supported. Reverse flows need a known extent; use [`Ui::sized_flow`] for those.
    ///
    /// A content-sized flow must also be nested in a forward flow. Inside a reverse
    /// [`Ui::sized_flow`], wrap it in another `sized_flow`.
    ///
    /// Like [`Ui::vertical`] and [`Ui::horizontal`], this changes layout only and does not clip.
    pub fn flow<R>(
        &mut self,
        config: FlowConfig,
        cross_size: Option<f32>,
        build: impl FnOnce(&mut Ui<'_>) -> R,
    ) -> R {
        debug_assert!(
            config.direction.is_forward(),
            "content-sized flow requires a forward direction; use sized_flow for reverse directions"
        );
        debug_assert!(
            self.layout.direction.is_forward(),
            "content-sized flow inside a reverse flow is unsupported; wrap it in sized_flow"
        );

        self.content_region(Edges::ZERO, cross_size, config, build)
            .0
    }

    /// Builds content in a fixed-size layout box.
    ///
    /// The box is allocated in the parent flow. Unlike [`Ui::pane`], it does not clip. All flow
    /// directions are supported, including bottom-to-top and right-to-left.
    ///
    /// A content-sized flow such as [`Ui::vertical`] cannot be nested directly inside a reverse
    /// `sized_flow`; wrap it in another `sized_flow` or use leaf widgets.
    pub fn sized_flow<R>(
        &mut self,
        size: Vec2,
        config: FlowConfig,
        build: impl FnOnce(&mut Ui<'_>) -> R,
    ) -> R {
        let rect = self.allocate_space(size);
        self.layout_at(rect, rect.min, config, build).0
    }

    /// Starts a vertical region at an absolute position.
    ///
    /// `pos` is in absolute logical coordinates, the same space used by rectangles returned from
    /// widgets and by [`Ui::origin`]. Use [`Ui::place_relative`] to offset from the current
    /// region's content origin instead.
    ///
    /// The region extends to the current region's bottom-right corner. It is out of flow: the
    /// parent cursor does not advance and the region does not contribute to the parent's used
    /// rectangle.
    pub fn place_at<R>(&mut self, pos: Pos2, build: impl FnOnce(&mut Ui<'_>) -> R) -> R {
        let rect = Rect::from_min_max(pos, self.layout.rect.max);
        self.clipped_layout_at(rect, pos, FlowConfig::vertical(), build)
            .0
    }

    /// Starts a vertical region at `offset` from the current region's content origin.
    ///
    /// For ordinary regions the content origin is the region's top-left corner. Inside a
    /// [`Ui::pane`] it is shifted by the pane's offset, so placed content scrolls with the pane
    /// content. The placed region's absolute origin is available through [`Ui::origin`].
    ///
    /// The region extends to the current region's bottom-right corner. It is out of flow: the
    /// parent cursor does not advance and the region does not contribute to the parent's used
    /// rectangle.
    pub fn place_relative<R>(&mut self, offset: Vec2, build: impl FnOnce(&mut Ui<'_>) -> R) -> R {
        let pos = self.layout.origin + offset;
        self.place_at(pos, build)
    }

    /// Builds content in an in-flow viewport of a fixed `size`.
    ///
    /// `offset` translates the content: the content origin is `rect.min + offset`, so a positive
    /// offset moves content right and down. Content is clipped to the viewport, and the parent
    /// flow advances by `size` rather than by the content's measured size.
    ///
    /// Returns the closure's result, the viewport rectangle and the measured extent of the
    /// in-flow content. The extent does not depend on `offset`. Out-of-flow children such as
    /// [`Ui::place_at`] and [`Ui::place_relative`] are not included.
    ///
    /// To create an out-of-flow pane, wrap it in [`Ui::place_at`] or [`Ui::place_relative`].
    pub fn pane<R>(
        &mut self,
        size: Vec2,
        offset: Vec2,
        build: impl FnOnce(&mut Ui<'_>) -> R,
    ) -> (R, PaneResponse) {
        let spacing = self.spacing();
        let rect = allocate_space(&mut self.layout, size, spacing);
        let origin = rect.min + offset;

        let (result, used) = self.clipped_layout_at(rect, origin, FlowConfig::vertical(), build);

        let content_size = if used.is_positive() {
            (used.max - origin).max(Vec2::ZERO)
        } else {
            Vec2::ZERO
        };

        (result, PaneResponse { rect, content_size })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::test_util::context;
    use crate::{Align, RawInput, pos2, vec2};

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
    fn sized_flow_bottom_to_top_places_children_from_the_bottom() {
        let mut context = context();
        let spacing = context.style().spacing;
        let mut first = Rect::NOTHING;
        let mut second = Rect::NOTHING;
        let mut after = Rect::NOTHING;

        let _ = context.run(vec2(200.0, 200.0), 1.0, RawInput::default(), |ui| {
            ui.sized_flow(vec2(100.0, 80.0), FlowConfig::bottom_to_top(), |ui| {
                first = ui.allocate_space(vec2(10.0, 10.0));
                second = ui.allocate_space(vec2(10.0, 10.0));
            });
            after = ui.allocate_space(vec2(10.0, 10.0));
        });

        assert_eq!(first.min.x, 0.0);
        assert_eq!(first.max.y, 80.0);
        assert_eq!(second.max.y, first.min.y - spacing);
        assert_eq!(second.min.y, first.min.y - spacing - 10.0);
        // The parent flow advances by the box size, not by the content's measured extent.
        assert_eq!(after.min.y, 80.0 + spacing);
    }

    #[test]
    fn sized_flow_right_to_left_places_children_from_the_right() {
        let mut context = context();
        let spacing = context.style().spacing;
        let mut first = Rect::NOTHING;
        let mut second = Rect::NOTHING;

        let _ = context.run(vec2(200.0, 200.0), 1.0, RawInput::default(), |ui| {
            ui.sized_flow(vec2(100.0, 80.0), FlowConfig::right_to_left(), |ui| {
                first = ui.allocate_space(vec2(10.0, 10.0));
                second = ui.allocate_space(vec2(10.0, 10.0));
            });
        });

        assert_eq!(first.min.y, 0.0);
        assert_eq!(first.max.x, 100.0);
        assert_eq!(second.max.x, first.min.x - spacing);
        assert_eq!(second.min.x, first.min.x - spacing - 10.0);
    }

    #[test]
    fn sized_flow_inside_forward_flow_keeps_parent_flow() {
        let mut context = context();
        let spacing = context.style().spacing;
        let mut before = Rect::NOTHING;
        let mut inside = Rect::NOTHING;
        let mut after = Rect::NOTHING;

        let _ = context.run(vec2(200.0, 200.0), 1.0, RawInput::default(), |ui| {
            before = ui.allocate_space(vec2(10.0, 10.0));
            ui.sized_flow(vec2(50.0, 40.0), FlowConfig::bottom_to_top(), |ui| {
                inside = ui.allocate_space(vec2(10.0, 10.0));
            });
            after = ui.allocate_space(vec2(10.0, 10.0));
        });

        let box_max_y = before.max.y + spacing + 40.0;
        assert_eq!(inside.max.y, box_max_y);
        assert_eq!(after.min.y, box_max_y + spacing);
    }

    #[test]
    fn nested_sized_flows_support_reverse_directions() {
        let mut context = context();
        let mut inside = Rect::NOTHING;

        let _ = context.run(vec2(200.0, 200.0), 1.0, RawInput::default(), |ui| {
            ui.sized_flow(vec2(100.0, 100.0), FlowConfig::bottom_to_top(), |ui| {
                ui.sized_flow(vec2(20.0, 20.0), FlowConfig::right_to_left(), |ui| {
                    inside = ui.allocate_space(vec2(5.0, 5.0));
                });
            });
        });

        // The outer reverse vertical flow puts the 20x20 box at the bottom-left; the inner reverse
        // horizontal flow puts the item at that box's right edge.
        assert_eq!(inside.max.x, 20.0);
        assert_eq!(inside.min.y, 80.0);
        assert_eq!(inside.max.y, 85.0);
    }

    #[test]
    fn flow_align_centers_children_on_the_cross_axis() {
        let mut context = context();
        let mut rect = Rect::NOTHING;

        let _ = context.run(vec2(100.0, 100.0), 1.0, RawInput::default(), |ui| {
            ui.flow(FlowConfig::vertical().align(Align::Center), None, |ui| {
                rect = ui.allocate_space(vec2(10.0, 10.0));
            });
        });

        assert_eq!(rect.min.x, 45.0);
        assert_eq!(rect.max.x, 55.0);
    }

    #[test]
    fn horizontal_flow_align_centers_children_on_the_cross_axis() {
        let mut context = context();
        let mut rect = Rect::NOTHING;

        let _ = context.run(vec2(100.0, 100.0), 1.0, RawInput::default(), |ui| {
            ui.flow(FlowConfig::horizontal().align(Align::Center), None, |ui| {
                rect = ui.allocate_space(vec2(10.0, 10.0));
            });
        });

        assert_eq!(rect.min.y, 45.0);
        assert_eq!(rect.max.y, 55.0);
    }

    #[test]
    fn flow_align_end_and_stretch() {
        let mut context = context();
        let mut end = Rect::NOTHING;
        let mut stretched = Rect::NOTHING;

        let _ = context.run(vec2(100.0, 100.0), 1.0, RawInput::default(), |ui| {
            ui.flow(FlowConfig::vertical().align(Align::End), None, |ui| {
                end = ui.allocate_space(vec2(10.0, 10.0));
            });
            ui.flow(FlowConfig::vertical().align(Align::Stretch), None, |ui| {
                stretched = ui.allocate_space(vec2(10.0, 10.0));
            });
        });

        assert_eq!(end.max.x, 100.0);
        assert_eq!(stretched.min.x, 0.0);
        assert_eq!(stretched.width(), 100.0);
    }

    #[test]
    fn flow_cross_size_constrains_the_width_of_a_vertical_flow() {
        let mut context = context();
        let spacing = context.style().spacing;
        let mut region = Vec2::ZERO;
        let mut stretched = Rect::NOTHING;
        let mut after = Rect::NOTHING;

        let _ = context.run(vec2(200.0, 200.0), 1.0, RawInput::default(), |ui| {
            ui.flow(
                FlowConfig::vertical().align(Align::Stretch),
                Some(60.0),
                |ui| {
                    region = ui.size();
                    stretched = ui.allocate_space(vec2(10.0, 10.0));
                },
            );
            after = ui.allocate_space(vec2(10.0, 10.0));
        });

        assert_eq!(region, vec2(60.0, 200.0));
        assert_eq!(stretched.width(), 60.0);
        // The parent still advances by the used height, not by the cross-axis width.
        assert_eq!(after.min.y, stretched.max.y + spacing);
        assert_eq!(after.min.x, 0.0);
    }

    #[test]
    fn flow_cross_size_constrains_the_height_of_a_horizontal_flow() {
        let mut context = context();
        let mut region = Vec2::ZERO;
        let mut first = Rect::NOTHING;

        let _ = context.run(vec2(200.0, 200.0), 1.0, RawInput::default(), |ui| {
            ui.flow(FlowConfig::horizontal(), Some(50.0), |ui| {
                region = ui.size();
                first = ui.allocate_space(vec2(10.0, 10.0));
            });
        });

        assert_eq!(region, vec2(200.0, 50.0));
        assert_eq!(first.min, pos2(0.0, 0.0));
    }

    #[test]
    fn flow_cross_size_reserves_a_fixed_width_column() {
        let mut context = context();
        let spacing = context.style().spacing;
        let mut first = Rect::NOTHING;
        let mut next = Rect::NOTHING;

        let _ = context.run(vec2(400.0, 200.0), 1.0, RawInput::default(), |ui| {
            ui.horizontal(|ui| {
                ui.flow(FlowConfig::vertical(), Some(120.0), |ui| {
                    first = ui.allocate_space(vec2(10.0, 10.0));
                });
                next = ui.allocate_space(vec2(10.0, 10.0));
            });
        });

        assert_eq!(first.min.x, 0.0);
        assert_eq!(next.min.x, first.min.x + 120.0 + spacing);
    }

    #[test]
    fn flow_cross_size_reserves_a_fixed_height_row() {
        let mut context = context();
        let spacing = context.style().spacing;
        let mut first = Rect::NOTHING;
        let mut next = Rect::NOTHING;

        let _ = context.run(vec2(200.0, 400.0), 1.0, RawInput::default(), |ui| {
            ui.vertical(|ui| {
                ui.flow(FlowConfig::horizontal(), Some(40.0), |ui| {
                    first = ui.allocate_space(vec2(10.0, 10.0));
                });
                next = ui.allocate_space(vec2(10.0, 10.0));
            });
        });

        assert_eq!(first.min.y, 0.0);
        assert_eq!(next.min.y, first.min.y + 40.0 + spacing);
    }

    #[test]
    fn sized_flow_does_not_narrow_the_clip() {
        let mut context = context();
        let frame = context.run(vec2(200.0, 200.0), 1.0, RawInput::default(), |ui| {
            ui.sized_flow(vec2(100.0, 50.0), FlowConfig::vertical(), |ui| {
                ui.label("inside");
            });
        });

        let root_clip = Rect::from_min_max(pos2(0.0, 0.0), pos2(200.0, 200.0));
        assert!(!frame.paint.draws.is_empty());
        assert!(
            frame
                .paint
                .draws
                .iter()
                .all(|draw| draw.clip_rect == root_clip)
        );
    }

    #[test]
    fn flow_containers_do_not_narrow_the_clip() {
        let mut context = context();
        let frame = context.run(vec2(200.0, 200.0), 1.0, RawInput::default(), |ui| {
            ui.pane(vec2(100.0, 50.0), Vec2::ZERO, |ui| {
                ui.label("first");
                // The cursor has advanced, so this stack covers less than the pane. It must still
                // paint with the pane's clip rather than introducing a narrower one.
                ui.vertical(|ui| {
                    ui.label("second");
                });
            });
            ui.label("after");
        });

        let pane_clip = Rect::from_min_size(pos2(0.0, 0.0), vec2(100.0, 50.0));
        let root_clip = Rect::from_min_max(pos2(0.0, 0.0), pos2(200.0, 200.0));

        // Without the split, the stack would contribute a third clip rectangle between `pane_clip`
        // and `root_clip`.
        assert!(!frame.paint.draws.is_empty());
        assert!(
            frame
                .paint
                .draws
                .iter()
                .all(|draw| draw.clip_rect == pane_clip || draw.clip_rect == root_clip)
        );
        assert!(
            frame
                .paint
                .draws
                .iter()
                .any(|draw| draw.clip_rect == pane_clip)
        );
        assert!(
            frame
                .paint
                .draws
                .iter()
                .any(|draw| draw.clip_rect == root_clip)
        );
    }

    #[test]
    fn place_relative_starts_a_region_at_a_position_relative_to_the_region_origin() {
        let mut context = context();
        let spacing = context.style().spacing;
        let mut first = Rect::NOTHING;
        let mut placed = Rect::NOTHING;
        let mut nested = Rect::NOTHING;
        let mut next = Rect::NOTHING;

        let _ = context.run(vec2(400.0, 300.0), 1.0, RawInput::default(), |ui| {
            first = ui.label("first");
            placed = ui.place_relative(vec2(120.0, 80.0), |ui| ui.label("floating"));
            ui.place_relative(vec2(200.0, 100.0), |ui| {
                nested = ui.place_relative(vec2(5.0, 6.0), |ui| ui.label("nested"));
            });
            next = ui.label("next");
        });

        assert_eq!(placed.min, pos2(120.0, 80.0));
        assert_eq!(nested.min, pos2(205.0, 106.0));
        assert_eq!(next.min.x, first.min.x);
        assert_eq!(next.min.y, first.max.y + spacing);
    }

    #[test]
    fn place_at_uses_absolute_coordinates() {
        let mut context = context();
        let mut outer_origin = None;
        let mut inner = Rect::NOTHING;

        let _ = context.run(vec2(400.0, 300.0), 1.0, RawInput::default(), |ui| {
            ui.place_at(pos2(100.0, 50.0), |ui| {
                outer_origin = Some(ui.origin());
                inner = ui.place_at(pos2(120.0, 80.0), |ui| ui.allocate_space(vec2(5.0, 5.0)));
            });
        });

        assert_eq!(outer_origin, Some(pos2(100.0, 50.0)));
        assert_eq!(inner.min, pos2(120.0, 80.0));
    }

    #[test]
    fn ui_origin_is_absolute_in_placed_and_panned_regions() {
        let mut context = context();
        let mut root = None;
        let mut placed = None;
        let mut nested = None;
        let mut panned = None;
        let mut pane_rect = Rect::NOTHING;

        let _ = context.run(vec2(300.0, 200.0), 1.0, RawInput::default(), |ui| {
            root = Some(ui.origin());
            ui.place_at(pos2(40.0, 30.0), |ui| {
                placed = Some(ui.origin());
                ui.place_relative(vec2(5.0, 6.0), |ui| {
                    nested = Some(ui.origin());
                });
            });

            let ((), response) = ui.pane(vec2(100.0, 50.0), vec2(7.0, 9.0), |ui| {
                panned = Some(ui.origin());
            });
            pane_rect = response.rect;
        });

        assert_eq!(root, Some(pos2(0.0, 0.0)));
        assert_eq!(placed, Some(pos2(40.0, 30.0)));
        assert_eq!(nested, Some(pos2(45.0, 36.0)));
        assert_eq!(panned, Some(pane_rect.min + vec2(7.0, 9.0)));
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
            ui.place_relative(vec2(0.0, 100.0), |ui| {
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
                placed =
                    ui.place_relative(vec2(10.0, 20.0), |ui| ui.allocate_space(vec2(5.0, 5.0)));
                next = ui.allocate_space(vec2(5.0, 5.0));
            });
        });

        assert_eq!(placed.min, pos2(10.0, 60.0));
        assert_eq!(next.min, pos2(0.0, 40.0));
    }

    #[test]
    fn region_clips_painting_and_restores_the_parent_clip() {
        let mut context = context();
        let frame = context.run(vec2(200.0, 200.0), 1.0, RawInput::default(), |ui| {
            ui.place_relative(vec2(50.0, 60.0), |ui| {
                ui.label("inside");
            });
            ui.label("after");
        });

        let placed_clip = Rect::from_min_max(pos2(50.0, 60.0), pos2(200.0, 200.0));
        let root_clip = Rect::from_min_max(pos2(0.0, 0.0), pos2(200.0, 200.0));

        assert!(frame.paint.draws.len() >= 2);
        assert_eq!(frame.paint.draws[0].clip_rect, placed_clip);
        assert_eq!(frame.paint.draws.last().unwrap().clip_rect, root_clip);
    }

    #[test]
    fn placing_a_pane_keeps_it_out_of_the_parent_flow() {
        let mut context = context();
        let spacing = context.style().spacing;
        let mut first = Rect::NOTHING;
        let mut next = Rect::NOTHING;

        let _ = context.run(vec2(200.0, 200.0), 1.0, RawInput::default(), |ui| {
            first = ui.allocate_space(vec2(10.0, 10.0));
            ui.place_relative(vec2(50.0, 50.0), |ui| {
                ui.pane(vec2(30.0, 30.0), Vec2::ZERO, |ui| {
                    ui.allocate_space(vec2(10.0, 10.0));
                });
            });
            next = ui.allocate_space(vec2(10.0, 10.0));
        });

        assert_eq!(next.min.y, first.max.y + spacing);
    }
}
