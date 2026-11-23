//! Export textures › OUTPUT TEMPLATES (texturecraft #674): the reference's layout, names and the
//! preset list's right-click menu and F2.

use egui::{Event, Modifiers, PointerButton};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use serde_json::json;
use texturecraft_engine::core::Channel;
use texturecraft_engine::formats::{Dest, SourceRef};
use texturecraft_ui_egui::TextureCraftApp;
use texturecraft_ui_egui::state::{MapExport, SetExport};

#[path = "output_template/template_library.rs"]
mod template_library;

use template_library::TemplateLibrary;

/// OUTPUT TEMPLATES with the user template "Mine" selected
fn harness(library: &TemplateLibrary) -> Harness<'static, TextureCraftApp> {
    let session = texturecraft_engine::sample_session(256, 64).unwrap();
    let mut h = Harness::builder().with_size(egui::vec2(1600.0, 960.0)).build_eframe(|cc| TextureCraftApp::new(cc, session));
    h.state_mut().ui.prefs.language = "en".into();
    h.run_steps(4);
    h.state_mut().session.load_export_presets(library.0.clone());
    let base = texturecraft_engine::formats::builtin_preset("Unreal Engine (Packed)").unwrap();
    let mine = texturecraft_engine::formats::ExportPreset { name: "Mine".into(), ..base };
    h.state_mut().run("export.preset.save", json!({ "preset": mine })).unwrap();
    let ui = &mut h.state_mut().ui;
    ui.windows.export = true;
    ui.export.tab = 1;
    ui.export.preset = "Mine".into();
    h.run_steps(4);
    h
}

/// Left edge of the right rail in the 1600 point wide harness
const RAIL_LEFT_EDGE: f32 = 1560.0;

fn click(h: &mut Harness<'_, TextureCraftApp>, label: &str) {
    h.get_all_by_label(label).last().unwrap_or_else(|| panic!("no {label}")).click();
    h.run_steps(4);
}

/// Right-click the grip of output map `index` and pick `item` ("Move up" or "Move down")
fn move_map(h: &mut Harness<'_, TextureCraftApp>, index: usize, item: &str) {
    h.get_all_by_label("Reorder").nth(index).unwrap_or_else(|| panic!("no grip {index}")).click_secondary();
    h.run_steps(4);
    h.get_by_label(item).click();
    h.run_steps(4);
}

/// The file name field of output map `index`: the text field on its grip's row
fn name_field(h: &Harness<'_, TextureCraftApp>, index: usize) -> egui::Rect {
    let grip = h.get_all_by_label("Reorder").nth(index).unwrap_or_else(|| panic!("no grip {index}")).rect();
    h.get_all_by_role(egui::Role::TextInput)
        .map(|field| field.rect())
        .find(|field| (field.center().y - grip.center().y).abs() < 6.0 && field.left() > grip.right())
        .unwrap_or_else(|| panic!("no file name field on row {index}"))
}

/// Hover the file name field of output map `index` until its tooltip shows
fn hover_name_field(h: &mut Harness<'_, TextureCraftApp>, index: usize) {
    let field = name_field(h, index);
    h.hover_at(field.center());
    h.run_steps(30);
}

/// The last button labelled `label` in the window (the newest output map's swatch)
fn last_window_button<'h>(h: &'h Harness<'_, TextureCraftApp>, label: &str) -> egui_kittest::Node<'h> {
    h.get_all_by_role(egui::Role::Button)
        .filter(|button| button.rect().max.x < RAIL_LEFT_EDGE && button.accesskit_node().label().as_deref() == Some(label))
        .last()
        .unwrap_or_else(|| panic!("no {label} button"))
}

