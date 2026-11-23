//! The Export panel on the rail: Export Mesh and Export Textures

use egui::Ui;

use crate::TextureCraftApp;
use crate::i18n::tr;
use crate::theme::caption;
use crate::widgets::primary_button;

/// Width of the Export Mesh and Export Textures buttons
const EXPORT_BUTTON_WIDTH: f32 = 176.0;

/// The Export panel body (a rail popup or dock tab); its buttons need an open project
pub(crate) fn body(app: &mut TextureCraftApp, ui: &mut Ui) {
    ui.label(caption(&tr("Export").to_uppercase()));
    let project_open = app.session.project_open;
    ui.vertical_centered(|ui| {
        let size = [EXPORT_BUTTON_WIDTH, ui.spacing().interact_size.y];
        if ui.add_enabled(project_open, egui::Button::new(tr("Export Mesh...")).min_size(size.into())).clicked() {
            app.ui.windows.mesh_export_options = Some(app.session.project_id);
            app.ui.rail_panel = None;
        }
        if ui.add_enabled(project_open, primary_button(ui, tr("Export Textures...")).min_size(size.into())).clicked() {
            app.ui.windows.export = true;
            app.ui.rail_panel = None;
        }
    });
}
