//! The Resources Updater: compare image resources with the files they came from, read changed
//! files again and point a resource at another file. Each change is one undo step, and every
//! layer, mask and view using a changed resource recomposites.

use serde_json::{Value, json};
use texturecraft_core::ResourceSource;

use crate::commands::{CommandSpec, always, bad};
use crate::resource_state::ResourceState;
use crate::resource_status::ResourceStatus;
use crate::{Result, Session, cmd};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "resource.status",
            "Check resources on disk",
            [],
            None,
            "{} → {resources: [{name, status: outdated|missing|upToDate|noSource, path?, error?}] (outdated first), total, outdated}",
            always,
            |s, _| Ok(status(s))
        ),
        cmd!(
            "resource.update",
            "Update resources",
            [],
            None,
            "{names?: [resource name] (default: every outdated resource)} → {updated, failed: [{name, error}]} — reads their files again, one undo step",
            always,
            update
        ),
        cmd!(
            "resource.relink",
            "Relink resource",
            [],
            None,
            "{name, path — an image file (png, jpg, tga, tif, bmp, webp, exr, hdr)} → {name, path} — the resource takes the file's pixels and follows it from now on",
            always,
            relink
        ),
    ]
}

impl Session {
    /// The image resources with their files and how they compare, outdated first, then by name.
    /// Reads a file only when its size matches the stored bytes but its modification time does not.
    pub fn resource_states(&self) -> Vec<ResourceState> {
        let sources = &self.doc.project.resource_sources;
        let mut states: Vec<ResourceState> = self
            .doc
            .resources
            .iter()
            .filter(|(name, _)| self.images.contains_key(*name) || sources.contains_key(*name))
            .map(|(name, bytes)| {
                let source = sources.get(name);
                let (status, error) = source.map_or((ResourceStatus::NoSource, None), |source| compare_with_file(source, bytes));
                ResourceState { name: name.clone(), status, path: source.map(|source| source.path.clone()), error }
            })
            .collect();

        states.sort_by(|a, b| a.status.cmp(&b.status).then_with(|| a.name.cmp(&b.name)));
        states
    }

    /// Put reloaded files in place of their resources' bytes and pixels as one undo step
    #[cfg(not(target_arch = "wasm32"))]
    fn install_reloaded(&mut self, label: &str, files: Vec<crate::reloaded_file::ReloadedFile>) -> Result<Vec<String>> {
        if files.is_empty() {
            return Ok(Vec::new());
        }

        let decoded = self.edit(label, |session| {
            let mut decoded = Vec::with_capacity(files.len());
            for file in files {
                session.doc.resources.insert(file.name.clone(), std::sync::Arc::new(file.bytes));
                session.doc.project.resource_sources.insert(file.name.clone(), file.source);
                decoded.push((file.name, file.image));
            }

            Ok(decoded)
        })?;

        // The edit marked every set using a replaced resource for recompositing; the new pixels are read then
        let mut names = Vec::with_capacity(decoded.len());
        for (name, image) in decoded {
            if crate::fill_alpha::matches_saved_picture(&name, &image) {
                self.library_alphas.insert(name.clone());
            } else {
                self.library_alphas.remove(&name);
            }

            self.images.insert(name.clone(), crate::resource_cmds::decoded(image));
            names.push(name);
        }

        Ok(names)
    }
}

/// How `stored` (a resource's bytes) compares with the file `source` names now
#[cfg(not(target_arch = "wasm32"))]
fn compare_with_file(source: &ResourceSource, stored: &[u8]) -> (ResourceStatus, Option<String>) {
    let path = std::path::Path::new(&source.path);
    let metadata = match std::fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => metadata,
        Ok(_) => return (ResourceStatus::Missing, Some(format!("{} is not a regular file", source.path))),
        Err(error) => return (ResourceStatus::Missing, Some(format!("{}: {error}", source.path))),
    };

    if metadata.len() != stored.len() as u64 {
        return (ResourceStatus::Outdated, None);
    }

    let modified_ms = modified_ms(&metadata);
    if modified_ms != 0 && modified_ms == source.modified_ms {
        return (ResourceStatus::UpToDate, None);
    }

    // A newer time alone is not a change: the file may have been saved again unchanged
    match crate::resource_cmds::read_resource_file("resource.status", path, crate::resource_cmds::MAX_BYTES as u64) {
        Ok(bytes) if bytes == stored => (ResourceStatus::UpToDate, None),
        Ok(_) => (ResourceStatus::Outdated, None),
        Err(error) => (ResourceStatus::Missing, Some(error.to_string())),
    }
}

