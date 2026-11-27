//! Image resources: import (PNG, JPEG, TGA, TIFF, BMP, WEBP, EXR, HDR), list, remove, and use as
//! fill sources.

use std::sync::Arc;

use serde_json::{Value, json};
use texturecraft_compose::ImageRes;
use texturecraft_core::{Channel, Fill, Layer, LayerKind, Source};
use texturecraft_formats::{MAX_RESOURCE_NAME_LEN, sanitize_resource_name};

use crate::commands::{CommandSpec, always, bad, channel_param, strp};
use crate::{EngineError, Result, Session, cmd};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "resource.import",
            "Import resources",
            ["File"],
            None,
            "{path — an image file (png, jpg, tga, tif, bmp, webp, exr, hdr), prefix?: put in front of the resource name (anything outside A-Z a-z 0-9 _ . - becomes _)}",
            always,
            import
        ),
        cmd!("resource.list", "List resources", [], None, "{}", always, |s, _| Ok(list(s))),
        cmd!("resource.remove", "Remove resource", [], None, "{name}", always, remove),
        cmd!("resource.export", "Export resource", [], None, "{name, path} — the resource's file as it was imported", always, export),
        cmd!(
            "resource.rename",
            "Rename resource",
            [],
            None,
            "{name, to} — everything that uses the image follows it; `to` keeps the old extension when it has none",
            always,
            rename
        ),
        cmd!(
            "resource.reload",
            "Reload resource",
            [],
            None,
            "{name} — read the image again from the file it was imported from in this session",
            always,
            reload
        ),
        cmd!(
            "resource.update",
            "Update resources used in project",
            [],
            None,
            "{} — read again every image imported from a file in this session whose file changed (one undo step); returns {updated: [names], failed: [{name, error}]}",
            always,
            update
        ),
        cmd!("file.removeUnused", "Remove unused resources...", ["File"], None, "{}", always, remove_unused),
        cmd!(
            "layer.add.image",
            "Add fill layer from image",
            [],
            None,
            "{name (exact resource name with extension or unique stem), channel?: baseColor, projection?: uv|triplanar|planar, scale?}",
            always,
            add_image_fill
        ),
    ]
}

/// Largest accepted resource file.
pub(crate) const MAX_BYTES: usize = 512 << 20;
pub(crate) const EXR_SIGNATURE: &[u8] = &[0x76, 0x2f, 0x31, 0x01];

pub(crate) fn decoded(d: texturecraft_formats::DecodedImage) -> Arc<ImageRes> {
    Arc::new(ImageRes::new(d.width, d.height, d.rgba, d.srgb))
}

impl Session {
    /// Add an image resource from bytes (one undo step); returns its unique, sanitized name
    pub fn import_resource_bytes(&mut self, file_name: &str, bytes: Vec<u8>) -> Result<String> {
        self.import_resource_bytes_prefixed("", file_name, bytes)
    }

    /// Like [`Self::import_resource_bytes`], with `prefix` put in front of the resource's name
    pub fn import_resource_bytes_prefixed(&mut self, prefix: &str, file_name: &str, bytes: Vec<u8>) -> Result<String> {
        self.edit("Import resource", |session| session.add_image_resource_prefixed(prefix, file_name, bytes, texturecraft_formats::MAX_RESOURCE_TEXELS))
    }

    /// Add an image within a decoded texel budget, inside the caller's edit
    pub(crate) fn add_image_resource(&mut self, file_name: &str, bytes: Vec<u8>, max_texels: u64) -> Result<String> {
        self.add_image_resource_prefixed("", file_name, bytes, max_texels)
    }

    fn add_image_resource_prefixed(&mut self, prefix: &str, file_name: &str, bytes: Vec<u8>, max_texels: u64) -> Result<String> {
        if bytes.len() > MAX_BYTES {
            return Err(EngineError::Other(format!("{file_name} is larger than {} MB", MAX_BYTES >> 20)));
        }

        let image = texturecraft_formats::decode_image_within(&bytes, max_texels).map_err(|error| EngineError::Other(format!("{file_name}: {error}")))?;
        let leaf = std::path::Path::new(file_name).file_name().and_then(|name| name.to_str()).unwrap_or("image");
        let mut base = sanitize_resource_name(leaf);

        // The file name has priority over the optional prefix
        let prefix_room = MAX_RESOURCE_NAME_LEN - base.len();
        let prefix = prefix.trim();
        if !prefix.is_empty() && prefix_room > 0 {
            let mut prefix = sanitize_resource_name(prefix);
            prefix.truncate(prefix_room);
            base.insert_str(0, &prefix);
        }

        let (stem, extension) = base.rfind('.').map_or((base.as_str(), ""), |extension_start| base.split_at(extension_start));
        let mut name = base.clone();
        let mut duplicate_index = 2;
        while self.doc.resources.contains_key(&name) || crate::fill_alpha::library_alpha_of(&name).is_some() {
            let discriminator = format!("{duplicate_index}_");
            let available = MAX_RESOURCE_NAME_LEN - discriminator.len();

            // Leave room for a stem even for names that end in an unusually long extension
            let (stem, extension) = if extension.len() < available { (stem, extension) } else { (base.as_str(), "") };
            let stem = stem.get(..stem.len().min(available - extension.len())).unwrap_or(stem);
            name = format!("{discriminator}{stem}{extension}");
            duplicate_index += 1;
        }

        self.doc.resources.insert(name.clone(), Arc::new(bytes));
        self.images.insert(name.clone(), decoded(image));
        Ok(name)
    }

