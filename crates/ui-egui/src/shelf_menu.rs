//! An asset tile's right-click menu: what the asset is (name, usages, path, category), then what can be done with it

use egui::{Align2, Color32, CornerRadius, FontId, Response, Sense, Ui, pos2, vec2};
use serde_json::json;

use crate::TextureCraftApp;
use crate::i18n::tr;
use crate::shelf_entry::{LIBRARIES, ShelfEntry};
use crate::theme::Tokens;

/// A menu row's height and the inset of its hover face from the menu's sides, in points
const ROW_HEIGHT: f32 = 24.0;
const HOVER_INSET: f32 = 3.0;
const HOVER_ROUNDING: u8 = 3;
/// Where a row's text starts, from the menu's left edge, in points
const TEXT_X: f32 = 10.0;
/// The space after the text of the widest row, in points
const RIGHT_PADDING: f32 = 24.0;
/// The air between the info block's label column and its values, in points
const LABEL_GAP: f32 = 12.0;
/// The widest the info block's values get before the path is cut in the middle, in points
const MAX_VALUE_WIDTH: f32 = 220.0;
/// The menu's text size, in points
const TEXT_SIZE: f32 = 12.0;
/// The air above and below the separators, in points
const SEPARATOR_SPACE: f32 = 3.0;

/// The right-click menu on the tile whose response is `response`
pub(crate) fn show(app: &mut TextureCraftApp, response: &Response, entry: &ShelfEntry) {
    response.context_menu(|ui| {
        let tokens = Tokens::get(ui.ctx());
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        let info = info_rows(entry);
        let width = menu_width(ui, &info, &row_labels(entry));
        ui.set_width(width);
        info_block(ui, width, &info, &tokens);
        separator(ui, &tokens);

        if let ShelfEntry::Image(name) = entry {
            if row(ui, width, tr("Export resource..."), true, &tokens).clicked() {
                export_image(app, name);
                ui.close();
            }
            separator(ui, &tokens);
            if row(ui, width, tr("Add as decal (view center)"), true, &tokens).clicked() {
                app.run("layer.add.decal", json!({"image": name}));
                ui.close();
            }
            if row(ui, width, tr("Remove resource"), true, &tokens).clicked() {
                app.run("resource.remove", json!({"name": name}));
                ui.close();
            }
        } else {
            row(ui, width, tr("Export resource..."), false, &tokens).on_hover_text(tr("Built-in assets have no file to export"));
        }

        separator(ui, &tokens);
        if row(ui, width, tr("Close"), true, &tokens).clicked() {
            ui.close();
        }
    });
}

/// The info block's label and value rows: rows only where the asset has the data
fn info_rows(entry: &ShelfEntry) -> Vec<(&'static str, String)> {
    let path = entry.path();
    let shown_path: Vec<&str> = path.iter().map(|part| LIBRARIES.iter().find(|(id, _)| id == part).map_or_else(|| tr(part), |(_, name)| tr(name))).collect();
    let mut rows =
        vec![(tr("Name"), entry.label().to_string()), (tr("Usages"), tr(entry.usage()).to_string()), (tr("Path"), format!("/{}/", shown_path.join("/")))];
    if let Some(category) = path.get(2) {
        rows.push((tr("Category"), tr(category).to_string()));
    }
    rows
}

/// The labels of the rows under the info block, for the menu's width
fn row_labels(entry: &ShelfEntry) -> Vec<&'static str> {
    let mut labels = vec![tr("Export resource..."), tr("Close")];
    if matches!(entry, ShelfEntry::Image(_)) {
        labels.extend([tr("Add as decal (view center)"), tr("Remove resource")]);
    }
    labels
}

fn text_width(ui: &Ui, text: &str) -> f32 {
    ui.fonts_mut(|fonts| fonts.layout_no_wrap(text.to_owned(), FontId::proportional(TEXT_SIZE), Color32::WHITE).size().x)
}

/// The label column's width: the longest label and its gap
fn label_column(ui: &Ui, info: &[(&str, String)]) -> f32 {
    info.iter().map(|(label, _)| text_width(ui, label)).fold(0.0, f32::max) + LABEL_GAP
}

