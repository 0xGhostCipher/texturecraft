//! The order of the menus, the tool shelf and the right bar, and what is enabled with no project open,
//! follow the reference app's measured layout. Items TextureCraft has no equivalent of are absent.

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use texturecraft_engine::Session;
use texturecraft_ui_egui::{TextureCraftApp, menus};

fn harness(session: Session) -> Harness<'static, TextureCraftApp> {
    let mut harness = Harness::builder().with_size(egui::vec2(1600.0, 960.0)).build_eframe(|cc| TextureCraftApp::new(cc, session));
    harness.state_mut().ui.prefs.language = "en".into();
    harness.run_steps(4);
    harness
}

fn buttons_in(harness: &Harness<'static, TextureCraftApp>, x: std::ops::Range<f32>) -> Vec<(f32, String, bool)> {
    let mut found: Vec<(f32, String, bool)> = harness
        .get_all_by_role(Role::Button)
        .filter(|button| x.contains(&button.rect().min.x))
        .filter_map(|button| {
            let node = button.accesskit_node();
            node.label().map(|label| (button.rect().min.y, label.to_string(), node.is_disabled()))
        })
        .collect();
    found.sort_by(|a, b| a.0.total_cmp(&b.0));
    found
}

fn named<'a>(found: &'a [(f32, String, bool)], name: &str) -> Option<&'a (f32, String, bool)> {
    found.iter().find(|(_, label, _)| label == name || label.starts_with(&format!("{name} (")))
}

#[test]
fn every_menu_lists_its_items_in_the_reference_order() {
    // Items the reference has and TextureCraft lacks are absent; TextureCraft's own sit where they fit
    let expected = [
        "File: New Project | Open | Recent Projects > Clear | - | Open Sample Project | - | Close | - | Save | Save As | More Save Options > Save As Template | More Save Options > Save and Reduce File Size | More Save Options > Save As Copy | Remove Unused Resources | - | Import Resources | Export Textures | Export Mesh | - | Quit",
        "Edit: Undo | Redo | Project Configuration | Reimport Mesh | Settings",
        "Mode: Bake Mesh Maps | Painting | Rendering",
        "Window: Views > Assets | Views > Baking Log | Views > Common Settings | Views > Display Settings | Views > Export | Views > History | Views > Layers | Views > Log | Views > Mesh Map Bakers | Views > Mesh Map Settings | Views > Properties | Views > Renderer Settings | Views > Shader Settings | Views > Texture Set List | Views > Texture Set Settings | Toolbars > Docks | Toolbars > Tools | - | Save UI Layout > Painting Mode | Save UI Layout > Rendering Mode | Save UI Layout > Baking Mode | Load UI Layout | - | Hide UI | Reset UI",
        "Viewport: Display Material | Display Next Channel | Display Previous Channel | Display Next Mesh Map | Display Previous Mesh Map | - | Show Entire Mesh/Path | - | Enable Quick Mask | Edit Quick Mask | Invert Quick Mask",
        "Help: Release Notes | Documentation | Shortcuts List | - | Discord | Report a Bug | Show Log | Export Log | - | Check for Updates | About TextureCraft",
    ];
    let actual: Vec<String> = menus::TOP
        .iter()
        .map(|top| {
            let items: Vec<String> = menus::CATALOG
                .iter()
                .filter(|item| item.menu == *top)
                .map(|item| match item.sub {
                    Some(sub) => format!("{sub} > {}", item.label),
                    None => item.label.to_string(),
                })
                .collect();
            format!("{top}: {}", items.join(" | "))
        })
        .collect();
    for (expected, actual) in expected.iter().zip(&actual) {
        assert_eq!(expected, actual);
    }
    assert_eq!(menus::TOP, ["File", "Edit", "Mode", "Window", "Viewport", "Help"], "the reference's top menus, minus the scripting ones");
}

#[test]
fn the_tool_shelf_keeps_the_reference_order() {
    let harness = harness(texturecraft_engine::sample_session(256, 64).unwrap());
    let shelf = buttons_in(&harness, 0.0..60.0);
    let order = ["Paint", "Eraser", "Path", "Projection", "Polygon fill", "Smudge", "Clone", "Material picker", "Geometry mask"];
    let row_positions: Vec<f32> = order.iter().map(|name| named(&shelf, name).unwrap_or_else(|| panic!("{name} on the shelf: {shelf:?}")).0).collect();

    assert!(row_positions.windows(2).all(|pair| pair[0] < pair[1]), "{row_positions:?}");
}