    /// The file image resource `name` was imported from in this session (what `resource.reload` reads again)
    pub fn resource_source(&self, name: &str) -> Option<&std::path::Path> {
        let bytes = self.doc.resources.get(name)?;
        self.resource_sources.iter().rev().find(|(source, _)| std::ptr::eq(source.as_ptr(), Arc::as_ptr(bytes))).map(|(_, path)| path.as_path())
    }

    /// Remembers that image resource `name`, with the bytes it holds now, came from the file at `path`
    #[cfg(not(target_arch = "wasm32"))]
    fn remember_resource_source(&mut self, name: &str, path: &std::path::Path) {
        self.resource_sources.retain(|(source, _)| source.strong_count() > 0);
        if let Some(bytes) = self.doc.resources.get(name) {
            self.resource_sources.push((Arc::downgrade(bytes), path.to_path_buf()));
        }
    }

    /// Decode resources that are not decoded yet and forget removed ones (after undo, redo, load).
    pub fn sync_images(&mut self) {
        self.images.retain(|name, _| self.doc.resources.contains_key(name));
        self.library_alphas.retain(|name| self.images.contains_key(name));

        let mut linked_packages = std::collections::HashSet::new();
        let mut linked_images = std::collections::HashMap::new();
        for layer in self.doc.project.layers.values() {
            let LayerKind::Fill(Fill { link: Some(link), sources, .. }) = &layer.kind else { continue };

            linked_packages.insert(link.package.as_str());
            for source in sources.values() {
                if let Source::Image { name, .. } = source {
                    linked_images.entry(name.as_str()).and_modify(|size: &mut u32| *size = (*size).max(link.size)).or_insert(link.size);
                }
            }
        }

        for (name, bytes) in &self.doc.resources {
            if self.images.contains_key(name) || linked_packages.contains(name.as_str()) {
                continue;
            }

            // EXR may need a full float decode buffer even when encoded project bytes are small
            let image = match linked_images.get(name.as_str()) {
                Some(&size) if size > 0 && bytes.starts_with(EXR_SIGNATURE) => {
                    let texels = (u64::from(size) * u64::from(size)).min(texturecraft_formats::MAX_RESOURCE_TEXELS);
                    let decoder_bytes = texels * std::mem::size_of::<[f32; 4]>() as u64;
                    texturecraft_formats::decode_cooked_image(bytes, [size; 2], decoder_bytes).or_else(|_| texturecraft_formats::decode_image(bytes))
                }
                _ => texturecraft_formats::decode_image(bytes),
            };
            match image {
                Ok(image) => {
                    if crate::fill_alpha::matches_saved_picture(name, &image) {
                        self.library_alphas.insert(name.clone());
                    }

                    self.images.insert(name.clone(), decoded(image));
                }
                Err(error) => self.log.warn(crate::log::LIBRARY_SOURCE, format!("resource {name}: {error}")),
            }
        }

        self.drop_missing_image_view();
    }

    /// The "Other..." view cannot show an image that is gone (removed or renamed): the material comes back
    pub(crate) fn drop_missing_image_view(&mut self) {
        if self.view == crate::ViewMode::Image && !self.view_image.as_ref().is_some_and(|name| self.images.contains_key(name)) {
            self.view = crate::ViewMode::Material;
            self.view_image = None;
            self.revision = self.revision.wrapping_add(1);
        }
    }
}

fn list(s: &Session) -> Value {
    json!(
        s.doc
            .resources
            .iter()
            .map(|(k, v)| {
                let (w, h) = s.images.get(k).map(|i| (i.width, i.height)).unwrap_or((0, 0));
                json!({"name": k, "bytes": v.len(), "width": w, "height": h})
            })
            .collect::<Vec<_>>()
    )
}

#[cfg(not(target_arch = "wasm32"))]
fn import(s: &mut Session, p: &Value) -> Result<Value> {
    let path = strp(p, "path").ok_or_else(|| bad("resource.import", "`path` is required"))?;
    let bytes = read_image_file("resource.import", path, MAX_BYTES as u64)?;
    let name = s.import_resource_bytes_prefixed(strp(p, "prefix").unwrap_or(""), path, bytes)?;
    s.remember_resource_source(&name, std::path::Path::new(path));
    Ok(json!({ "name": name }))
}