/// Drop `source` on the point `at`, as a drag from the map lists ends there
fn drop_on(h: &mut Harness<'_, TextureCraftApp>, source: SourceRef, at: egui::Pos2) {
    egui::DragAndDrop::set_payload(&h.ctx, source);
    h.hover_at(at);
    h.run_steps(2);
    h.event(Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(4);
}

/// Right-click the Presets list's `label` (listed before the selected one's name in the middle)
fn right_click(h: &mut Harness<'_, TextureCraftApp>, label: &str) {
    h.get_all_by_label(label).next().unwrap_or_else(|| panic!("no {label}")).click_secondary();
    h.run_steps(4);
}

#[test]
fn the_tab_shows_the_preset_tools_and_map_editor() {
    let library = TemplateLibrary::new("layout");
    let mut h = harness(&library);
    for text in ["Presets", "Naming pattern", "Current preset:", "Output maps", "Create:", "Drag and drop maps on output's channels to setup your export."] {
        assert!(h.query_by_label(text).is_some(), "{text}");
    }
    assert!(h.get_all_by_role(egui::Role::ComboBox).any(|c| c.value().as_deref() == Some("OpenPBR")));
    let maps = h.state().session.export_preset("Mine").unwrap().maps.len();
    let ts = h.state().session.ts();
    let plan = h.state_mut().run("export.plan", json!({"preset": "Mine", "ts": ts})).unwrap();
    let exported = plan.as_array().unwrap().len();
    assert!(h.query_by_label(&format!("{exported} output maps will be exported.")).is_some());
    for create in ["Gray", "RGB", "R+G+B", "RGB+A", "R+G+B+A"] {
        assert!(h.query_by_label(create).is_some(), "{create}");
    }
    click(&mut h, "R+G+B+A");
    assert_eq!(h.state().session.export_preset("Mine").unwrap().maps.len(), maps + 1, "Create adds an output map");

    let info = h.get_by_label(egui_phosphor::regular::INFO).rect().center();
    h.hover_at(info);
    h.run_steps(30);
    assert!(h.query_by_label("Select the naming pattern used to display the input maps on this page.").is_some());
    assert!(h.query_by_label("Some input maps (channels) may be named differently depending on the shader used as a Texture Set.").is_some());
}

#[test]
fn the_presets_list_holds_the_sorted_templates_without_the_first_group() {
    let library = TemplateLibrary::new("list");
    let h = harness(&library);
    assert!(h.query_by_label("glTF PBR Metal Roughness").is_none());
    assert!(h.query_by_label("Document channels + Normal + AO (No Alpha)").is_none());
    let names = ["Blender (Principled BSDF)", "Mine", "Unreal Engine (Packed)"];
    let tops: Vec<f32> = names.iter().map(|n| h.get_all_by_label(n).next().unwrap().rect().top()).collect();
    assert!(tops.windows(2).all(|w| w[0] < w[1]), "sorted by name: {tops:?}");
}

#[test]
fn new_and_duplicate_use_the_reference_names() {
    let library = TemplateLibrary::new("names");
    let mut h = harness(&library);
    click(&mut h, "New template");
    assert_eq!(h.state().ui.export.preset, "new_export_preset");
    right_click(&mut h, "Mine");
    click(&mut h, "Duplicate");
    assert_eq!(h.state().ui.export.preset, "Mine_copy");
    assert!(h.state().session.export_preset("Mine_copy").is_some());
}

#[test]
fn the_right_click_menu_renames_and_removes_user_templates_only() {
    let library = TemplateLibrary::new("menu");
    let mut h = harness(&library);
    right_click(&mut h, "Mine");
    for item in ["Rename", "Duplicate", "Remove"] {
        assert!(!h.get_by_label(item).accesskit_node().is_disabled(), "{item}");
    }
    click(&mut h, "Rename");
    assert_eq!(h.state().ui.export.renaming.as_deref(), Some("Mine"));
    h.key_press(egui::Key::Escape);
    h.run_steps(4);

    right_click(&mut h, "Mine");
    click(&mut h, "Remove");
    assert!(matches!(&h.state().ui.export.ask, Some(texturecraft_ui_egui::export_templates::Ask::Delete(n)) if n == "Mine"));
    h.state_mut().ui.export.ask = None;
    h.run_steps(2);

    right_click(&mut h, "Unreal Engine (Packed)");
    assert!(h.get_by_label("Rename").accesskit_node().is_disabled(), "built-in templates are read-only");
    assert!(h.get_by_label("Remove").accesskit_node().is_disabled());
    assert!(!h.get_by_label("Duplicate").accesskit_node().is_disabled());
}

#[test]
fn f2_renames_the_selected_template_and_leaves_the_views_alone() {
    let library = TemplateLibrary::new("f2");
    let mut h = harness(&library);
    let layout = h.state().ui.layout;
    h.key_press(egui::Key::F2);
    h.run_steps(4);
    assert_eq!(h.state().ui.export.renaming.as_deref(), Some("Mine"));
    assert_eq!(h.state().ui.layout, layout, "F2 does not switch to the 3D view here");
}

#[test]
fn the_preview_ignores_a_texture_sets_output_map_settings() {
    let library = TemplateLibrary::new("preview");
    let mut h = harness(&library);

    // A two-map template on show, while the current texture set unticks map 2 of its own three-map template
    let mut two = h.state().session.export_preset("Mine").unwrap();
    two.name = "Two".into();
    two.maps.truncate(2);
    h.state_mut().run("export.preset.save", json!({ "preset": two })).unwrap();
    let own = "Unreal Engine (Packed)".to_string();
    let mut o = texturecraft_ui_egui::state::SetExport { preset: Some(own.clone()), maps_template: own, ..Default::default() };
    o.maps.insert(2, texturecraft_ui_egui::state::MapExport { enabled: false, file_type: None });
    let ts = h.state().session.ts();
    h.state_mut().ui.export.sets.insert(ts, o);
    h.state_mut().ui.export.preset = "Two".into();
    h.run_steps(4);
    assert!(h.query_by_label_contains("Not saved").is_none(), "the preview plans the template on show");
    for index in 0..2 {
        hover_name_field(&mut h, index);
        assert!(h.query_all_by_label_contains(egui_phosphor::regular::ARROW_RIGHT).next().is_some(), "map {index} shows its file");
    }
}

#[test]
fn texture_set_overrides_follow_a_renamed_template_and_drop_a_removed_one() {
    let library = TemplateLibrary::new("follow");
    let mut h = harness(&library);
    let mut o = texturecraft_ui_egui::state::SetExport { preset: Some("Mine".into()), maps_template: "Mine".into(), ..Default::default() };
    o.maps.insert(1, texturecraft_ui_egui::state::MapExport { enabled: false, file_type: None });

    // The override is there when the window opens
    h.state_mut().ui.windows.export = false;
    h.run_steps(2);
    h.state_mut().ui.export.sets.insert(0, o);
    let ui = &mut h.state_mut().ui;
    ui.windows.export = true;
    ui.export.tab = 1;
    ui.export.preset = "Mine".into();
    h.run_steps(4);

    // F2, a new name, Enter
    h.key_press(egui::Key::F2);
    h.run_steps(3);
    h.state_mut().ui.export.renaming = Some("Yours".into());
    h.key_press(egui::Key::Enter);
    h.run_steps(4);
    assert_eq!(h.state().ui.export.preset, "Yours");
    let o = h.state().ui.export.sets[&0].clone();
    assert_eq!((o.preset.as_deref(), o.maps_template.as_str()), (Some("Yours"), "Yours"), "the set keeps its template and map ticks");
    assert_eq!(o.maps.len(), 1);

    right_click(&mut h, "Yours");
    click(&mut h, "Remove");
    click(&mut h, "Delete");
    assert!(h.state().session.export_preset("Yours").is_none());
    assert!(!h.state().ui.export.sets.contains_key(&0), "the set exports with the global settings again");

    // Cancel goes back to the settings the window opened with, minus the removed template
    click(&mut h, "Cancel");
    assert!(h.state().ui.export.sets.values().all(|o| o.preset.as_deref() != Some("Yours") && o.preset.as_deref() != Some("Mine")));
}

#[test]
fn the_footer_counts_only_the_maps_the_texture_set_exports() {
    let library = TemplateLibrary::new("count");
    let mut h = harness(&library);

    // Two maps; the texture set has no sheen, so only roughness is written
    let channel = |c: &str| json!([["L", {"Channel": {"channel": c}}]]);
    let maps = json!([
        {"file": "$textureSet_rough", "format": "Png", "bit_depth": 8, "channels": channel("Roughness")},
        {"file": "$textureSet_sheen", "format": "Png", "bit_depth": 8, "channels": channel("SheenWeight")},
    ]);
    h.state_mut().run("export.preset.save", json!({"preset": {"name": "Mine", "maps": maps}})).unwrap();
    h.run_steps(4);
    let ts = h.state().session.ts();
    let plan = h.state_mut().run("export.plan", json!({"preset": "Mine", "ts": ts})).unwrap();
    assert_eq!(plan.as_array().unwrap().len(), 1, "the sample set has no sheen: {plan}");
    hover_name_field(&mut h, 1);
    assert!(h.query_all_by_label_contains("Not exported").next().is_some(), "the sheen row says so");
    assert!(h.query_by_label("1 output maps will be exported.").is_some());
    assert!(h.query_by_label("2 output maps will be exported.").is_none());
}

#[test]
fn per_map_settings_follow_their_map_when_the_template_reorders_it() {
    let library = TemplateLibrary::new("reorder");
    let mut h = harness(&library);

    // The first texture set leaves out the template's first map
    let off = MapExport { enabled: false, file_type: None };
    h.state_mut().ui.export.sets.insert(0, SetExport { maps_template: "Mine".into(), maps: [(0, off.clone())].into(), ..SetExport::default() });
    h.run_steps(2);
    let first = h.state().session.export_preset("Mine").unwrap().maps[0].file.clone();
    move_map(&mut h, 0, "Move down");
    assert_eq!(h.state().session.export_preset("Mine").unwrap().maps[1].file, first, "the map moved down");
    let maps = h.state().ui.export.sets.get(&0).map(|o| o.maps.clone()).unwrap_or_default();
    assert_eq!(maps, [(1, off)].into(), "the setting moved with its map");
}

#[test]
fn a_draft_in_the_same_order_as_the_external_save_remaps_from_that_order() {
    let library = TemplateLibrary::new("stale");
    let mut h = harness(&library);
    let off = MapExport { enabled: false, file_type: None };
    h.state_mut().ui.export.sets.insert(0, SetExport { maps_template: "Mine".into(), maps: [(0, off.clone())].into(), ..SetExport::default() });
    h.run_steps(2);
    let saved = h.state().session.export_preset("Mine").unwrap();
    let last = saved.maps.len() - 1;

    // An unfinished edit (an empty file name is not saved) moves the first map down
    h.state_mut().ui.export.draft.as_mut().unwrap().maps[last].file.clear();
    h.run_steps(2);
    move_map(&mut h, 0, "Move down");

    // An agent saves the template in that order meanwhile: overrides now count from it
    let mut agent = saved.clone();
    agent.maps.swap(0, 1);
    h.state_mut().run("export.preset.save", json!({ "preset": agent })).unwrap();
    h.run_steps(2);

    // Stabilize the layout after fixing the draft so the next click hits the move button
    h.state_mut().ui.export.draft.as_mut().unwrap().maps[last].file = saved.maps[last].file.clone();
    h.run_steps(2);
    move_map(&mut h, 0, "Move down");
    let now = h.state().session.export_preset("Mine").unwrap();
    assert_eq!(now.maps[1].file, saved.maps[1].file, "{:?}", now.maps.iter().map(|m| &m.file).collect::<Vec<_>>());
    let maps = h.state().ui.export.sets.get(&0).map(|o| o.maps.clone()).unwrap_or_default();
    assert_eq!(maps, [(1, off)].into(), "counted from the agent's order, not the edit's old base");
}

#[test]
fn a_retained_draft_keeps_overrides_with_maps_after_an_external_reorder() {
    let library = TemplateLibrary::new("different-external-order");
    let mut harness = harness(&library);
    let saved = harness.state().session.export_preset("Mine").unwrap();
    harness.state_mut().ui.export.draft.as_mut().unwrap().maps[0].file.clear();

    let mut external = saved.clone();
    external.maps.swap(0, 1);
    harness.state_mut().run("export.preset.save", json!({"preset": external})).unwrap();
    let disabled = MapExport { enabled: false, file_type: None };
    harness.state_mut().ui.export.sets.insert(0, SetExport { maps_template: "Mine".into(), maps: [(0, disabled.clone())].into(), ..SetExport::default() });
    harness.run_steps(3);

    // Repair the invalid first filename through the editor so the draft commits on that frame
    let first_field = name_field(&harness, 0);
    harness.get_all_by_role(egui::Role::TextInput).find(|field| field.rect() == first_field).unwrap().set_text_value_accesskit(&saved.maps[0].file);
    harness.run_steps(4);

    let current = harness.state().session.export_preset("Mine").unwrap();
    assert_eq!(current, saved, "the corrected draft was saved in its own order");
    assert_eq!(harness.state().ui.export.sets[&0].maps, [(1, disabled)].into());
}

#[test]
fn the_tab_row_holds_the_naming_pattern_and_the_bottom_bar_only_cancels() {
    let library = TemplateLibrary::new("bottom-bar");
    let h = harness(&library);

    let naming = h.get_by_label("Naming pattern").rect();
    let tabs = h.get_by_label("OUTPUT TEMPLATES").rect();
    let presets = h.get_by_label("Presets").rect();

    assert!(naming.center().y < presets.top() && (naming.center().y - tabs.center().y).abs() < tabs.height(), "on the tab row: {naming:?}");
    assert!(naming.left() > presets.right(), "at the right end");
    for absent in ["Include all", "Reset all"] {
        assert!(h.query_by_label(absent).is_none(), "{absent}");
    }
    assert!(h.query_by_label_contains("textures selected for export").is_none());
    for greyed in ["Save settings", "Export"] {
        // The rail at the right edge has an Export button of its own
        let button = h
            .get_all_by_role(egui::Role::Button)
            .filter(|b| b.rect().max.x < RAIL_LEFT_EDGE && b.accesskit_node().label().as_deref() == Some(greyed))
            .last()
            .unwrap();
        assert!(button.accesskit_node().is_disabled(), "{greyed}");
    }
    assert!(!h.get_all_by_label("Cancel").last().unwrap().accesskit_node().is_disabled());
}

#[test]
fn the_presets_row_holds_copy_minus_and_plus_without_tooltips() {
    let library = TemplateLibrary::new("preset-icons");
    let mut h = harness(&library);

    let presets = h.get_by_label("Presets").rect();
    let icons = ["Duplicate the selected template", "Delete the selected template", "New template"].map(|label| h.get_by_label(label).rect());

    assert!(icons.iter().all(|icon| (icon.center().y - presets.center().y).abs() < presets.height()), "on the Presets row: {icons:?}");
    assert!(icons.windows(2).all(|pair| pair[0].center().x < pair[1].center().x), "copy, minus, plus from left to right");
    h.hover_at(icons[2].center());
    h.run_steps(30);
    assert_eq!(h.query_all_by_label("New template").count(), 1, "no tooltip repeats the name");
    assert!(h.query_by_label("Import a template (.json)").is_none(), "no import button");

    right_click(&mut h, "Mine");
    assert!(h.query_by_label("Import a template (.json)").is_some(), "import and export stay in the right-click menu");
    assert!(h.query_by_label("Export the selected template (.json)").is_some());
}

#[test]
fn the_map_lists_stand_in_three_panes_with_the_reference_names() {
    let library = TemplateLibrary::new("panes");
    let h = harness(&library);

    let input = h.get_by_label("Input maps").rect();
    let mesh = h.get_by_label("Mesh maps").rect();
    let converted = h.get_by_label("Converted maps").rect();

    assert!(mesh.left() > input.right(), "Mesh maps stand in a column right of Input maps");
    assert!(converted.top() > mesh.bottom() && (converted.left() - mesh.left()).abs() < 2.0, "Converted maps stand below Mesh maps");
    for group in ["Base", "Transmission", "Subsurface", "Coat", "Fuzz/Sheen"] {
        assert!(h.query_all_by_label(group).next().is_some(), "group {group}");
    }
    for name in ["Base metalness", "Specular roughness", "Coat normal", "Fuzz weight", "World space normal", "Bent normals", "Normal DirectX", "f0"] {
        let entry = h.query_all_by_label(name).next().unwrap_or_else(|| panic!("{name}"));
        assert!(!entry.accesskit_node().is_disabled(), "{name} can be dragged");
    }
    for name in ["Base weight", "Coat tangent", "1/ior", "Glossiness²", "2D View"] {
        let entry = h.query_all_by_label(name).next().unwrap_or_else(|| panic!("{name}"));
        assert!(entry.accesskit_node().is_disabled(), "{name} is listed greyed");
    }
    for gone in ["Drag onto an output channel", "Texture set channels", "Constants", "World Space Normal"] {
        assert!(h.query_by_label(gone).is_none(), "{gone}");
    }
}

#[test]
fn a_map_dropped_on_a_channel_asks_which_of_its_channels_to_use() {
    let library = TemplateLibrary::new("drop");
    let mut h = harness(&library);
    click(&mut h, "R+G+B+A");
    let last = h.state().session.export_preset("Mine").unwrap().maps.len() - 1;

    let green = last_window_button(&h, "G").rect().center();
    drop_on(&mut h, SourceRef::Channel { channel: Channel::BaseColor, component: 0 }, green);

    assert!(h.query_by_label("From Base color").is_some(), "the popup names the dropped map");
    for choice in ["R Channel", "G Channel", "B Channel"] {
        assert!(!h.get_by_label(choice).accesskit_node().is_disabled(), "{choice}");
    }
    for greyed in ["Gray Channel", "A Channel"] {
        assert!(h.get_by_label(greyed).accesskit_node().is_disabled(), "{greyed}");
    }
    h.get_by_label("B Channel").click();
    h.run_steps(4);

    let map = h.state().session.export_preset("Mine").unwrap().maps[last].clone();
    assert!(map.channels.contains(&(Dest::G, SourceRef::Channel { channel: Channel::BaseColor, component: 2 })), "{:?}", map.channels);
    assert!(h.query_by_label("From Base color").is_none(), "the popup closed");
    assert!(h.query_all_by_label("G: Base color (B Channel)").next().is_some(), "the swatch names its map");
    assert_eq!(h.state().ui.export.active_map, Some(last), "the lists mark this map's sources");
}

#[test]
fn a_gray_map_dropped_on_a_channel_offers_its_gray_channel() {
    let library = TemplateLibrary::new("drop-gray");
    let mut h = harness(&library);
    click(&mut h, "R+G+B+A");

    let alpha = last_window_button(&h, "A").rect().center();
    drop_on(&mut h, SourceRef::Channel { channel: Channel::Metallic, component: 0 }, alpha);

    assert!(h.query_by_label("From Base metalness").is_some());
    assert!(!h.get_by_label("Gray Channel").accesskit_node().is_disabled());
    assert!(h.get_by_label("A Channel").accesskit_node().is_disabled());
    assert!(h.query_by_label("R Channel").is_none(), "a gray map has no R, G or B");
}

#[test]
fn a_click_on_a_channel_offers_clear_and_split_or_collapse_and_a_right_click_invert() {
    let library = TemplateLibrary::new("swatch-menu");
    let mut h = harness(&library);
    click(&mut h, "RGB");
    let last = h.state().session.export_preset("Mine").unwrap().maps.len() - 1;

    // The RGB swatch is the last "RGB" button, after the Create button of that name
    last_window_button(&h, "RGB").click();
    h.run_steps(4);
    assert!(h.query_by_label("Clear channel").is_some());
    h.get_by_label("Split/Collapse RGB").click();
    h.run_steps(4);
    assert_eq!(h.state().ui.export.draft_split.get(last), Some(&true), "R, G and B show apart");

    let green = last_window_button(&h, "G");
    let at = green.rect().center();
    green.click_secondary();
    h.run_steps(4);

    // The properties panel has Invert controls of its own: take the menu's, next to the swatch
    let invert = h.get_all_by_label("Invert").min_by(|a, b| a.rect().center().distance(at).total_cmp(&b.rect().center().distance(at))).unwrap();
    invert.click();
    h.run_steps(4);
    assert_eq!(h.state().session.export_preset("Mine").unwrap().maps[last].inverted, vec![Dest::G]);
}
