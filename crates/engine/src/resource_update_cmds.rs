//! The Resources Updater: compare image resources with the files they came from, read changed
//! files again and point a resource at another file. Each change is one undo step, and every
//! layer, mask and view using a changed resource recomposites; a mesh map imported from a changed
//! file is resampled while it still holds what the file gave it.

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
            "{name: image resource, path — an image file (png, jpg, tga, tif, bmp, webp, exr, hdr)} → {name, path} — the resource takes the file's pixels and follows it from now on",
            always,
            relink
        ),
    ]
}

impl Session {
    /// The image resources with their files and how they compare, outdated first, then by name.
    /// A file is read only when its size matches the stored bytes but its modification time does
    /// not, and that verdict is kept until the file's size or time changes again.
    pub fn resource_states(&mut self) -> Vec<ResourceState> {
        let mut states = self.compare_resources();
        states.sort_by(|a, b| a.status.cmp(&b.status).then_with(|| a.name.cmp(&b.name)));
        states
    }

    /// Whether `name` is an image resource the Resources Updater lists
    fn is_image_resource(&self, name: &str) -> bool {
        self.doc.resources.contains_key(name) && (self.images.contains_key(name) || self.doc.project.resource_sources.contains_key(name))
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn compare_resources(&mut self) -> Vec<ResourceState> {
        let mut checks = std::mem::take(&mut self.resource_checks);
        let names: Vec<String> = self.doc.resources.keys().filter(|name| self.is_image_resource(name)).cloned().collect();
        let mut states = Vec::with_capacity(names.len());
        for name in names {
            let (Some(bytes), source) = (self.doc.resources.get(&name), self.doc.project.resource_sources.get(&name)) else { continue };
            let (status, error) = source.map_or((ResourceStatus::NoSource, None), |source| compare_with_file(&name, source, bytes, &mut checks));
            states.push(ResourceState { path: source.map(|source| source.path.clone()), name, status, error });
        }

        checks.retain(|name, _| self.doc.resources.contains_key(name));
        self.resource_checks = checks;
        states
    }

    #[cfg(target_arch = "wasm32")]
    fn compare_resources(&mut self) -> Vec<ResourceState> {
        let sources = &self.doc.project.resource_sources;
        let browser = "file paths are not available in the browser";
        let names = self.doc.resources.keys().filter(|name| self.is_image_resource(name));
        names
            .map(|name| match sources.get(name) {
                Some(source) => {
                    ResourceState { name: name.clone(), status: ResourceStatus::Missing, path: Some(source.path.clone()), error: Some(browser.into()) }
                }
                None => ResourceState { name: name.clone(), status: ResourceStatus::NoSource, path: None, error: None },
            })
            .collect()
    }

    /// Put reloaded files in place of their resources' bytes as one undo step, resample the mesh
    /// maps imported from them, then decode the stored bytes as undo, redo and reopening do
    #[cfg(not(target_arch = "wasm32"))]
    fn install_reloaded(&mut self, label: &str, files: Vec<crate::reloaded_file::ReloadedFile>) -> Result<Vec<String>> {
        use std::collections::{BTreeMap, BTreeSet};

        if files.is_empty() {
            return Ok(Vec::new());
        }

        let names: Vec<String> = files.iter().map(|file| file.name.clone()).collect();
        self.edit(label, |session| {
            let mut fresh = BTreeMap::new();
            let mut slots = BTreeSet::new();
            for file in files {
                slots.extend(file.source.mesh_map.map(|slot| (slot.texture_set, slot.map)));
                session.doc.resources.insert(file.name.clone(), std::sync::Arc::new(file.bytes));
                session.doc.project.resource_sources.insert(file.name.clone(), file.source);
                fresh.insert(file.name, crate::resource_cmds::decoded(file.image));
            }

            for (texture_set, map) in slots {
                session.refresh_mesh_map(texture_set, map, &fresh)?;
            }

            Ok(())
        })?;

        for name in &names {
            self.images.remove(name);
        }
        self.sync_images();
        Ok(names)
    }

    /// Resample mesh map `map` of set `texture_set` from the new pixels of the files it was
    /// imported from, unless a bake or another image has replaced it since (it no longer equals
    /// what the old pixels give)
    #[cfg(not(target_arch = "wasm32"))]
    fn refresh_mesh_map(
        &mut self,
        texture_set: usize,
        map: texturecraft_core::BakedMap,
        fresh: &std::collections::BTreeMap<String, std::sync::Arc<texturecraft_compose::ImageRes>>,
    ) -> Result<()> {
        const CMD: &str = "resource.update";

        let tiles: Vec<(u32, String)> = self
            .doc
            .project
            .resource_sources
            .iter()
            .filter_map(|(name, source)| {
                source.mesh_map.filter(|slot| slot.texture_set == texture_set && slot.map == map).map(|slot| (slot.tile, name.clone()))
            })
            .collect();
        let (mut before, mut after) = (std::collections::BTreeMap::new(), std::collections::BTreeMap::new());
        for (tile, name) in tiles {
            let Some(old) = self.images.get(&name) else { return Ok(()) };

            before.insert(tile, old.clone());
            after.insert(tile, fresh.get(&name).unwrap_or(old).clone());
        }

        let current = self.doc.baked.get(&texture_set).and_then(|baked| baked.maps.get(&map));
        let unchanged = crate::meshmap_cmds::resampled(self, CMD, texture_set, map, &before).ok();
        let from_these_files = current.zip(unchanged).is_some_and(|(current, (_, unchanged))| current.as_slice() == unchanged.as_slice());
        if from_these_files {
            crate::meshmap_cmds::use_images(self, CMD, texture_set, map, &after)?;
        }

        Ok(())
    }
}

/// How `stored` (a resource's bytes) compares with the file `source` names now, reusing the
/// remembered verdict in `checks` while the file's size and time stay the same
#[cfg(not(target_arch = "wasm32"))]
fn compare_with_file(
    name: &str,
    source: &ResourceSource,
    stored: &std::sync::Arc<Vec<u8>>,
    checks: &mut std::collections::HashMap<String, crate::resource_check::ResourceCheck>,
) -> (ResourceStatus, Option<String>) {
    let path = std::path::Path::new(&source.path);
    let metadata = match std::fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => metadata,
        Ok(_) => return (ResourceStatus::Missing, Some(format!("{} is not a regular file", source.path))),
        Err(error) => return (ResourceStatus::Missing, Some(format!("{}: {error}", source.path))),
    };

