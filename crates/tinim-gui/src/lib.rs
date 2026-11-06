//! A tiny immediate-mode GUI built on [`tinim_paint`] and [`tinim_text`].
//!
//! v0.1 is intentionally small: a [`Context`] owns the painter and text system, [`Ui`] stacks
//! labels and buttons vertically or horizontally, and [`Context::run`] returns a [`Frame`] with a
//! paint list and glyph texture updates.
//!
//! There are no widget ids and no persistent widget state. A button only reports a press edge;
//! click/press-release pairing is left for a future version.

mod context;
mod input;
mod style;
mod text_cache;
mod ui;

pub use tinim_paint as paint;
pub use tinim_paint::{Color32, PaintList, Pos2, Rect, TextureId, TexturesDelta, Vec2, pos2, vec2};
pub use tinim_text as text;
pub use tinim_text::{TextLayout, TextStyle, TextSystem};

pub use context::{Context, Frame};
pub use input::{Modifiers, RawInput};
pub use style::{ButtonStyle, Style};
pub use ui::{PaneResponse, Response, Ui};
