//! Demo pages.

mod image;
mod layout;
mod pane;
mod placed;
mod scroll;
mod style;
mod widgets;

use tinim_gui::{ScrollBars, ScrollState, Ui};

pub(crate) use image::ImagePage;
pub(crate) use layout::LayoutPage;
pub(crate) use pane::PanePage;
pub(crate) use placed::PlacedPage;
pub(crate) use scroll::ScrollPage;
pub(crate) use style::StylePage;
pub(crate) use widgets::WidgetsPage;

use crate::Theme;

/// Builds a page's content in a frame that fills the remaining viewport.
///
/// The frame shows the page bounds and the scrollable keeps content reachable when the window is
/// smaller than the page. Each page owns its [`ScrollState`], so switching pages keeps their
/// scroll positions.
pub(crate) fn page<R>(
    ui: &mut Ui<'_>,
    scroll: &mut ScrollState,
    theme: Theme,
    build: impl FnOnce(&mut Ui<'_>) -> R,
) -> R {
    let size = ui.available_size();
    let (result, _) = ui.sized_frame(size, theme.page_frame(), |ui| {
        let size = ui.available_size();
        ui.scrollable_with_bars(size, ScrollBars::BOTH, scroll, build)
            .0
    });
    result
}
