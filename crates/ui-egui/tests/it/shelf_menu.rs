//! An asset tile's right-click menu: the info block says what the asset is, and its rows act on it

use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use texturecraft_engine::materials::MATERIALS;
use texturecraft_ui_egui::TextureCraftApp;
use texturecraft_ui_egui::file_dialogs::Answer;

/// The types' positions in the shelf's type row (its table is crate-private)
const MATERIALS_TYPE: usize = 0;
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
