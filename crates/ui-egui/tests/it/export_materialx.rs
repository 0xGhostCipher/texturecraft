//! Export textures › SETTINGS: the Export MaterialX option (texturecraft #760). It passes
//! `materialX: true` to `file.exportTextures`, which writes an OpenPBR material per texture set.

use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use texturecraft_ui_egui::TextureCraftApp;
use texturecraft_ui_egui::state::ConsoleLine;

const OPENPBR: &str = "OpenPBR (separate maps)";

/// The Export textures window writing into a scratch folder of its own for `name`
fn harness(name: &str) -> (Harness<'static, TextureCraftApp>, std::path::PathBuf) {
    let session = texturecraft_engine::sample_session(256, 64).unwrap();
    let mut h = Harness::builder().with_size(egui::vec2(1600.0, 960.0)).build_eframe(|cc| TextureCraftApp::new(cc, session));
    h.state_mut().ui.prefs.language = "en".into();
    h.run_steps(4);
    let dir = std::env::temp_dir().join(format!("tc-export-materialx-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    h.state_mut().ui.export.dir = dir.display().to_string();
    h.state_mut().ui.windows.export = true;
    h.run_steps(4);
    (h, dir)
}

fn checkbox<'a>(h: &'a Harness<'static, TextureCraftApp>) -> egui_kittest::Node<'a> {
    h.get_by_role_and_label(egui::Role::CheckBox, "Export MaterialX")
}

/// Click Export and run frames until the export ends
fn export(h: &mut Harness<'static, TextureCraftApp>) {
    h.get_all_by_label("Export").last().unwrap().click();
    for _ in 0..64 {
        h.run_steps(1);
        if h.state().ui.export.job.is_none() {
            break;
        }
    }
    h.run_steps(2);
}

fn mtlx_files(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut files: Vec<_> = entries.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "mtlx")).collect();
    files.sort();
    files
}

#[test]
fn the_window_offers_export_materialx_off_by_default() {
    let (h, _dir) = harness("offer");
    assert!(!checkbox(&h).accesskit_node().is_disabled());
    assert!(!h.state().ui.export.materialx);
}

#[test]
fn ticking_export_materialx_writes_one_mtlx_per_texture_set() {
    let (mut h, dir) = harness("on");
    h.state_mut().ui.export.preset = OPENPBR.into();
    h.run_steps(3);
    checkbox(&h).click();
    h.run_steps(3);
    assert!(h.state().ui.export.materialx);
    export(&mut h);

    let files = mtlx_files(&dir);
    assert_eq!(files.len(), h.state().session.set_count(), "one material per texture set: {files:?}");
    for file in &files {
        let text = std::fs::read_to_string(file).unwrap();
        assert!(text.contains("<open_pbr_surface"), "{text}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn ticking_export_materialx_with_any_template_writes_the_openpbr_maps_beside_its_files() {
    let (mut h, dir) = harness("any-template");
    assert_eq!(h.state().ui.export.preset, texturecraft_engine::formats::DEFAULT_PRESET);
    checkbox(&h).click();
    h.run_steps(3);
    export(&mut h);

    let files = mtlx_files(&dir);
    assert_eq!(files.len(), h.state().session.set_count(), "one material per texture set: {files:?}");
    let mut references = 0;
    for file in &files {
        for reference in std::fs::read_to_string(file).unwrap().split("name=\"file\" type=\"filename\" value=\"").skip(1) {
            let name = reference.split('"').next().unwrap();
            assert!(dir.join(name).is_file(), "{name} is not next to the material");
            references += 1;
        }
    }
    assert!(references > 0);
    let written: Vec<String> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    assert!(written.iter().any(|f| f.ends_with("_baseColor.png")), "the template's own maps are still written: {written:?}");
    assert!(written.iter().any(|f| f.ends_with("_base_color.png")), "{written:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// The option is off by default, so a plain export writes no `.mtlx` even with the OpenPBR template
#[test]
fn no_mtlx_is_written_when_the_option_is_off() {
    let (mut h, dir) = harness("off");
    h.state_mut().ui.export.preset = OPENPBR.into();
    h.run_steps(3);
    export(&mut h);
    assert!(std::fs::read_dir(&dir).unwrap().next().is_some(), "the textures were written");
    assert!(mtlx_files(&dir).is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn restoring_the_default_parameters_unticks_export_materialx() {
    let (mut h, _dir) = harness("reset");
    checkbox(&h).click();
    h.run_steps(3);
    assert!(h.state().ui.export.materialx);
    h.get_by_label("Restore the default export parameters").click();
    h.run_steps(3);
    assert!(!h.state().ui.export.materialx);
}

/// Every texture set's export raises this warning: the material's normal map is read as OpenGL
const DIRECTX_WARNING: &str = "MaterialX: the normalmap node reads OpenGL (+Y) normals";

#[test]
fn a_warning_every_texture_set_raises_shows_once_in_the_console_and_the_log() {
    let (mut h, dir) = harness("warning");
    assert!(h.state().session.set_count() >= 2, "the warning has to come from more than one texture set");
    let e = &mut h.state_mut().ui.export;
    e.preset = OPENPBR.into();
    e.normal_format = "directX".into();
    h.run_steps(3);
    checkbox(&h).click();
    h.run_steps(3);
    export(&mut h);

    let lines: Vec<&str> = h
        .state()
        .ui
        .export
        .console
        .iter()
        .filter_map(|line| match line {
            ConsoleLine::Step(text) if text.starts_with("Warning: ") => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(lines.iter().filter(|line| line.contains(DIRECTX_WARNING)).count(), 1, "{lines:?}");
    assert_eq!(h.state().session.log.iter().filter(|line| line.contains(DIRECTX_WARNING)).count(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}
