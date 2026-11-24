//! The Resources Updater: image resources remember their files, a changed file shows as outdated,
//! Update reads it again (one undo step, recompositing its layers) and Relink follows another file

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use texturecraft_engine::core::Channel;
use texturecraft_engine::geom::primitives;
use texturecraft_engine::{ResourceStatus, Session, execute};

const RESOLUTION: u32 = 64;
const SAMPLE_STRIDE: usize = 4;
const RED: [u8; 4] = [255, 0, 0, 255];
const GREEN: [u8; 4] = [0, 255, 0, 255];

/// A test folder removed when dropped
struct TempDir(PathBuf);

impl TempDir {
    fn new(test: &str) -> TempDir {
        let path = std::env::temp_dir().join(format!("tc-resource-updater-{test}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        TempDir(path)
    }

    fn file(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn session() -> Session {
    Session::new_project("t", primitives::sphere(1.0, 16, 8), "builtin:sphere", None, RESOLUTION).unwrap()
}

/// Write a `size` × `size` PNG of one colour (sizes differ between versions, so the file size changes too)
fn write_png(path: &Path, size: u32, color: [u8; 4]) {
    image::RgbaImage::from_pixel(size, size, image::Rgba(color)).save(path).unwrap();
}

fn text(path: &Path) -> &str {
    path.to_str().unwrap()
}

/// Import `path` and show it as a base colour fill; returns the resource name
fn import_as_fill(s: &mut Session, path: &Path) -> String {
    let name = execute(s, "resource.import", &json!({"path": text(path)})).unwrap()["name"].as_str().unwrap().to_string();
    execute(s, "layer.add.image", &json!({"name": name})).unwrap();
    s.update();
    name
}

fn base_colors(s: &Session) -> Vec<[f32; 3]> {
    let composite = &s.caches[s.ts()].composite;
    let samples = || (0..RESOLUTION).step_by(SAMPLE_STRIDE);
    samples().flat_map(|y| samples().map(move |x| (x, y))).map(|(x, y)| composite.value(Channel::BaseColor, x, y)).collect()
}

fn status_of(s: &Session, name: &str) -> ResourceStatus {
    s.resource_states().into_iter().find(|state| state.name == name).expect("the resource is listed").status
}

#[test]
fn a_changed_file_is_outdated_and_update_replaces_its_pixels_as_one_undo_step() {
    let dir = TempDir::new("update");
    let file = dir.file("bricks.png");
    write_png(&file, 4, RED);
    let mut s = session();
    let name = import_as_fill(&mut s, &file);
    let before = base_colors(&s);
    assert_eq!(status_of(&s, &name), ResourceStatus::UpToDate);

    write_png(&file, 8, GREEN);
    let status = execute(&mut s, "resource.status", &Value::Null).unwrap();

    assert_eq!(status["outdated"], json!(1));
    assert_eq!(status["resources"][0]["name"], json!(name));
    assert_eq!(status["resources"][0]["status"], json!("outdated"));

    let steps = s.history_labels().len();
    let updated = execute(&mut s, "resource.update", &json!({})).unwrap();
    s.update();
    let after = base_colors(&s);

    assert_eq!(updated["updated"], json!([name]));
    assert_eq!(s.history_labels().len(), steps + 1, "one undo step");
    assert_eq!(status_of(&s, &name), ResourceStatus::UpToDate);
    assert_eq!(s.images[&name].width, 8, "the new pixels are decoded");
    assert_ne!(after, before, "the fill using the resource recomposited");

    execute(&mut s, "edit.undo", &Value::Null).unwrap();
    s.update();

    assert_eq!(s.images[&name].width, 4, "undo brings back the old pixels under the same name");
    assert_eq!(base_colors(&s), before);
    assert_eq!(status_of(&s, &name), ResourceStatus::Outdated, "the file on disk is still the new one");

    execute(&mut s, "edit.redo", &Value::Null).unwrap();
    s.update();

    assert_eq!(s.images[&name].width, 8);
    assert_eq!(base_colors(&s), after);
}

#[test]
fn nothing_outdated_adds_no_undo_step() {
    let dir = TempDir::new("unchanged");
    let file = dir.file("same.png");
    write_png(&file, 4, RED);
    let mut s = session();
    import_as_fill(&mut s, &file);
    let steps = s.history_labels().len();

    let updated = execute(&mut s, "resource.update", &json!({})).unwrap();

    assert_eq!(updated["updated"], json!([]));
    assert_eq!(s.history_labels().len(), steps);
}

#[test]
fn a_missing_file_is_reported_and_updating_it_changes_nothing() {
    let dir = TempDir::new("missing");
    let file = dir.file("gone.png");
    write_png(&file, 4, RED);
    let mut s = session();
    let name = import_as_fill(&mut s, &file);
    std::fs::remove_file(&file).unwrap();
    let steps = s.history_labels().len();

    let state = s.resource_states().into_iter().find(|state| state.name == name).unwrap();
    let error = execute(&mut s, "resource.update", &json!({"names": [name]})).unwrap_err();

    assert_eq!(state.status, ResourceStatus::Missing);
    assert!(state.error.is_some(), "the reason is shown");
    assert!(error.to_string().contains("nothing was updated"), "{error}");
    assert_eq!(s.history_labels().len(), steps);
    assert_eq!(s.images[&name].width, 4, "the resource keeps its pixels");
}

#[test]
fn an_unreadable_file_fails_alone_while_the_others_update() {
    let dir = TempDir::new("partial");
    let (kept, gone) = (dir.file("kept.png"), dir.file("gone.png"));
    write_png(&kept, 4, RED);
    write_png(&gone, 4, RED);
    let mut s = session();
    let kept_name = import_as_fill(&mut s, &kept);
    let gone_name = import_as_fill(&mut s, &gone);
    write_png(&kept, 8, GREEN);
    std::fs::remove_file(&gone).unwrap();

    let result = execute(&mut s, "resource.update", &json!({"names": [kept_name, gone_name]})).unwrap();

    assert_eq!(result["updated"], json!([kept_name]));
    assert_eq!(result["failed"][0]["name"], json!(gone_name));
    assert_eq!(s.images[&kept_name].width, 8);
}

#[test]
fn relink_reads_another_file_and_follows_it_from_then_on() {
    let dir = TempDir::new("relink");
    let (first, second) = (dir.file("first.png"), dir.file("second.png"));
    write_png(&first, 4, RED);
    write_png(&second, 8, GREEN);
    let mut s = session();
    let name = import_as_fill(&mut s, &first);
    let before = base_colors(&s);
    let steps = s.history_labels().len();

    execute(&mut s, "resource.relink", &json!({"name": name, "path": text(&second)})).unwrap();
    s.update();

    assert_eq!(s.history_labels().len(), steps + 1, "one undo step");
    assert_eq!(s.images[&name].width, 8, "the resource keeps its name and takes the new file's pixels");
    assert_ne!(base_colors(&s), before);
    assert!(s.doc.project.resource_sources[&name].path.ends_with("second.png"));

    // The old file no longer matters, the new one does
    write_png(&first, 16, GREEN);
    assert_eq!(status_of(&s, &name), ResourceStatus::UpToDate);
    write_png(&second, 16, RED);
    assert_eq!(status_of(&s, &name), ResourceStatus::Outdated);

    execute(&mut s, "edit.undo", &Value::Null).unwrap();
    assert!(s.doc.project.resource_sources[&name].path.ends_with("first.png"), "undo restores the link");
}

#[test]
fn resources_without_a_file_cannot_update_and_a_reopened_project_keeps_its_links() {
    let dir = TempDir::new("reopen");
    let file = dir.file("kept.png");
    write_png(&file, 4, RED);
    let mut s = session();
    let linked = import_as_fill(&mut s, &file);
    let png = std::fs::read(&file).unwrap();
    let unlinked = s.import_resource_bytes("made.png", png).unwrap();

    assert_eq!(status_of(&s, &unlinked), ResourceStatus::NoSource);
    assert!(execute(&mut s, "resource.update", &json!({"names": [unlinked]})).is_err());

    let reopened = Session::from_project_bytes(&s.to_project_bytes().unwrap()).unwrap();
    write_png(&file, 8, GREEN);

    assert_eq!(status_of(&reopened, &linked), ResourceStatus::Outdated);
}

#[test]
fn a_removed_resource_forgets_its_file() {
    let dir = TempDir::new("remove");
    let file = dir.file("old.png");
    write_png(&file, 4, RED);
    let mut s = session();
    let name = execute(&mut s, "resource.import", &json!({"path": text(&file)})).unwrap()["name"].as_str().unwrap().to_string();

    execute(&mut s, "resource.remove", &json!({"name": name})).unwrap();
    let png = std::fs::read(&file).unwrap();
    let reused = s.import_resource_bytes("old.png", png).unwrap();

    assert_eq!(reused, name, "the name is free again");
    assert_eq!(status_of(&s, &reused), ResourceStatus::NoSource, "a new resource under the old name does not inherit the file");
}
