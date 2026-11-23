//! Export textures window (texturecraft #789): the reference's size, tab labels, button tooltips
//! and settings texts, word for word.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use texturecraft_ui_egui::TextureCraftApp;

/// The Export textures window open on SETTINGS
fn harness() -> Harness<'static, TextureCraftApp> {
    let session = texturecraft_engine::sample_session(256, 64).unwrap();
    let mut h = Harness::builder().with_size(egui::vec2(1600.0, 960.0)).build_eframe(|cc| TextureCraftApp::new(cc, session));
    h.state_mut().ui.prefs.language = "en".into();
    h.run_steps(4);
    h.state_mut().ui.windows.export = true;
    h.run_steps(4);
    h
}

/// Hover the newest node labelled `label` until its tooltip shows
fn hover(h: &mut Harness<'_, TextureCraftApp>, label: &str) {
    let center = h.get_all_by_label(label).last().unwrap_or_else(|| panic!("no {label}")).rect().center();
    h.hover_at(center);
    h.run_steps(30);
}

/// Hover `label` and check that its tooltip says `tooltip`
fn tooltip_is(h: &mut Harness<'_, TextureCraftApp>, label: &str, tooltip: &str) {
    hover(h, label);
    assert!(h.query_all_by_label(tooltip).next().is_some(), "{label}: {tooltip}");
}

#[test]
fn the_window_is_as_wide_as_the_reference_window() {
    let h = harness();
    let reset = h.get_by_label("Reset all").rect();
    let save = h.get_by_label("Save settings").rect();

    // The reference window is 1100 wide; the buttons sit at its two ends (it was 900 wide)
    assert!(save.max.x - reset.min.x > 900.0, "{reset:?} {save:?}");
}

#[test]
fn the_tabs_read_in_capitals() {
    let h = harness();
    for tab in ["SETTINGS", "OUTPUT TEMPLATES", "LIST OF EXPORTS"] {
        assert!(h.query_by_label(tab).is_some(), "{tab}");
    }
}

#[test]
fn the_buttons_carry_the_reference_tooltips() {
    let mut h = harness();
    tooltip_is(&mut h, "Include all", "Includes all Texture Sets, UV Tiles and output maps.");
    tooltip_is(&mut h, "Reset all", "Resets all settings to their default values, including all textures.");
    tooltip_is(&mut h, "Cancel", "Closes the export window and undoes all recent changes");
    tooltip_is(&mut h, "Save settings", "Closes the export window and saves changes");
    tooltip_is(&mut h, "Export", "Exports all selected textures and additional files");
}

#[test]
fn the_unavailable_checkboxes_carry_the_reference_texts() {
    let mut h = harness();
    tooltip_is(
        &mut h,
        "Export USD asset",
        "Export USD files hierarchy with material and textures. References the original mesh if possible, exports the mesh otherwise",
    );
    tooltip_is(&mut h, "Export shaders parameters", "Exports a JSON file with the settings for all shader instances");
}