/// The widest row, the info block's values capped at [`MAX_VALUE_WIDTH`]
fn menu_width(ui: &Ui, info: &[(&str, String)], labels: &[&str]) -> f32 {
    let widest_value = info.iter().map(|(_, value)| text_width(ui, value)).fold(0.0, f32::max).min(MAX_VALUE_WIDTH);
    let widest_row = labels.iter().map(|label| text_width(ui, label)).fold(0.0, f32::max);
    TEXT_X + (label_column(ui, info) + widest_value).max(widest_row) + RIGHT_PADDING
}

/// The label (dim) and value rows; a value too wide is cut in the middle with "…"
fn info_block(ui: &mut Ui, width: f32, info: &[(&str, String)], tokens: &Tokens) {
    let column = label_column(ui, info);
    let value_width = width - TEXT_X - column - RIGHT_PADDING;
    for (label, value) in info {
        let (rect, response) = ui.allocate_exact_size(vec2(width, ROW_HEIGHT), Sense::hover());
        let shown = cut_middle(ui, value, value_width);
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, format!("{label} {value}")));
        let painter = ui.painter();
        painter.text(pos2(rect.left() + TEXT_X, rect.center().y), Align2::LEFT_CENTER, *label, FontId::proportional(TEXT_SIZE), tokens.text_dim);
        painter.text(pos2(rect.left() + TEXT_X + column, rect.center().y), Align2::LEFT_CENTER, shown, FontId::proportional(TEXT_SIZE), tokens.text);
    }
}

/// `text`, or its start and end around "…" so it fits `width` points
fn cut_middle(ui: &Ui, text: &str, width: f32) -> String {
    if text_width(ui, text) <= width {
        return text.to_owned();
    }

    let chars: Vec<char> = text.chars().collect();
    let mut kept = chars.len();
    loop {
        let head = kept.div_ceil(2);
        let tail = kept / 2;
        let cut: String = chars[..head].iter().chain(['…'].iter()).chain(chars[chars.len() - tail..].iter()).collect();
        if kept == 0 || text_width(ui, &cut) <= width {
            return cut;
        }

        kept -= 1;
    }
}

/// A hairline across the menu between groups of rows
fn separator(ui: &mut Ui, tokens: &Tokens) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), SEPARATOR_SPACE * 2.0 + 1.0), Sense::hover());
    ui.painter().hline(rect.x_range(), rect.center().y, egui::Stroke::new(1.0, tokens.separator));
}

/// One menu row: a hover face inset from the sides, its text dim and unclickable when not `enabled`
fn row(ui: &mut Ui, width: f32, label: &str, enabled: bool, tokens: &Tokens) -> Response {
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, response) = ui.allocate_exact_size(vec2(width, ROW_HEIGHT), sense);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, label));
    let painter = ui.painter();
    if enabled && (response.hovered() || response.has_focus()) {
        painter.rect_filled(rect.shrink2(vec2(HOVER_INSET, 0.0)), CornerRadius::same(HOVER_ROUNDING), tokens.button);
    }
    let color = if enabled { tokens.text } else { tokens.text_dim };
    painter.text(pos2(rect.left() + TEXT_X, rect.center().y), Align2::LEFT_CENTER, label, FontId::proportional(TEXT_SIZE), color);
    if enabled { response } else { response.on_hover_cursor(egui::CursorIcon::NotAllowed) }
}

/// Asks where to save image resource `name`, then writes it there as imported
#[cfg(not(target_arch = "wasm32"))]
fn export_image(app: &mut TextureCraftApp, name: &str) {
    use crate::file_dialogs::{Bound, FileDialog};

    let extension = std::path::Path::new(name).extension().and_then(|extension| extension.to_str()).unwrap_or_default().to_owned();
    let name = name.to_owned();
    let dialog = FileDialog::save().filter(tr("Image"), &[extension.as_str()]).file_name(&name);
    app.ask_file(dialog, Bound::Project, move |app, paths| {
        if let Some(path) = paths.first() {
            app.run("resource.export", json!({"name": name, "path": path.to_string_lossy()}));
        }
    });
}

#[cfg(target_arch = "wasm32")]
fn export_image(app: &mut TextureCraftApp, _: &str) {
    app.toast(tr("Exporting resources needs the desktop app"), true);
}
