//! The Resources Updater window, driven from the keyboard: the shelf button opens it, outdated
//! resources come first, Update reads a changed file again as one undo step and Select new resource
//! follows another file. File pickers are answered by the override, never shown.

use std::path::{Path, PathBuf};

use egui::{Key, Role};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use serde_json::json;
use texturecraft_engine::Session;
use texturecraft_ui_egui::TextureCraftApp;
use texturecraft_ui_egui::file_dialogs::{Answer, Request};

use super::temp_dir::TempDir;

fn app() -> Harness<'static, TextureCraftApp> {
    let session = Session::new_project("Test", texturecraft_engine::geom::primitives::cube(1.0), "builtin:cube", None, 128).unwrap();
    let mut h = Harness::builder().with_size(egui::vec2(1600.0, 960.0)).build_eframe(|cc| TextureCraftApp::new(cc, session));
    h.state_mut().ui.prefs.language = "en".into();
    h.run_steps(4);

    h
}

/// Write a `size` × `size` PNG (each size is a different file size, so a change is seen at once)
fn png(dir: &Path, name: &str, size: u32) -> PathBuf {
    let file = dir.join(name);
    image::RgbaImage::from_pixel(size, size, image::Rgba([10, 200, 30, 255])).save(&file).unwrap();

    file
}

fn import(h: &mut Harness<'_, TextureCraftApp>, file: &Path) {
    h.state_mut().run("resource.import", json!({ "path": file.to_str().unwrap() })).unwrap();
    h.run_steps(2);
}

fn press(h: &mut Harness<'_, TextureCraftApp>, key: Key) {
    h.key_press(key);
    h.run_steps(3);
}

/// Move the keyboard focus to button `label` and press Enter
fn activate(h: &mut Harness<'_, TextureCraftApp>, label: &str) {
    h.get_by_role_and_label(Role::Button, label).focus();
    h.run_steps(2);
    press(h, Key::Enter);
}

fn shows(h: &Harness<'_, TextureCraftApp>, text: &str) -> bool {
    h.query_by_label(text).is_some()
}

fn width_of(h: &Harness<'_, TextureCraftApp>, name: &str) -> u32 {
    h.state().session.images[name].width
}

#[test]
fn the_window_opens_from_the_keyboard_and_updates_an_outdated_resource_as_one_undo_step() {
    let dir = TempDir::new("resources-updater-update");
    let (kept, changed) = (png(&dir, "kept.png", 4), png(&dir, "changed.png", 4));
    let mut h = app();
    import(&mut h, &kept);
    import(&mut h, &changed);
    png(&dir, "changed.png", 8);

    activate(&mut h, "Resources Updater");

    let rows = &h.state().ui.windows.resources_updater.as_ref().expect("the window is open").rows;
    assert_eq!(rows[0].name, "changed.png", "outdated resources come first");
    assert!(shows(&h, "Project : Test"));
    assert!(shows(&h, "(2 resources, 1 outdated)"));
    assert!(shows(&h, "Outdated"), "the status is written out, not only coloured");
    assert!(h.get_by_role_and_label(Role::Button, "Update kept.png").accesskit_node().is_disabled(), "only outdated resources update");

    let steps = h.state().session.history_labels().len();
    activate(&mut h, "Update changed.png");

    assert_eq!(width_of(&h, "changed.png"), 8, "the new file's pixels are in the project");
    assert_eq!(h.state().session.history_labels().len(), steps + 1, "one undo step");
    assert!(shows(&h, "(2 resources, 0 outdated)"));

    // Undo brings the old pixels back, so the list shows the file as changed again
    h.state_mut().run("edit.undo", json!({})).unwrap();
    h.run_steps(3);

    assert_eq!(width_of(&h, "changed.png"), 4);
    assert!(shows(&h, "(2 resources, 1 outdated)"));

    activate(&mut h, "Update All");
    assert_eq!(width_of(&h, "changed.png"), 8);

    press(&mut h, Key::Escape);
    assert!(h.state().ui.windows.resources_updater.is_none(), "Escape closes the window");
}

#[test]
fn select_new_resource_follows_the_chosen_file_and_the_name_filter_narrows_the_list() {
    let dir = TempDir::new("resources-updater-relink");
    let (first, second) = (png(&dir, "first.png", 4), png(&dir, "second.png", 4));
    let other = png(&dir, "other.png", 16);
    let mut h = app();
    import(&mut h, &first);
    import(&mut h, &second);
    activate(&mut h, "Resources Updater");

    h.state_mut().file_dialogs.set_override(Some(Box::new(move |_: &Request| Answer::Paths(vec![other.clone()]))));
    activate(&mut h, "Select new resource first.png");
    h.run_steps(3);

    assert_eq!(width_of(&h, "first.png"), 16, "the resource keeps its name and takes the chosen file's pixels");
    assert!(h.state().session.doc.project.resource_sources["first.png"].path.ends_with("other.png"));

    h.get_by_role_and_label(Role::TextInput, "Name filter:").focus();
    h.run_steps(2);
    h.get_by_role_and_label(Role::TextInput, "Name filter:").type_text("SEC");
    h.run_steps(3);

    assert!(h.query_by_label("Update first.png").is_none(), "the filter hides other names");
    assert!(h.query_by_label("Update second.png").is_some(), "names match ignoring case");
}

#[test]
fn show_in_folder_shows_the_folder_of_the_resource_file() {
    let dir = TempDir::new("resources-updater-folder");
    let file = png(&dir, "bricks.png", 4);
    let mut h = app();
    import(&mut h, &file);
    activate(&mut h, "Resources Updater");

    activate(&mut h, "Show in folder bricks.png");

    let shown = h.state().file_dialogs.shown_folders().to_vec();
    assert_eq!(shown.len(), 1);
    assert_eq!(shown[0].file_name(), dir.file_name(), "the file's own folder");
}
