//! A generator's preview enlarged in a tool window, following its settings while they change

use egui::{Rect, Sense, pos2};
use texturecraft_engine::core::{EffectKind, LayerId};

use crate::TextureCraftApp;
use crate::i18n::tr;
use crate::shelf_preview::ShelfPreview;
use crate::theme::Tokens;

/// The width the enlarged picture is rendered at, in pixels: the most a preview renders
const PICTURE_PX: usize = texturecraft_engine::orb_preview::MAX_PREVIEW_SIZE_PX;

/// The window's first width, in points
const DEFAULT_WIDTH: f32 = 420.0;

/// The narrowest the picture shrinks to, in points
const MIN_SIDE: f32 = 160.0;

/// How soon a picture still rendering is looked for again
const PICTURE_POLL: std::time::Duration = std::time::Duration::from_millis(100);

/// The open enlarged preview, if any: closed by its ×, and when its generator is gone
pub(crate) fn show(app: &mut TextureCraftApp, ctx: &egui::Context) {
    let Some(effect) = app.ui.windows.generator_preview else { return };
    let generator = match app.session.doc.project.effect(LayerId(effect)).map(|(_, effect)| &effect.kind) {
        Some(EffectKind::Generator(generator)) => generator.clone(),
        _ => {
            app.ui.windows.generator_preview = None;
            return;
        }
    };

    let set = ShelfPreview::generator_set(&generator, &app.session.images);
    let texture = app.thumbs.generator_set_picture(ctx, effect, set, PICTURE_PX);
    let tokens = Tokens::get(ctx);
    let mut open = true;
    crate::windows::window("generator_preview", tr(generator.kind.label()), &mut open).default_width(DEFAULT_WIDTH).show(ctx, |ui| {
        let side = ui.available_width().max(MIN_SIDE);
        let (rect, response) = ui.allocate_exact_size(egui::vec2(side, side), Sense::hover());
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Image, true, tr("Preview")));
        match &texture {
            Some(texture) => {
                ui.painter().image(texture.id(), rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), egui::Color32::WHITE);
            }
            None => {
                ui.painter().rect_filled(rect, 0.0, tokens.button);
                ctx.request_repaint_after(PICTURE_POLL);
            }
        }
    });
    if !open {
        app.ui.windows.generator_preview = None;
    }
}

/// Opens generator effect `effect`'s enlarged preview, or closes it when it is the one open
pub(crate) fn toggle(app: &mut TextureCraftApp, effect: u64) {
    let open = &mut app.ui.windows.generator_preview;
    *open = if *open == Some(effect) { None } else { Some(effect) };
}
