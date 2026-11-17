//! Demo pages.

mod layout;
mod pane;
mod placed;
mod scroll;
mod style;
mod widgets;

use tinim_gui::{Color32, Edges, FrameStyle, ScrollBars, ScrollState, Stroke, Ui};

pub(crate) use layout::LayoutPage;
pub(crate) use pane::PanePage;
pub(crate) use placed::PlacedPage;
pub(crate) use scroll::ScrollPage;
pub(crate) use style::StylePage;
pub(crate) use widgets::WidgetsPage;

const FRAME_STYLE1: FrameStyle = FrameStyle {
    background: Color32::from_rgb(38, 38, 44),
    border: Stroke::new(1.0, Color32::from_rgb(82, 82, 94)),
    padding: Edges::all(8.0),
    corner_radius: 4,
};

const FRAME_STYLE2: FrameStyle = const {
    let mut style = FRAME_STYLE1;
    style.background = Color32::from_rgb(53, 53, 61);
    style
};

/// Builds a page's content in a frame that fills the remaining viewport.
///
/// The frame shows the page bounds and the scrollable keeps content reachable when the window is
/// smaller than the page. Each page owns its [`ScrollState`], so switching pages keeps their
/// scroll positions.
pub(crate) fn page<R>(
    ui: &mut Ui<'_>,
    scroll: &mut ScrollState,
    build: impl FnOnce(&mut Ui<'_>) -> R,
) -> R {
    let size = ui.available_size();
    let (result, _) = ui.sized_frame(size, FRAME_STYLE1, |ui| {
        let size = ui.available_size();
        ui.scrollable_with_bars(size, ScrollBars::BOTH, scroll, build)
            .0
    });
    result
}
