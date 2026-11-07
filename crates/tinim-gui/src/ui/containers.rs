//! Layout containers: stacks and placed regions.

use tinim_paint::{Pos2, Rect, Vec2};

use super::Ui;
use super::layout::{Direction, allocate_space};
use super::response::PaneResponse;

impl Ui<'_> {
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
        let (start, max) = {
            let parent = self.regions.last().expect("root region");
            (parent.origin + pos, parent.rect.max)
        };
        self.region_at(
            Rect::from_min_max(start, max),
            start,
            Direction::Vertical,
            build,
        )
        .0
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
        let origin = rect.min + offset;

        let (result, used) = self.region_at(rect, origin, Direction::Vertical, build);

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
    use crate::{RawInput, pos2, vec2};

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
    fn region_clips_painting_and_restores_the_parent_clip() {
        let mut context = context();
        let frame = context.run(vec2(200.0, 200.0), 1.0, RawInput::default(), |ui| {
            ui.place(pos2(50.0, 60.0), |ui| {
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
            ui.place(pos2(50.0, 50.0), |ui| {
                ui.pane(vec2(30.0, 30.0), Vec2::ZERO, |ui| {
                    ui.allocate_space(vec2(10.0, 10.0));
                });
            });
            next = ui.allocate_space(vec2(10.0, 10.0));
        });

        assert_eq!(next.min.y, first.max.y + spacing);
    }
}
