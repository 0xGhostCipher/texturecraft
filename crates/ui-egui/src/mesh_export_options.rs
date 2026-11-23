//! File > Export Mesh asks for the mesh export options first; Export then leads to the file choice

use egui::Ui;

use crate::TextureCraftApp;
use crate::i18n::tr;
use crate::unavailable;

/// Indent of an option's checkbox under its choice
const OPTION_INDENT: f32 = 20.0;

/// Width of the Cancel and Export buttons
const BUTTON_WIDTH: f32 = 110.0;

/// The reference dialog's content width
const DIALOG_WIDTH: f32 = 510.0;

/// Gap between the two option groups, as in the reference
const GROUP_GAP: f32 = 25.0;

/// The prompt, while File > Export Mesh waits on it
pub(crate) fn show(app: &mut TextureCraftApp, ctx: &egui::Context) {
    if app.ui.windows.mesh_export_options != Some(app.session.project_id) {
        app.ui.windows.mesh_export_options = None;
        return;
    }

    let mut open = true;
    let (mut export, mut cancel) = (false, false);
    crate::windows::dialog("mesh_export_options", tr("Mesh Export Options"), &mut open).show(ctx, |ui| {
        ui.set_width(DIALOG_WIDTH);

        // TextureCraft exports the mesh as it is: triangulated, without displacement
        ui.add(egui::RadioButton::new(true, tr("Without displacement/tessellation")));
        option(ui, true, tr("Apply triangulation"));
        ui.add_space(GROUP_GAP);
        unavailable::greyed(ui, |ui| ui.add(egui::RadioButton::new(false, tr("With displacement/tessellation"))));
        option(ui, true, tr("Recompute vertex normals"));
        ui.add_space(8.0);

        // One row keeps the window to its contents; a bare right-to-left layout fills the screen height
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let size = egui::vec2(BUTTON_WIDTH, ui.spacing().interact_size.y);
                export = ui.add(egui::Button::new(tr("Export")).min_size(size)).clicked();
                cancel = ui.add(egui::Button::new(tr("Cancel")).min_size(size)).clicked();
            });
        });
    });

    if export {
        app.ui.windows.mesh_export_options = None;
        crate::windows::file_action(app, ctx, "file.exportMesh");
    } else if cancel || !open {
        app.ui.windows.mesh_export_options = None;
    }
}

/// A greyed option under a choice, ticked as the reference shows it
fn option(ui: &mut Ui, checked: bool, label: &str) {
    ui.horizontal(|ui| {
        ui.add_space(OPTION_INDENT);
        unavailable::checkbox(ui, checked, label);
    });
}
