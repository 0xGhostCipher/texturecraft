//! An asset tile's right-click menu: the info block says what the asset is, and its rows act on it

use egui::{Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use serde_json::json;
use texturecraft_engine::materials::MATERIALS;
use texturecraft_ui_egui::TextureCraftApp;
use texturecraft_ui_egui::file_dialogs::Answer;

/// The types' positions in the shelf's type row (its table is crate-private)
const MATERIALS_TYPE: usize = 0;
const BRUSHES_TYPE: usize = 4;
const TEXTURES_TYPE: usize = 6;

/// The sample project with the shelf on type `tab`
fn shelf(tab: usize) -> Harness<'static, TextureCraftApp> {
    let session = texturecraft_engine::sample_session(256, 64).unwrap();
    let mut harness = Harness::builder().with_size(egui::vec2(1600.0, 960.0)).build_eframe(|cc| TextureCraftApp::new(cc, session));
    harness.state_mut().ui.shelf_tab = tab;
    harness.run_steps(4);
    harness
}

fn right_click(harness: &mut Harness<'_, TextureCraftApp>, label: &str) {
    harness.get_all_by_label(label).next().unwrap_or_else(|| panic!("no tile {label}")).click_secondary();
    harness.run_steps(3);
}

#[test]
fn a_built_in_tile_menu_says_what_the_asset_is_and_where_it_lives() {
    // The grid's first plain material (by name), so its tile is in view
    let material = MATERIALS.iter().filter(|material| !material.smart).min_by_key(|material| material.label.to_lowercase()).unwrap();
    let mut harness = shelf(MATERIALS_TYPE);

    right_click(&mut harness, material.label);

    for row in [
        format!("Name {}", material.label),
        "Usages material".to_string(),
        format!("Path /Starter assets/Materials/{}/", material.category),
        format!("Category {}", material.category),
    ] {
        assert!(harness.query_by_label(&row).is_some(), "the menu has the info row {row:?}");
    }
    assert!(harness.get_by_label("Export resource...").accesskit_node().is_disabled(), "a built-in has no file to export");

    harness.get_by_label("Close").click();
    harness.run_steps(3);
    assert!(harness.query_by_label("Usages material").is_none(), "Close closes the menu");
}

#[test]
fn export_resource_writes_the_image_as_imported() {
    let mut harness = shelf(TEXTURES_TYPE);
    let mut bytes = Vec::new();
    image::RgbaImage::from_pixel(4, 4, image::Rgba([180, 90, 40, 255])).write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png).unwrap();
    let name = harness.state_mut().session.import_resource_bytes("plate.png", bytes.clone()).unwrap();
    harness.run_steps(3);
    let dir = super::temp_dir::TempDir::new("shelf-export-resource");
    let target = dir.join("exported.png");
    let answer = target.clone();
    harness.state_mut().file_dialogs.set_override(Some(Box::new(move |_| Answer::Paths(vec![answer.clone()]))));

    right_click(&mut harness, &name);
    harness.get_by_label("Export resource...").click();
    harness.run_steps(4);

    assert_eq!(std::fs::read(&target).unwrap(), bytes, "the exported file is the resource's own bytes");
}

fn preset_names(harness: &Harness<'_, TextureCraftApp>) -> Vec<String> {
    harness.state().session.brush_presets.list.iter().map(|preset| preset.name.clone()).collect()
}

/// Selects the menu's Name field text and types `text`
fn type_name(harness: &mut Harness<'_, TextureCraftApp>, text: &str) {
    harness.get_by_role_and_label(egui::Role::TextInput, "Name").click();
    harness.run_steps(3);
    harness.key_press_modifiers(Modifiers::COMMAND, Key::A);
    harness.event(egui::Event::Text(text.into()));
    harness.run_steps(3);
}

#[test]
fn a_saved_brush_menu_renames_replaces_and_deletes_it() {
    // A name that sorts before every built-in brush, so its tile is in view
    let mut harness = shelf(BRUSHES_TYPE);
    texturecraft_engine::execute(&mut harness.state_mut().session, "brush.preset.store", &json!({"name": "Aa ink"})).unwrap();
    harness.run_steps(3);

    right_click(&mut harness, "Aa ink");
    type_name(&mut harness, "Never applied");
    harness.key_press(Key::Escape);
    harness.run_steps(3);
    assert_eq!(preset_names(&harness), ["Aa ink"], "Escape keeps the name");

    right_click(&mut harness, "Aa ink");
    type_name(&mut harness, "Aa pen");
    harness.key_press(Key::Enter);
    harness.run_steps(3);
    assert_eq!(preset_names(&harness), ["Aa pen"], "Enter renames the brush");

    texturecraft_engine::execute(&mut harness.state_mut().session, "tool.brush", &json!({"size": 77})).unwrap();
    right_click(&mut harness, "Aa pen");
    harness.get_by_label("Replace with the current brush").click();
    harness.run_steps(3);
    assert_eq!(harness.state().session.brush_presets.list[0].brush, harness.state().session.tool.brush, "the preset holds the current brush");

    right_click(&mut harness, "Aa pen");
    harness.get_by_label("Delete").click();
    harness.run_steps(3);
    assert!(preset_names(&harness).is_empty(), "Delete removes the brush");
}

#[test]
fn delete_removes_a_project_image() {
    let mut harness = shelf(TEXTURES_TYPE);
    let mut bytes = Vec::new();
    image::RgbaImage::from_pixel(4, 4, image::Rgba([180, 90, 40, 255])).write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png).unwrap();
    let name = harness.state_mut().session.import_resource_bytes("plate.png", bytes).unwrap();
    harness.run_steps(3);

    right_click(&mut harness, &name);
    harness.get_by_label("Delete").click();
    harness.run_steps(3);

    assert!(!harness.state().session.doc.resources.contains_key(&name));
}
