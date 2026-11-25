//! The picture that follows the pointer while an asset is dragged from the Assets panel, with a
//! badge on its corner saying whether the place under the pointer takes the asset

use egui::{Color32, Context, Id, LayerId, Order, Pos2, Rect, Stroke, Vec2, vec2};

use crate::shelf::ShelfDrag;
pub use crate::shelf_drag_verdict::Verdict;
use crate::theme::Tokens;

/// Side of the dragged asset's thumbnail
const GHOST_SIZE: f32 = 48.0;

/// The thumbnail's top-left corner from the pointer: just under its tip
const GHOST_OFFSET: Vec2 = vec2(4.0, 4.0);

/// How opaque the thumbnail is, so the target shows through it
const GHOST_OPACITY: f32 = 0.6;

/// Corner rounding of the name chip
const CHIP_ROUNDING: f32 = 3.0;

/// Side of the badge
const BADGE_SIZE: f32 = 9.0;

/// The badge's centre from the pointer: past the pointer arrow's tail, on the thumbnail's corner
const BADGE_OFFSET: Vec2 = vec2(14.0, 22.0);

/// Size of the badge's icon
const BADGE_ICON_SIZE: f32 = 7.5;

/// Corner rounding of the square badge
const BADGE_ROUNDING: f32 = 2.0;

/// Fill of the square badge and colour of the icon on the red one
const BADGE_LIGHT: Color32 = Color32::from_gray(245);

/// Colour of the icon on the square badge
const BADGE_DARK: Color32 = Color32::from_gray(35);

/// Opacity of the thin dark outline that keeps the badge apart from whatever is under it
const BADGE_OUTLINE_ALPHA: u8 = 140;

/// Padding of the name chip shown when the asset has no thumbnail
const CHIP_PADDING: Vec2 = vec2(6.0, 3.0);

/// Text size of the name chip
const CHIP_TEXT_SIZE: f32 = 11.0;

/// Pass number of the last drop target that took the dragged asset (ctx temp data)
const ACCEPTED: &str = "shelf_drag_accepted";

/// What the last pass painted (ctx temp data)
const SHOWN: &str = "shelf_drag_shown";

/// The ghost as the last pass painted it
#[derive(Clone, Copy, Debug)]
pub struct Shown {
    /// What the badge says
    pub verdict: Verdict,
    /// Where the thumbnail (or the name chip) is
    pub ghost: Rect,
    /// Where the badge is
    pub badge: Rect,
}

/// Note that the drop target being drawn takes the dragged asset, where it paints its drop highlight
pub(crate) fn accept(ctx: &Context) {
    let pass = ctx.cumulative_pass_nr();
    ctx.data_mut(|data| data.insert_temp(Id::unique(ACCEPTED), pass));
}

/// The ghost painted this pass, if an asset is being dragged
pub fn shown(ctx: &Context) -> Option<Shown> {
    ctx.data(|data| data.get_temp(Id::unique(SHOWN)))
}

/// Paint the dragged asset and its badge at the pointer, once a pass after every drop target
pub(crate) fn show(ctx: &Context) {
    let (Some(drag), Some(pointer)) = (egui::DragAndDrop::payload::<ShelfDrag>(ctx), ctx.input(|input| input.pointer.hover_pos())) else {
        ctx.data_mut(|data| data.remove::<Shown>(Id::unique(SHOWN)));
        return;
    };

    let pass = ctx.cumulative_pass_nr();
    let accepted = ctx.data(|data| data.get_temp::<u64>(Id::unique(ACCEPTED))) == Some(pass);
    let verdict = if accepted { Verdict::Valid } else { Verdict::NotValid };
    let painter = ctx.layer_painter(LayerId::new(Order::Tooltip, Id::unique("shelf_drag_ghost")));
    let tokens = Tokens::get(ctx);

    let ghost = match &drag.thumb {
        Some(thumb) => {
            let rect = Rect::from_min_size(pointer + GHOST_OFFSET, Vec2::splat(GHOST_SIZE));
            let uv = Rect::from_min_max(Pos2::ZERO, egui::pos2(1.0, 1.0));
            painter.image(thumb.id(), rect, uv, Color32::WHITE.gamma_multiply(GHOST_OPACITY));
            rect
        }
        None => name_chip(&painter, &tokens, pointer + GHOST_OFFSET, &drag.label),
    };
    let badge = badge(&painter, &tokens, pointer + BADGE_OFFSET, verdict);
    ctx.data_mut(|data| data.insert_temp(Id::unique(SHOWN), Shown { verdict, ghost, badge }));
}

/// The asset's name in a chip at `top_left`, for assets without a thumbnail (saved smart materials)
fn name_chip(painter: &egui::Painter, tokens: &Tokens, top_left: Pos2, label: &str) -> Rect {
    let galley = painter.layout_no_wrap(label.to_owned(), egui::FontId::proportional(CHIP_TEXT_SIZE), tokens.text_strong);
    let rect = Rect::from_min_size(top_left, galley.size() + CHIP_PADDING * 2.0);
    painter.rect_filled(rect, CHIP_ROUNDING, tokens.row_selected);
    painter.rect_stroke(rect, CHIP_ROUNDING, Stroke::new(1.0, tokens.accent), egui::StrokeKind::Inside);
    painter.galley(rect.min + CHIP_PADDING, galley, tokens.text_strong);
    rect
}

/// The badge at `centre`: a crosshair on a light square where a drop works, else a no-entry sign
fn badge(painter: &egui::Painter, tokens: &Tokens, centre: Pos2, verdict: Verdict) -> Rect {
    let rect = Rect::from_center_size(centre, Vec2::splat(BADGE_SIZE));
    let outline = Stroke::new(1.0, Color32::from_black_alpha(BADGE_OUTLINE_ALPHA));
    let (icon, color) = match verdict {
        Verdict::Valid => {
            painter.rect(rect, BADGE_ROUNDING, BADGE_LIGHT, outline, egui::StrokeKind::Outside);
            (egui_phosphor::regular::CROSSHAIR_SIMPLE, BADGE_DARK)
        }
        Verdict::NotValid => {
            painter.circle(centre, BADGE_SIZE / 2.0, tokens.danger, outline);
            (egui_phosphor::regular::PROHIBIT, BADGE_LIGHT)
        }
    };
    let glyph = painter.layout_no_wrap(icon.to_owned(), egui::FontId::proportional(BADGE_ICON_SIZE), color);
    painter.galley(centre - glyph.mesh_bounds.center().to_vec2(), glyph, color);
    rect.expand(outline.width)
}
