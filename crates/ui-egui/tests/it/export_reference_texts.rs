//! Export textures window (texturecraft #789): the reference's size, button tooltips and settings
//! texts, word for word.

use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use texturecraft_ui_egui::TextureCraftApp;

/// The reference window's content size in points
const REFERENCE_WINDOW_WIDTH_POINTS: f32 = 1100.0;
const REFERENCE_WINDOW_HEIGHT_POINTS: f32 = 740.0;

/// How far the measured content may differ from the reference size (item spacing, borders)
const SIZE_TOLERANCE_POINTS: f32 = 40.0;

/// The right bar is the last 38 points of the window
const RAIL_WIDTH_POINTS: f32 = 38.0;

/// The Export textures window open on SETTINGS in a screen of `screen_size`
fn harness(screen_size: egui::Vec2) -> Harness<'static, TextureCraftApp> {
    let session = texturecraft_engine::sample_session(256, 64).unwrap();
    let mut h = Harness::builder().with_size(screen_size).build_eframe(|cc| TextureCraftApp::new(cc, session));
    h.state_mut().ui.prefs.language = "en".into();
    h.run_steps(4);
    h.state_mut().ui.windows.export = true;
    h.run_steps(4);
    h
}

/// The rectangle of the window's button named `label`; the rail has an Export icon of its own
fn window_button_rect(h: &Harness<'_, TextureCraftApp>, label: &str, screen_width: f32) -> egui::Rect {
    h.get_all_by_role(egui::Role::Button)
        .filter(|button| button.rect().max.x < screen_width - RAIL_WIDTH_POINTS && button.accesskit_node().label().as_deref() == Some(label))
        .map(|button| button.rect())
        .next_back()
        .unwrap_or_else(|| panic!("no {label} button in the window"))
}

/// Hover the window's button `label` and check that its tooltip says `tooltip`
fn tooltip_is(h: &mut Harness<'_, TextureCraftApp>, label: &str, tooltip: &str) {
    let screen_width = h.ctx.content_rect().width();
    h.hover_at(window_button_rect(h, label, screen_width).center());
    h.run_steps(30);

    assert!(h.query_all_by_label(tooltip).next().is_some(), "{label}: {tooltip}");
}

#[test]
fn the_window_has_the_reference_size() {
    let h = harness(egui::vec2(1600.0, 960.0));
    let reset = window_button_rect(&h, "Reset all", 1600.0);
    let export = window_button_rect(&h, "Export", 1600.0);
    let tabs = h.get_by_label("SETTINGS").rect();

    let width = export.max.x - reset.min.x;
    let height = export.max.y - tabs.min.y;

    assert!((width - REFERENCE_WINDOW_WIDTH_POINTS).abs() < SIZE_TOLERANCE_POINTS, "width {width}");
    assert!((height - REFERENCE_WINDOW_HEIGHT_POINTS).abs() < SIZE_TOLERANCE_POINTS, "height {height}");
}

#[test]
fn on_a_small_screen_the_window_fits_with_its_buttons_in_view() {
    let screen = egui::vec2(1000.0, 700.0);
    let h = harness(screen);

    for label in ["Reset all", "Cancel", "Save settings", "Export"] {
        let rect = window_button_rect(&h, label, screen.x);
        assert!(rect.min.x >= 0.0 && rect.max.x <= screen.x, "{label} {rect:?}");
        assert!(rect.min.y >= 0.0 && rect.max.y <= screen.y, "{label} {rect:?}");
    }
}

#[test]
fn the_buttons_carry_the_reference_tooltips() {
    let mut h = harness(egui::vec2(1600.0, 960.0));
    tooltip_is(&mut h, "Include all", "Includes all Texture Sets, UV Tiles and output maps.");
    tooltip_is(&mut h, "Reset all", "Resets all settings to their default values, including all textures.");
    tooltip_is(&mut h, "Cancel", "Closes the export window and undoes all recent changes");
    tooltip_is(&mut h, "Save settings", "Closes the export window and saves changes");
    tooltip_is(&mut h, "Export", "Exports all selected textures and additional files");
}

/// Hover the checkbox `label` and check that its tooltip says `tooltip`
fn checkbox_tooltip_is(h: &mut Harness<'_, TextureCraftApp>, label: &str, tooltip: &str) {
    let center = h.get_by_role_and_label(egui::Role::CheckBox, label).rect().center();
    h.hover_at(center);
    h.run_steps(30);

    assert!(h.query_all_by_label(tooltip).next().is_some(), "{label}: {tooltip}");
}

#[test]
fn the_unavailable_checkboxes_carry_the_reference_texts() {
    let mut h = harness(egui::vec2(1600.0, 960.0));
    checkbox_tooltip_is(
        &mut h,
        "Export USD asset",
        "Export USD files hierarchy with material and textures. References the original mesh if possible, exports the mesh otherwise",
    );
    checkbox_tooltip_is(&mut h, "Export shaders parameters", "Exports a JSON file with the settings for all shader instances");
}

#[test]
fn the_bottom_bar_buttons_share_one_width() {
    let h = harness(egui::vec2(1600.0, 960.0));

    let widths: Vec<f32> = ["Reset all", "Cancel", "Save settings", "Export"].iter().map(|label| window_button_rect(&h, label, 1600.0).width()).collect();

    assert!(widths.windows(2).all(|pair| (pair[0] - pair[1]).abs() < 1.0), "{widths:?}");
}