#[test]
fn the_right_bar_keeps_the_reference_order() {
    let harness = harness(texturecraft_engine::sample_session(256, 64).unwrap());
    let bar = buttons_in(&harness, 1562.0..1600.0);
    let order = ["Export", "Log", "Display Settings", "Shader Settings", "History"];
    let row_positions: Vec<f32> = order.iter().map(|name| named(&bar, name).unwrap_or_else(|| panic!("{name} on the bar: {bar:?}")).0).collect();

    assert!(row_positions.windows(2).all(|pair| pair[0] < pair[1]), "{row_positions:?}");
}

#[test]
fn the_blank_screen_enables_what_the_reference_enables() {
    let harness = harness(Session::closed().unwrap());
    let shelf = buttons_in(&harness, 0.0..60.0);
    for name in ["Paint", "Eraser", "Path", "Projection", "Polygon fill", "Smudge", "Clone", "Material picker", "Geometry mask"] {
        assert!(named(&shelf, name).unwrap().2, "{name} should be disabled with no project");
    }
    let bar = buttons_in(&harness, 1562.0..1600.0);
    for name in ["Export", "Log", "Display Settings", "Shader Settings", "History"] {
        assert!(!named(&bar, name).unwrap().2, "{name} on the right bar stays enabled");
    }

    // Top bar: layout stays on; camera type and the three modes are greyed, Painting shown as the current mode
    let top = buttons_in(&harness, 600.0..1562.0);
    let on = |name: &str| !named(&top, name).unwrap_or_else(|| panic!("{name}: {top:?}")).2;

    assert!(on("3D/2D views"), "the layout selector works without a project");
    assert!(!on("Perspective") && !on("Painting") && !on("Bake Mesh Maps") && !on("Rendering (path traced)"));

    let painting =
        harness.get_all_by_role(Role::Button).find(|button| button.accesskit_node().label().is_some_and(|label| label.starts_with("Painting"))).unwrap();

    assert_eq!(painting.accesskit_node().toggled(), Some(egui::accesskit::Toggled::True), "Painting is the shown mode");
}

#[test]
fn the_blank_screen_menus_match_the_measured_enabled_states() {
    let harness = harness(Session::closed().unwrap());
    let app = harness.state();
    let disabled = [
        "file.close",
        "file.save",
        "file.saveAs",
        "file.saveCopy",
        "file.removeUnused",
        "ui.export",
        "file.exportMesh",
        "edit.undo",
        "edit.redo",
        "ui.projectConfig",
        "file.reimportMesh",
        "ui.mode.bake",
        "ui.mode.paint",
        "ui.mode.render",
        "viewport.displayMaterial",
        "viewport.nextChannel",
        "viewport.previousChannel",
        "viewport.nextMeshMap",
        "viewport.previousMeshMap",
        "viewport.quickMask",
        "viewport.quickMask.edit",
        "viewport.quickMask.invert",
    ];
    for id in disabled {
        assert!(!menus::enabled(app, id), "{id} is disabled on the blank screen");
    }
    let enabled = [
        "ui.newProject",
        "file.open",
        "project.sample",
        "ui.quit",
        "ui.settings",
        "ui.view.assets",
        "ui.view.layers",
        "ui.view.properties",
        "ui.view.textureSetList",
        "ui.view.textureSetSettings",
        "ui.view.display",
        "ui.view.export",
        "ui.view.history",
        "ui.view.log",
        "ui.view.shader",
        "ui.hideUi",
        "ui.resetUi",
        "ui.layout.save.painting",
        "ui.layout.load",
        "viewport.frame",
        "ui.help.docs",
        "ui.help.shortcuts",
        "ui.help.bug",
        "ui.about",
    ];
    for id in enabled {
        assert!(menus::enabled(app, id), "{id} is enabled on the blank screen");
    }
    for id in ["ui.view.bakeSettings", "ui.view.bakeMaps", "ui.view.meshMapBakers"] {
        assert!(!menus::enabled(app, id), "{id} belongs to Baking Mode");
    }
}

#[test]
fn opening_a_project_enables_the_same_items() {
    let harness = harness(texturecraft_engine::sample_session(256, 64).unwrap());
    let app = harness.state();
    for id in [
        "file.close",
        "file.save",
        "file.saveAs",
        "file.saveCopy",
        "ui.export",
        "file.exportMesh",
        "ui.projectConfig",
        "ui.mode.bake",
        "ui.mode.paint",
        "ui.mode.render",
        "viewport.displayMaterial",
        "viewport.quickMask",
    ] {
        assert!(menus::enabled(app, id), "{id} is enabled with a project");
    }
}