/// Read a regular image file within both the per-file cap and the caller's remaining byte budget
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn read_image_file(cmd: &str, path: &str, max_bytes: u64) -> Result<Vec<u8>> {
    read_resource_file(cmd, std::path::Path::new(path), max_bytes.min(MAX_BYTES as u64))
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn open_resource_file(cmd: &str, path: &std::path::Path) -> Result<std::fs::File> {
    let display = path.display();
    let read_error = |error| bad(cmd, format!("cannot read {display}: {error}"));

    // Pipes can block on open, and devices may never reach EOF
    if !std::fs::metadata(path).map_err(read_error)?.is_file() {
        return Err(bad(cmd, format!("{display} is not a regular file")));
    }

    #[cfg(unix)]
    let file = {
        use rustix::fs::{Mode, OFlags, open};

        // Follow user-selected symlinks without waiting on a substituted pipe
        let descriptor = open(path, OFlags::RDONLY | OFlags::NONBLOCK | OFlags::CLOEXEC, Mode::empty()).map_err(|error| read_error(error.into()))?;
        std::fs::File::from(descriptor)
    };
    #[cfg(not(unix))]
    let file = std::fs::File::open(path).map_err(read_error)?;

    if !file.metadata().map_err(read_error)?.is_file() {
        return Err(bad(cmd, format!("{display} is not a regular file")));
    }

    Ok(file)
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn read_resource_file(cmd: &str, path: &std::path::Path, max_bytes: u64) -> Result<Vec<u8>> {
    use std::io::Read;

    let display = path.display();
    let read_error = |error| bad(cmd, format!("cannot read {display}: {error}"));
    let too_large = || bad(cmd, format!("{display} is too large (limit {max_bytes} bytes)"));
    let mut file = open_resource_file(cmd, path)?;
    let metadata = file.metadata().map_err(read_error)?;
    if metadata.len() > max_bytes {
        return Err(too_large());
    }

    let mut bytes = Vec::new();
    let capacity = usize::try_from(metadata.len()).map_err(|_| too_large())?;
    bytes.try_reserve_exact(capacity).map_err(|error| bad(cmd, format!("cannot allocate {display}: {error}")))?;

    // A growing file must not trigger allocations beyond the checked size
    file.by_ref().take(metadata.len()).read_to_end(&mut bytes).map_err(read_error)?;
    if file.read(&mut [0u8]).map_err(read_error)? != 0 {
        return Err(too_large());
    }

    Ok(bytes)
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn read_image_file(cmd: &str, _: &str, _: u64) -> Result<Vec<u8>> {
    Err(bad(cmd, "file paths are not available in the browser"))
}

#[cfg(target_arch = "wasm32")]
fn import(_: &mut Session, _: &Value) -> Result<Value> {
    Err(EngineError::Other("file paths are not available in the browser; use File > Import resources".into()))
}

fn remove(s: &mut Session, p: &Value) -> Result<Value> {
    let name = strp(p, "name").ok_or_else(|| bad("resource.remove", "`name` is required"))?.to_string();
    if !s.doc.resources.contains_key(&name) {
        return Err(bad("resource.remove", format!("no resource `{name}`")));
    }

    s.edit("Remove resource", |s| {
        s.doc.resources.remove(&name);
        Ok(())
    })?;
    s.sync_images();
    Ok(Value::Null)
}

/// Writes resource `name`'s bytes, as imported, to `path`
#[cfg(not(target_arch = "wasm32"))]
fn export(s: &mut Session, p: &Value) -> Result<Value> {
    let command = "resource.export";
    let name = strp(p, "name").ok_or_else(|| bad(command, "`name` is required"))?;
    let path = strp(p, "path").ok_or_else(|| bad(command, "`path` is required"))?;
    let bytes = s.doc.resources.get(name).ok_or_else(|| bad(command, format!("no resource `{name}`")))?;

    std::fs::write(path, bytes.as_slice()).map_err(|error| bad(command, format!("cannot write {path}: {error}")))?;

    Ok(json!({ "path": path }))
}

#[cfg(target_arch = "wasm32")]
fn export(_: &mut Session, _: &Value) -> Result<Value> {
    Err(bad("resource.export", "file paths are not available in the browser"))
}

/// Renames image resource `name` to `to` and every reference to it, in one undo step
fn rename(s: &mut Session, p: &Value) -> Result<Value> {
    let command = "resource.rename";
    let name = strp(p, "name").ok_or_else(|| bad(command, "`name` is required"))?.to_string();
    let to = strp(p, "to").ok_or_else(|| bad(command, "`to` is required"))?.trim();
    if !s.doc.resources.contains_key(&name) {
        return Err(bad(command, format!("no resource `{name}`")));
    }

    let mut to = sanitize_resource_name(to);
    if std::path::Path::new(&to).extension().is_none()
        && let Some(extension) = std::path::Path::new(&name).extension().and_then(|extension| extension.to_str())
    {
        // The stem gives way, so a long name keeps its extension
        let stem_room = MAX_RESOURCE_NAME_LEN.saturating_sub(extension.len() + 1);
        to.truncate(stem_room);
        to = format!("{to}.{extension}");
    }
    if to == name {
        return Ok(json!({ "name": name }));
    }
    if s.doc.resources.contains_key(&to) || crate::fill_alpha::library_alpha_of(&to).is_some() {
        return Err(bad(command, format!("a resource named `{to}` exists already")));
    }

    s.edit("Rename resource", |s| {
        let bytes = s.doc.resources.remove(&name).ok_or_else(|| bad(command, format!("no resource `{name}`")))?;
        s.doc.resources.insert(to.clone(), bytes);
        rename_references(&mut s.doc.project, &name, &to);

        // Refuse rather than leave a reference this rename does not know about pointing at nothing
        if project_images(&s.doc.project).contains(&name) {
            return Err(bad(command, format!("`{name}` is used somewhere it can't be renamed")));
        }

        Ok(())
    })?;

    for tool_image in s.tool_images_mut().filter(|tool_image| **tool_image == name) {
        tool_image.clone_from(&to);
    }

    // The decoded pixels are the same: they move instead of decoding again
    if let Some(image) = s.images.remove(&name) {
        s.images.insert(to.clone(), image);
    }
    if s.library_alphas.remove(&name) {
        s.library_alphas.insert(to.clone());
    }
    s.path_cache = None;
    s.sync_images();
    Ok(json!({ "name": to }))
}

/// Points every layer, effect, stored path and the environment that read image resource `from` at `to`
fn rename_references(project: &mut texturecraft_core::Project, from: &str, to: &str) {
    for layer in project.layers.values_mut() {
        let fill_images: Vec<&mut String> = match &mut layer.kind {
            LayerKind::Fill(fill) => fill.images_mut().collect(),
            _ => Vec::new(),
        };
        let effects = layer.mask.iter_mut().flat_map(|mask| mask.effects.iter_mut()).chain(&mut layer.content_effects);
        let mut images = fill_images;
        for effect in effects {
            crate::path_tool::rename_path_images(&mut effect.kind, from, to);
            images.extend(effect.images_mut());
        }

        for image in images.into_iter().filter(|image| *image == from) {
            *image = to.to_string();
        }
    }

    if project.display.environment_resource() == Some(from) {
        project.display.environment = format!("{}{to}", texturecraft_core::project::ENV_RESOURCE_PREFIX);
    }
}

/// Reads image resource `name` again from the file this session imported it from (one undo step)
#[cfg(not(target_arch = "wasm32"))]
fn reload(s: &mut Session, p: &Value) -> Result<Value> {
    let command = "resource.reload";
    let name = strp(p, "name").ok_or_else(|| bad(command, "`name` is required"))?.to_string();
    if !s.doc.resources.contains_key(&name) {
        return Err(bad(command, format!("no resource `{name}`")));
    }

    let path = s
        .resource_source(&name)
        .map(std::path::Path::to_path_buf)
        .ok_or_else(|| bad(command, format!("`{name}` was imported before this session: no file to reload from")))?;
    let (bytes, image) = read_source(command, &path)?;
    s.edit("Reload resource", |s| {
        s.doc.resources.insert(name.clone(), Arc::new(bytes));
        Ok(())
    })?;

    s.remember_resource_source(&name, &path);
    s.show_reloaded(name.clone(), image);
    Ok(json!({ "name": name }))
}

/// Reads again, in one undo step, every image whose source file no longer holds the bytes the project has; nothing changed adds no step
#[cfg(not(target_arch = "wasm32"))]
fn update(s: &mut Session, _: &Value) -> Result<Value> {
    let command = "resource.update";
    let mut changed = Vec::new();
    let mut failed = Vec::new();
    for (name, held) in &s.doc.resources {
        let Some(path) = s.resource_source(name) else { continue };
        // Only a file whose bytes changed is decoded
        let read = read_resource_file(command, path, MAX_BYTES as u64)
            .and_then(|bytes| if bytes == **held { Ok(None) } else { decode_source(command, path, &bytes).map(|image| Some((bytes, image))) });
        match read {
            Ok(None) => {}
            Ok(Some((bytes, image))) => changed.push((name.clone(), path.to_path_buf(), bytes, image)),
            Err(error) => failed.push(json!({ "name": name, "error": error.to_string() })),
        }
    }

    if !changed.is_empty() {
        s.edit("Update resources", |s| {
            for (name, _, bytes, _) in &mut changed {
                s.doc.resources.insert(name.clone(), Arc::new(std::mem::take(bytes)));
            }
            Ok(())
        })?;
    }

    let updated: Vec<String> = changed.iter().map(|(name, _, _, _)| name.clone()).collect();
    for (name, path, _, image) in changed {
        s.remember_resource_source(&name, &path);
        s.show_reloaded(name, image);
    }
    Ok(json!({ "updated": updated, "failed": failed }))
}

/// The bytes of the image file at `path` and its decoded pixels
#[cfg(not(target_arch = "wasm32"))]
fn read_source(command: &str, path: &std::path::Path) -> Result<(Vec<u8>, texturecraft_formats::DecodedImage)> {
    let bytes = read_resource_file(command, path, MAX_BYTES as u64)?;
    let image = decode_source(command, path, &bytes)?;
    Ok((bytes, image))
}

/// The pixels of `bytes` read from the image file at `path`
#[cfg(not(target_arch = "wasm32"))]
fn decode_source(command: &str, path: &std::path::Path, bytes: &[u8]) -> Result<texturecraft_formats::DecodedImage> {
    texturecraft_formats::decode_image_within(bytes, texturecraft_formats::MAX_RESOURCE_TEXELS)
        .map_err(|error| bad(command, format!("{}: {error}", path.display())))
}

#[cfg(not(target_arch = "wasm32"))]
impl Session {
    /// Shows the pixels of image `name` read again from its file, still a saved brush picture when it matches one
    fn show_reloaded(&mut self, name: String, image: texturecraft_formats::DecodedImage) {
        self.library_alphas.remove(&name);
        if crate::fill_alpha::matches_saved_picture(&name, &image) {
            self.library_alphas.insert(name.clone());
        }
        self.images.insert(name, decoded(image));
    }
}

#[cfg(target_arch = "wasm32")]
fn reload(_: &mut Session, _: &Value) -> Result<Value> {
    Err(bad("resource.reload", "file paths are not available in the browser"))
}

#[cfg(target_arch = "wasm32")]
fn update(_: &mut Session, _: &Value) -> Result<Value> {
    Err(bad("resource.update", "file paths are not available in the browser"))
}

/// Every image resource a layer, mask effect, stored path or the environment of `project` uses
fn project_images(project: &texturecraft_core::Project) -> Vec<String> {
    let mut used = Vec::new();
    for set in &project.texture_sets {
        for root in &set.stack {
            crate::material_lib::images_used(project, *root, &mut used, 0);
        }
    }
    used.extend(project.display.environment_resource().map(str::to_string));
    used
}

/// [`project_images`] and the images the brush alpha, the projection stencil and the mask brush use
fn images_in_use(s: &Session) -> Vec<String> {
    let mut used = project_images(&s.doc.project);
    used.extend(s.tool.stencil.as_ref().map(|st| st.image.clone()));
    used.extend(s.tool.brush.alpha_image.clone());
    used.extend(s.tool.mask_image.clone());
    used
}

/// Drop resources no layer, mask effect, the brush alpha, the projection stencil, the mask brush or the environment uses (one undo step)
fn remove_unused(s: &mut Session, _: &Value) -> Result<Value> {
    let used = images_in_use(s);
    let unused: Vec<String> = s.doc.resources.keys().filter(|k| !used.contains(k)).cloned().collect();
    if !unused.is_empty() {
        s.edit("Remove unused resources", |s| {
            s.doc.resources.retain(|k, _| !unused.contains(k));
            Ok(())
        })?;
        s.sync_images();
    }
    Ok(json!({ "removed": unused }))
}

#[cfg(not(target_arch = "wasm32"))]
const IMPORT_HINT: &str = "resource.import";
#[cfg(target_arch = "wasm32")]
const IMPORT_HINT: &str = "File > Import resources";

/// Resolve an image resource by exact name or unique file stem.
fn resolve_resource_name(s: &Session, name: &str) -> Result<String> {
    if s.images.contains_key(name) {
        return Ok(name.to_string());
    }
    let matches: Vec<&String> = s.images.keys().filter(|k| std::path::Path::new(k).file_stem().and_then(|stem| stem.to_str()) == Some(name)).collect();
    match matches.as_slice() {
        [single] => Ok((*single).clone()),
        [] => {
            let available: Vec<&str> = s.images.keys().map(String::as_str).collect();
            let hint =
                if available.is_empty() { format!(" (no resources imported: use {IMPORT_HINT})") } else { format!(" (available: {})", available.join(", ")) };
            Err(bad("layer.add.image", format!("no resource `{name}`{hint}")))
        }
        multiple => {
            let matches_str = multiple.iter().map(|m| format!("`{m}`")).collect::<Vec<_>>().join(", ");
            Err(bad("layer.add.image", format!("ambiguous resource `{name}` matches {matches_str}: give the full name with extension")))
        }
    }
}

fn add_image_fill(s: &mut Session, p: &Value) -> Result<Value> {
    let raw_name = strp(p, "name").ok_or_else(|| bad("layer.add.image", "`name` is required"))?;
    let name = resolve_resource_name(s, raw_name)?;
    let ch = channel_param("layer.add.image", p, "channel")?.unwrap_or(Channel::BaseColor);
    let projection = crate::layer_cmds::projection_from(p, "layer.add.image", None, || crate::warp_cmds::fitted_frame(&s.mesh))?;
    let ts = s.ts();
    let label = std::path::Path::new(&name).file_stem().and_then(|n| n.to_str()).unwrap_or("Image").to_string();
    let layer_name = s.doc.project.next_layer_name(ts, &label);
    let id = s.edit("Add image fill", |s| {
        let id = s.doc.project.alloc_id();
        let mut fill = Fill { projection, ..Fill::default() };
        fill.sources.insert(ch, Source::image(name.clone()));
        crate::layer_cmds::insert_selected(s, Layer::new(id, layer_name, LayerKind::Fill(fill), &[ch]))
    })?;
    Ok(json!({ "layer": id.0 }))
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[test]
    fn export_writes_the_imported_bytes() {
        let mut session = crate::sample_session(128, 32).unwrap();
        let bytes = texturecraft_formats::gray16_png(1, 1, [0.5].into_iter()).unwrap();
        let name = session.import_resource_bytes("icon.png", bytes.clone()).unwrap();
        let path = std::env::temp_dir().join(format!("texturecraft-export-resource-{}.png", std::process::id()));

        crate::commands::execute(&mut session, "resource.export", &json!({"name": name, "path": path.to_str().unwrap()})).unwrap();

        assert_eq!(std::fs::read(&path).unwrap(), bytes, "the file is the resource as imported");
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn a_renamed_image_stays_in_use_everywhere_it_was() {
        let mut session = crate::sample_session(128, 32).unwrap();
        let png = texturecraft_formats::gray16_png(1, 1, [0.5].into_iter()).unwrap();
        let name = session.import_resource_bytes("photo.png", png).unwrap();
        crate::execute(&mut session, "layer.add.image", &json!({"name": name})).unwrap();
        session.tool.brush.alpha_image = Some(name.clone());

        let renamed = crate::execute(&mut session, "resource.rename", &json!({"name": name, "to": "stone"})).unwrap()["name"].as_str().unwrap().to_string();

        assert_ne!(renamed, name);
        assert!(renamed.ends_with(".png"), "the extension stays: {renamed}");
        let used = images_in_use(&session);
        assert!(used.contains(&renamed) && !used.contains(&name), "every use follows the rename: {used:?}");
        assert!(session.images.contains_key(&renamed) && !session.images.contains_key(&name));

        session.undo().unwrap();

        assert!(project_images(&session.doc.project).contains(&name), "undo brings the old name back");
        assert!(session.images.contains_key(&name));
        assert_eq!(session.tool.brush.alpha_image.as_ref(), Some(&name), "the brush alpha follows the undo");

        session.redo().unwrap();

        assert_eq!(session.tool.brush.alpha_image.as_ref(), Some(&renamed), "the brush alpha follows the redo");
    }

    #[test]
    fn renaming_a_linked_materials_package_moves_a_material_effects_link_with_it() {
        use std::collections::BTreeMap;
        use texturecraft_core::{Effect, EffectKind, MaterialLink};

        let mut session = crate::sample_session(128, 32).unwrap();
        let png = texturecraft_formats::gray16_png(1, 1, [0.5].into_iter()).unwrap();
        let package = session.import_resource_bytes("package.png", png).unwrap();
        crate::execute(&mut session, "layer.add.image", &json!({"name": package})).unwrap();
        let layer_id = session.selected_layer().unwrap();
        let link = MaterialLink {
            name: "linked".into(),
            package: package.clone(),
            graph: String::new(),
            params: Vec::new(),
            presets: Vec::new(),
            inputs: Vec::new(),
            size: 1,
            resolution: None,
            mapping: BTreeMap::new(),
            outputs: BTreeMap::from([("basecolor".to_string(), package.clone())]),
        };
        let effect_id = session.doc.project.alloc_id();
        let material = Effect::new(effect_id, EffectKind::Material(Fill { link: Some(Box::new(link)), ..Fill::default() }));
        session.doc.project.layers.get_mut(&layer_id).unwrap().content_effects.push(material);

        let renamed = crate::execute(&mut session, "resource.rename", &json!({"name": package, "to": "stone"})).unwrap()["name"].as_str().unwrap().to_string();

        let effect = &session.doc.project.layers[&layer_id].content_effects[0];
        let EffectKind::Material(fill) = &effect.kind else { panic!("the effect stays a material") };
        let link = fill.link.as_ref().unwrap();
        assert_ne!(renamed, package);
        assert_eq!(link.package, renamed, "the link's package follows the rename");
        assert_eq!(link.outputs.values().collect::<Vec<_>>(), [&renamed], "the link's cooked outputs follow the rename");
    }

    #[test]
    fn renaming_an_image_a_generator_reads_as_its_grunge_moves_the_grunge_with_it() {
        use texturecraft_core::{Effect, EffectKind, Generator, GeneratorKind};

        let mut session = crate::sample_session(128, 32).unwrap();
        let png = texturecraft_formats::gray16_png(1, 1, [0.5].into_iter()).unwrap();
        let grunge = session.import_resource_bytes("grunge.png", png).unwrap();
        crate::execute(&mut session, "layer.add.image", &json!({"name": grunge})).unwrap();
        let layer_id = session.selected_layer().unwrap();
        let generator = Generator { grunge: Some(Source::image(grunge.clone())), ..Generator::new(GeneratorKind::EdgeWear) };
        let effect_id = session.doc.project.alloc_id();
        session.doc.project.layers.get_mut(&layer_id).unwrap().content_effects.push(Effect::new(effect_id, EffectKind::Generator(generator)));

        let renamed = crate::execute(&mut session, "resource.rename", &json!({"name": grunge, "to": "worn"})).unwrap()["name"].as_str().unwrap().to_string();

        let EffectKind::Generator(generator) = &session.doc.project.layers[&layer_id].content_effects[0].kind else { panic!("the effect stays a generator") };
        assert_ne!(renamed, grunge);
        assert_eq!(generator.grunge, Some(Source::image(renamed)), "the generator's grunge follows the rename");
    }

    #[test]
    fn reloading_reads_the_file_again_and_undo_restores_the_pixels() {
        let mut session = crate::sample_session(128, 32).unwrap();
        let path = std::env::temp_dir().join(format!("texturecraft-reload-resource-{}.png", std::process::id()));
        std::fs::write(&path, texturecraft_formats::gray16_png(1, 1, [0.2].into_iter()).unwrap()).unwrap();
        let name = crate::execute(&mut session, "resource.import", &json!({"path": path.to_str().unwrap()})).unwrap()["name"].as_str().unwrap().to_string();
        let before = session.images[&name].rgba.clone();
        std::fs::write(&path, texturecraft_formats::gray16_png(1, 1, [0.8].into_iter()).unwrap()).unwrap();

        crate::execute(&mut session, "resource.reload", &json!({"name": name})).unwrap();
        let reloaded = session.images[&name].rgba.clone();
        session.undo().unwrap();

        assert_ne!(reloaded, before, "reload decodes the file's new pixels");
        assert_eq!(session.images[&name].rgba, before, "undo shows the old pixels again");
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn a_new_image_under_a_removed_ones_name_does_not_reload_its_file() {
        let mut session = crate::sample_session(128, 32).unwrap();
        let path = std::env::temp_dir().join(format!("texturecraft-reused-name-{}.png", std::process::id()));
        std::fs::write(&path, texturecraft_formats::gray16_png(1, 1, [0.2].into_iter()).unwrap()).unwrap();
        let name = crate::execute(&mut session, "resource.import", &json!({"path": path.to_str().unwrap()})).unwrap()["name"].as_str().unwrap().to_string();
        crate::execute(&mut session, "resource.remove", &json!({"name": name})).unwrap();

        let reused = session.import_resource_bytes(&name, texturecraft_formats::gray16_png(1, 1, [0.6].into_iter()).unwrap()).unwrap();
        std::fs::remove_file(path).unwrap();

        assert_eq!(reused, name, "the name is free again");
        assert!(session.resource_source(&reused).is_none(), "the new image has no file to reload from");
    }

    #[test]
    fn undo_gives_an_image_back_its_own_file_after_another_file_took_its_name() {
        let mut session = crate::sample_session(128, 32).unwrap();
        let folder = |side: &str| std::env::temp_dir().join(format!("texturecraft-own-file-{side}-{}", std::process::id()));
        let import = |session: &mut Session, side: &str, value: f32| {
            std::fs::create_dir_all(folder(side)).unwrap();
            let path = folder(side).join("photo.png");
            std::fs::write(&path, texturecraft_formats::gray16_png(1, 1, [value].into_iter()).unwrap()).unwrap();
            let name = crate::execute(session, "resource.import", &json!({"path": path.to_str().unwrap()})).unwrap()["name"].as_str().unwrap().to_string();
            (path, name)
        };
        let (first_path, first) = import(&mut session, "first", 0.2);
        crate::execute(&mut session, "resource.remove", &json!({"name": first})).unwrap();
        let (second_path, second) = import(&mut session, "second", 0.6);

        // One undo takes back the second import, the next the removal
        session.undo().unwrap();
        session.undo().unwrap();
        let source = session.resource_source(&first).map(std::path::Path::to_path_buf);
        for side in ["first", "second"] {
            std::fs::remove_dir_all(folder(side)).unwrap();
        }

        assert_eq!(second, first, "the second file takes the removed image's name");
        assert_ne!(first_path, second_path);
        assert_eq!(source, Some(first_path), "the image the undo brings back reloads from its own file");
    }

    #[test]
    fn updating_reads_only_changed_files_in_one_undo_step() {
        let mut session = crate::sample_session(128, 32).unwrap();
        let import = |session: &mut Session, label: &str, value: f32| {
            let path = std::env::temp_dir().join(format!("texturecraft-update-{label}-{}.png", std::process::id()));
            std::fs::write(&path, texturecraft_formats::gray16_png(1, 1, [value].into_iter()).unwrap()).unwrap();
            let name = crate::execute(session, "resource.import", &json!({"path": path.to_str().unwrap()})).unwrap()["name"].as_str().unwrap().to_string();
            (path, name)
        };
        let (edited_path, edited) = import(&mut session, "edited", 0.2);
        let (kept_path, kept) = import(&mut session, "kept", 0.4);
        let unchanged = crate::execute(&mut session, "resource.update", &json!({})).unwrap();
        let steps = session.history_labels().len();
        let before = session.images[&edited].rgba.clone();
        std::fs::write(&edited_path, texturecraft_formats::gray16_png(1, 1, [0.8].into_iter()).unwrap()).unwrap();

        let result = crate::execute(&mut session, "resource.update", &json!({})).unwrap();
        let updated = session.images[&edited].rgba.clone();
        let steps_after = session.history_labels().len();
        session.undo().unwrap();

        assert_eq!(unchanged["updated"], json!([]), "files that didn't change aren't read in again");
        assert_eq!(result["updated"], json!([edited]), "only the changed file is read again, not `{kept}`");
        assert_ne!(updated, before, "the changed file's new pixels show");
        assert_eq!(steps_after, steps + 1, "one undo step for the update");
        assert_eq!(session.images[&edited].rgba, before, "undo shows the old pixels again");
        std::fs::remove_file(edited_path).unwrap();
        std::fs::remove_file(kept_path).unwrap();
    }

    #[test]
    fn file_reads_obey_the_remaining_byte_budget() {
        let path = std::env::temp_dir().join(format!("texturecraft-read-image-{}.png", std::process::id()));
        std::fs::write(&path, b"1234").unwrap();
        let name = path.to_str().unwrap();
        assert!(read_image_file("test", name, 3).unwrap_err().to_string().contains("too large"));
        assert_eq!(read_image_file("test", name, 4).unwrap(), b"1234");
        std::fs::remove_file(path).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn devices_are_rejected_before_reading() {
        assert!(read_image_file("test", "/dev/null", 16).unwrap_err().to_string().contains("not a regular file"));
    }

    #[test]
    fn duplicate_prefixed_image_names_survive_saving_and_reopening() {
        let mut session = crate::sample_session(128, 32).unwrap();
        let png = texturecraft_formats::gray16_png(1, 1, [0.5].into_iter()).unwrap();
        let file_name = format!("{}.png", "x".repeat(108));
        let prefix = "material";
        let mut names = Vec::new();

        for _ in 0..10 {
            let name = session.import_resource_bytes_prefixed(prefix, &file_name, png.clone()).unwrap();
            crate::execute(&mut session, "layer.add.image", &json!({"name": name})).unwrap();
            names.push(name);
        }

        let reopened = Session::from_project_bytes(&session.to_project_bytes().unwrap()).unwrap();

        assert_eq!(reopened.images.len(), names.len());
        for name in names {
            assert!(reopened.images.contains_key(&name), "missing image after reload: {name}");
            assert!(name.ends_with(".png"));
        }
    }

    #[test]
    fn removing_unused_resources_keeps_the_active_brush_alpha() {
        let mut session = crate::sample_session(128, 32).unwrap();
        let png = texturecraft_formats::gray16_png(1, 1, [0.5].into_iter()).unwrap();
        let alpha = session.import_resource_bytes("brush.png", png.clone()).unwrap();
        let unused = session.import_resource_bytes("unused.png", png).unwrap();
        crate::execute(&mut session, "tool.brush", &json!({"alpha": {"image": alpha}})).unwrap();

        let result = crate::execute(&mut session, "file.removeUnused", &Value::Null).unwrap();

        assert_eq!(result["removed"], json!([unused]));
        assert_eq!(session.tool.brush.alpha_image.as_deref(), Some(alpha.as_str()));
        assert!(session.images.contains_key(&alpha));
        assert!(!session.images.contains_key(&unused));
    }
}