#[cfg(target_arch = "wasm32")]
fn compare_with_file(_: &ResourceSource, _: &[u8]) -> (ResourceStatus, Option<String>) {
    (ResourceStatus::Missing, Some("file paths are not available in the browser".to_string()))
}

/// A file's modification time in milliseconds since the Unix epoch (0 when the system has none)
#[cfg(not(target_arch = "wasm32"))]
fn modified_ms(metadata: &std::fs::Metadata) -> u64 {
    let since_epoch = metadata.modified().ok().and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok());
    since_epoch.map_or(0, |elapsed| u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX))
}

/// [`modified_ms`] of the file at `path` (0 when it cannot be read)
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn modified_ms_of(path: &std::path::Path) -> u64 {
    std::fs::metadata(path).map_or(0, |metadata| modified_ms(&metadata))
}

/// `path` made absolute against the working folder, so the record does not depend on where the app started
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn absolute_path(path: &str) -> String {
    std::path::absolute(path).ok().and_then(|absolute| absolute.to_str().map(str::to_string)).unwrap_or_else(|| path.to_string())
}

fn status(s: &Session) -> Value {
    let states = s.resource_states();
    let outdated = states.iter().filter(|state| state.status == ResourceStatus::Outdated).count();
    let resources: Vec<Value> = states
        .iter()
        .map(|state| {
            let mut row = json!({"name": state.name, "status": state.status.id()});
            if let Some(path) = &state.path {
                row["path"] = json!(path);
            }
            if let Some(error) = &state.error {
                row["error"] = json!(error);
            }

            row
        })
        .collect();

    json!({"resources": resources, "total": states.len(), "outdated": outdated})
}

#[cfg(not(target_arch = "wasm32"))]
fn update(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "resource.update";

    let names: Vec<String> = match p.get("names") {
        None | Some(Value::Null) => s.resource_states().into_iter().filter(|state| state.status == ResourceStatus::Outdated).map(|state| state.name).collect(),
        Some(Value::Array(names)) => {
            names.iter().map(|name| name.as_str().map(str::to_string).ok_or_else(|| bad(CMD, "`names` must be resource names"))).collect::<Result<_>>()?
        }
        Some(_) => return Err(bad(CMD, "`names` must be an array of resource names")),
    };

    // Every name is checked before any file is read
    for name in &names {
        if !s.doc.resources.contains_key(name) {
            return Err(bad(CMD, format!("no resource `{name}`")));
        }
        if !s.doc.project.resource_sources.contains_key(name) {
            return Err(bad(CMD, format!("`{name}` has no source file: relink it to a file first")));
        }
    }

    // A file that cannot be read fails alone; the others still update
    let (mut files, mut failed) = (Vec::new(), Vec::new());
    for name in names {
        let Some(path) = s.doc.project.resource_sources.get(&name).map(|source| source.path.clone()) else { continue };
        match crate::reloaded_file::ReloadedFile::read(CMD, name.clone(), &path) {
            // Unchanged contents need no undo step
            Ok(file) if s.doc.resources.get(&name).is_some_and(|stored| **stored == file.bytes) => {}
            Ok(file) => files.push(file),
            Err(error) => failed.push(json!({"name": name, "error": error.to_string()})),
        }
    }

    if files.is_empty() && !failed.is_empty() {
        let reasons: Vec<&str> = failed.iter().filter_map(|failure| failure["error"].as_str()).collect();
        return Err(bad(CMD, format!("nothing was updated: {}", reasons.join("; "))));
    }

    let updated = s.install_reloaded("Update resources", files)?;
    Ok(json!({"updated": updated, "failed": failed}))
}

#[cfg(not(target_arch = "wasm32"))]
fn relink(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "resource.relink";

    let name = crate::commands::strp(p, "name").ok_or_else(|| bad(CMD, "`name` is required"))?.to_string();
    let path = crate::commands::strp(p, "path").ok_or_else(|| bad(CMD, "`path` is required"))?;
    if !s.doc.resources.contains_key(&name) {
        return Err(bad(CMD, format!("no resource `{name}`")));
    }

    let file = crate::reloaded_file::ReloadedFile::read(CMD, name.clone(), path)?;
    let path = file.source.path.clone();
    s.install_reloaded("Relink resource", vec![file])?;

    Ok(json!({"name": name, "path": path}))
}

#[cfg(target_arch = "wasm32")]
fn update(_: &mut Session, _: &Value) -> Result<Value> {
    Err(bad("resource.update", "file paths are not available in the browser"))
}

#[cfg(target_arch = "wasm32")]
fn relink(_: &mut Session, _: &Value) -> Result<Value> {
    Err(bad("resource.relink", "file paths are not available in the browser"))
}
