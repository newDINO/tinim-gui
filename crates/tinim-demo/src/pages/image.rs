//! Texture and image-widget showcase.

use tinim_gui::{
    Color32, Context, ImageData, ImageFit, ImageOptions, ScrollState, TextureFilter, TextureHandle,
    TextureOptions, Ui, vec2,
};

use super::page;
use crate::Theme;

const SIZE: u32 = 64;

pub(crate) struct ImagePage {
    scroll: ScrollState,
    linear: TextureHandle,
    nearest: TextureHandle,
    fit: ImageFit,
    tint: bool,
}

impl ImagePage {
    pub(crate) fn new(context: &mut Context) -> Self {
        let image = checkerboard();

        // The same pixels with two different per-texture samplers. The nearest version is useful
        // for pixel art; the linear version smooths the checkerboard when it is scaled.
        let linear = context.load_texture(image.clone());
        let nearest = context.load_texture_with(
            image,
            TextureOptions {
                mag_filter: TextureFilter::Nearest,
                min_filter: TextureFilter::Nearest,
                ..Default::default()
            },
        );

        Self {
            scroll: ScrollState::new(),
            linear,
            nearest,
            fit: ImageFit::Original,
            tint: false,
        }
    }

    pub(crate) fn draw(&mut self, ui: &mut Ui, theme: Theme) {
        let scroll = &mut self.scroll;
        let fit = &mut self.fit;
        let tint = &mut self.tint;
        let linear = self.linear;
        let nearest = self.nearest;

        page(ui, scroll, theme, |ui| {
            ui.label(
                "load_texture()/Ui::image() draw an RGBA8 image through the normal paint list.",
            );
            ui.label(
                "The natural size is the pixel size interpreted as logical points (like CSS).",
            );

            ui.horizontal(|ui| {
                ui.label("Fit:");
                for (value, label) in [
                    (ImageFit::Original, "Original"),
                    (ImageFit::Fill, "Fill"),
                    (ImageFit::Contain, "Contain"),
                    (ImageFit::Cover, "Cover"),
                ] {
                    if ui.selectable_label(*fit == value, label).pressed {
                        *fit = value;
                    }
                }
            });
            ui.checkbox(tint);
            ui.label("Tint halves the image's alpha (checkbox above).");

            let target = vec2(180.0, 120.0);
            let tint = if *tint {
                Color32::from_rgba_unmultiplied(255, 255, 255, 128)
            } else {
                Color32::WHITE
            };

            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label("linear");
                    ui.image_with(
                        &linear,
                        ImageOptions::default()
                            .with_size(target)
                            .with_fit(*fit)
                            .with_tint(tint),
                    );
                });
                ui.vertical(|ui| {
                    ui.label("nearest");
                    ui.image_with(
                        &nearest,
                        ImageOptions::default()
                            .with_size(target)
                            .with_fit(*fit)
                            .with_tint(tint),
                    );
                });
            });

            ui.separator();
            ui.label(
                "No explicit size: ui.image(&handle) uses the texture's natural logical size \
                 (64×64 points here).",
            );
            ui.image(&linear);

            ui.label(
                "The images above are generated in memory; decoding is intentionally left to the app.",
            );
        });
    }
}

fn checkerboard() -> ImageData {
    let mut rgba = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let checker = ((x / 8) + (y / 8)) % 2 == 0;
            let (r, g, b) = if checker {
                (220, 80, 60)
            } else {
                (50, 120, 200)
            };
            rgba.extend_from_slice(&[r, g, b, 255]);
        }
    }
    ImageData::new([SIZE, SIZE], rgba)
}
