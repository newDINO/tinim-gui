//! Shared helpers for unit tests.

use crate::{Context, TextureId};

pub(super) fn context() -> Context {
    let mut context = Context::new(TextureId(1));
    context
        .text_system_mut()
        .add_font(font_test_data::AHEM.to_vec())
        .unwrap();
    context
}
