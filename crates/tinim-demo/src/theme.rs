//! Demo theme: the widget style plus the window background and page frames.
//!
//! [`Style`] only describes widgets. The demo also needs a window background and frames around
//! pages and nested panels, so those live here and switch together with the style preset.

use tinim_gui::{Color32, Edges, FrameStyle, Stroke, Style, TextEditStyle};

const DEFAULT_TEXT: Color32 = Color32::from_rgb(230, 230, 230);
pub(crate) const WARM_TEXT: Color32 = Color32::from_rgb(255, 196, 120);
pub(crate) const COOL_TEXT: Color32 = Color32::from_rgb(180, 210, 255);

/// A complete visual preset for the demo.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Theme {
    /// The dark theme the demo has always used.
    #[default]
    Default,
    /// Light, browser-like widgets with square corners and a sans-serif font.
    Plain,
}

impl Theme {
    /// The widget style for this theme.
    pub(crate) fn style(self) -> Style {
        match self {
            Self::Default => Style {
                text_color: DEFAULT_TEXT,
                text_edit: TextEditStyle {
                    text: DEFAULT_TEXT,
                    ..TextEditStyle::default()
                },
                ..Style::default()
            },
            Self::Plain => Style::plain(),
        }
    }

    /// The color behind everything, painted as a full-viewport frame.
    pub(crate) fn background(self) -> Color32 {
        match self {
            Self::Default => Color32::from_rgb(18, 18, 23),
            Self::Plain => Color32::WHITE,
        }
    }

    /// The default label color for this theme.
    pub(crate) fn text_color(self) -> Color32 {
        match self {
            Self::Default => DEFAULT_TEXT,
            Self::Plain => Color32::BLACK,
        }
    }

    /// The frame around a page.
    pub(crate) fn page_frame(self) -> FrameStyle {
        match self {
            Self::Default => FrameStyle {
                background: Color32::from_rgb(38, 38, 44),
                border: Stroke::new(1.0, Color32::from_rgb(82, 82, 94)),
                padding: Edges::all(8.0),
                corner_radius: 4,
            },
            Self::Plain => FrameStyle {
                background: Color32::WHITE,
                border: Stroke::new(1.0, Color32::from_rgb(190, 190, 190)),
                padding: Edges::all(8.0),
                corner_radius: 0,
            },
        }
    }

    /// The frame around nested demo content.
    pub(crate) fn panel_frame(self) -> FrameStyle {
        let mut frame = self.page_frame();
        frame.background = match self {
            Self::Default => Color32::from_rgb(53, 53, 61),
            Self::Plain => Color32::from_rgb(248, 248, 248),
        };
        frame
    }
}