    let size = metadata.len();
    if size != stored.len() as u64 {
        return (ResourceStatus::Outdated, None);
    }

    let modified_ms = modified_ms(&metadata);
    if modified_ms != 0 && modified_ms == source.modified_ms {
        return (ResourceStatus::UpToDate, None);
    }

    if let Some(check) = checks.get(name)
        && check.holds_for(&source.path, size, modified_ms, stored)
    {
        return (check.status, None);
    }

    // A newer time alone is not a change: the file may have been saved again unchanged
    let status = match crate::resource_cmds::read_resource_file("resource.status", path, crate::resource_cmds::MAX_BYTES as u64) {
        Ok(bytes) if bytes == **stored => ResourceStatus::UpToDate,
        Ok(_) => ResourceStatus::Outdated,
        Err(error) => return (ResourceStatus::Missing, Some(error.to_string())),
    };
    let bytes = std::sync::Arc::downgrade(stored);
    checks.insert(name.to_string(), crate::resource_check::ResourceCheck { path: source.path.clone(), size, modified_ms, bytes, status });
    (status, None)
}

/// A file's modification time in milliseconds since the Unix epoch (0 when the system has none)
#[cfg(not(target_arch = "wasm32"))]
fn modified_ms(metadata: &std::fs::Metadata) -> u64 {
    let since_epoch = metadata.modified().ok().and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok());
    since_epoch.map_or(0, |elapsed| u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX))
}

/// The record for an image about to be read from `path`: its absolute path and its current
/// modification time, taken before reading so a write during the read still shows as a change
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn file_source(path: &str) -> ResourceSource {
    let absolute = std::path::absolute(path).ok().and_then(|absolute| absolute.to_str().map(str::to_string)).unwrap_or_else(|| path.to_string());
    let modified_ms = std::fs::metadata(path).map_or(0, |metadata| modified_ms(&metadata));
    ResourceSource { path: absolute, modified_ms, mesh_map: None }
}

/// The browser reads no paths; the record keeps the name it was given
#[cfg(target_arch = "wasm32")]
pub(crate) fn file_source(path: &str) -> ResourceSource {
    ResourceSource { path: path.to_string(), modified_ms: 0, mesh_map: None }
}

fn status(s: &mut Session) -> Value {
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
        let Some(previous) = s.doc.project.resource_sources.get(&name).cloned() else { continue };
        match crate::reloaded_file::ReloadedFile::read(CMD, name.clone(), &previous.path) {
            // Unchanged contents need no undo step
            Ok(file) if s.doc.resources.get(&name).is_some_and(|stored| **stored == file.bytes) => {}
            Ok(mut file) => {
                file.source.mesh_map = previous.mesh_map;
                files.push(file);
            }
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
    if !s.is_image_resource(&name) {
        return Err(bad(CMD, format!("no image resource `{name}`")));
    }

    // A relinked mesh map file still feeds its mesh map
    let mut file = crate::reloaded_file::ReloadedFile::read(CMD, name.clone(), path)?;
    file.source.mesh_map = s.doc.project.resource_sources.get(&name).and_then(|source| source.mesh_map);
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
