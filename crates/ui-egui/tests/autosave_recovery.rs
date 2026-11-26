//! Work recovered from an autosave copy survives the next autosave of the project the copy came from. This is a test
//! binary of its own because it points the process's data folder at a temporary one

#![cfg(not(target_arch = "wasm32"))]

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use egui_kittest::Harness;
use serde_json::json;
use texturecraft_engine::geom::primitives;
use texturecraft_engine::{Session, execute};
use texturecraft_ui_egui::{TextureCraftApp, control};

/// App time each frame advances by, so a few frames pass any autosave interval
const FRAME_SECONDS: f32 = 600.0;

/// How long the worker thread gets to write a copy
const WRITE_TIMEOUT: Duration = Duration::from_secs(60);

fn load(path: &Path) -> Option<Session> {
    texturecraft_engine::file_cmds::open_path(path.to_str().unwrap()).ok()
}

fn layers(session: &Session) -> usize {
    session.doc.project.layers.len()
}

fn start(session: Session) -> Harness<'static, TextureCraftApp> {
    Harness::builder()
        .with_step_dt(FRAME_SECONDS)
        .with_context_setup(texturecraft_ui_egui::framework::install_headless)
        .build_eframe(|cc| TextureCraftApp::new(cc, session))
}

fn add_layers(app: &mut TextureCraftApp, count: usize) {
    for _ in 0..count {
        execute(&mut app.session, "layer.add.paint", &json!({})).unwrap();
    }
}

/// Runs the app until a copy in `folder` holds a project of `count` layers, and returns it
fn autosave(harness: &mut Harness<'_, TextureCraftApp>, folder: &Path, count: usize) -> PathBuf {
    let started = Instant::now();
    loop {
        harness.step();
        let files = std::fs::read_dir(folder).map(|entries| entries.flatten().map(|entry| entry.path()).collect::<Vec<_>>()).unwrap_or_default();
        let found = files
            .into_iter()
            .find(|path| path.extension().is_some_and(|extension| extension == "tcraft") && load(path).is_some_and(|session| layers(&session) == count));
        if let Some(copy) = found {
            return copy;
        }

        assert!(started.elapsed() < WRITE_TIMEOUT, "autosave wrote no copy of {count} layers");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn recovered_work_survives_the_originals_next_autosave() {
    let data = std::env::temp_dir().join(format!("tc-autosave-recovery-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&data);
    std::fs::create_dir_all(&data).unwrap();
    texturecraft_ui_egui::set_data_root(data.clone());
    let folder = data.join("autosave");
    let own = data.join("hero.tcraft");
    let own_param = json!({ "path": own.to_str().unwrap() });

    let mut session = Session::new_project("hero", primitives::cube(1.0), "builtin:cube", None, 128).unwrap();
    execute(&mut session, "file.saveAs", &own_param).unwrap();
    let saved = layers(&session);

    let mut harness = start(session);
    add_layers(harness.state_mut(), 1);
    let copy = autosave(&mut harness, &folder, saved + 1);
    let copy_param = json!({ "path": copy.to_str().unwrap() });

    // Opening the copy gives an unsaved project: Save cannot write into it
    assert!(harness.state_mut().run("file.open", copy_param.clone()).is_some());
    assert_eq!(harness.state().session.path, None);
    add_layers(harness.state_mut(), 3);
    let recovered = layers(&harness.state().session);
    autosave(&mut harness, &folder, recovered);

    assert!(harness.state_mut().run("file.open", own_param).is_some());
    add_layers(harness.state_mut(), 2);
    assert_eq!(autosave(&mut harness, &folder, saved + 2), copy, "the original keeps its copy");
    autosave(&mut harness, &folder, recovered);

    // The control channel and a copy named on the command line open it the same way
    let ctx = harness.ctx.clone();
    let result = control::handle(harness.state_mut(), &ctx, "command.run", &json!({ "id": "file.open", "params": copy_param }));
    assert_eq!(result["ok"], true, "{result}");
    assert_eq!(harness.state().session.path, None);
    assert_eq!(start(load(&copy).unwrap()).state().session.path, None);

    drop(harness);
    std::fs::remove_dir_all(&data).unwrap();
}
