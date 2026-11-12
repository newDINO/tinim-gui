//! Demo pages.

mod layout;
mod pane;
mod placed;
mod scroll;
mod style;
mod widgets;

use tinim_gui::{Color32, Edges, FrameStyle, Stroke};

pub(crate) use layout::LayoutPage;
pub(crate) use pane::PanePage;
pub(crate) use placed::draw as draw_placed_page;
pub(crate) use scroll::ScrollPage;
pub(crate) use style::StylePage;
pub(crate) use widgets::WidgetsPage;

const FRAME_STYLE1: FrameStyle = FrameStyle {
    background: Color32::from_rgb(38, 38, 44),
    border: Stroke::new(1.0, Color32::from_rgb(82, 82, 94)),
    padding: Edges::all(8.0),
    corner_radius: 4,
};

const FRMAE_STYLE2: FrameStyle = const {
    let mut style = FRAME_STYLE1;
    style.background = Color32::from_rgb(53, 53, 61);
    style
};
