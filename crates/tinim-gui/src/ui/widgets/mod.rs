//! Leaf widgets: labels, buttons, toggles, sliders, number inputs and text editing.

use tinim_paint::{Color32, Painter, Pos2, Rect};
use tinim_paint_text::TextSystem;
use tinim_text::TextLayout;

use super::Ui;
use super::layout::{allocate_space, available_size, pointer_over};
use crate::input::RawInput;

mod button;
mod number_input;
mod slider;
mod text_edit;
mod toggle;

pub use number_input::{NumberInputResponse, NumberInputState};
pub use text_edit::{TextEditResponse, TextEditState};

/// Pointer state shared by interactive widgets.
#[derive(Clone, Copy, Debug)]
pub(super) struct Interaction {
    pub hovered: bool,
    pub pressed: bool,
    pub down: bool,
}

/// Hit-tests `rect` once and derives the press edges every interactive widget reports.
pub(super) fn interaction(clip: Rect, input: &RawInput, rect: Rect) -> Interaction {
    let hovered = pointer_over(clip, input, rect);
    Interaction {
        hovered,
        pressed: hovered && input.primary_pressed,
        down: hovered && input.primary_down,
    }
}

/// Whether `rect` has any visible area after the painter's current clip is applied.
pub(super) fn is_visible(painter: &Painter, rect: Rect) -> bool {
    painter.clip_rect().intersect(rect).is_positive()
}

impl Ui<'_> {
    /// Draws a non-interactive line of text. Returns the rectangle it occupied.
    pub fn label(&mut self, text: &str) -> Rect {
        let font = self.ctx.style().font.clone();
        let text_color = self.ctx.style().text_color;
        let spacing = self.spacing();
        let max_width = Some(available_size(&self.layout, spacing).x);
        // Borrow only the fields involved. `layout` keeps `text_cache` borrowed, so everything
        // that runs while it is alive must use disjoint field borrows instead of `&mut self.ctx`.
        let layout = self
            .ctx
            .text_cache
            .layout(&self.ctx.text_system, text, &font, max_width);
        let size = layout.size();
        let rect = allocate_space(&mut self.layout, size, spacing);

        paint_text(
            &mut self.ctx.text_system,
            &mut self.ctx.painter,
            self.ctx.pixels_per_point,
            layout,
            rect.min,
            text_color,
        );
        rect
    }
}

fn paint_text(
    text: &mut TextSystem,
    painter: &mut Painter,
    pixels_per_point: f32,
    layout: &TextLayout,
    origin: Pos2,
    color: Color32,
) {
    if !painter.clip_rect().is_positive() {
        return;
    }
    let mesh = text.prepare(layout, origin, color, pixels_per_point);
    painter.add_mesh(mesh);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::test_util::context;
    use crate::{RawInput, pos2, vec2};

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
}
