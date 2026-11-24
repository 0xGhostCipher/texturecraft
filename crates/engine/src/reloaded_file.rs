//! An image file read again for a resource, checked before the project changes

use texturecraft_core::ResourceSource;
use texturecraft_formats::DecodedImage;

use crate::Result;

/// A resource's new bytes, their decoded pixels and the file they came from
pub(crate) struct ReloadedFile {
    pub name: String,
    pub bytes: Vec<u8>,
    pub image: DecodedImage,
    pub source: ResourceSource,
}

impl ReloadedFile {
    /// Read and decode `path` for resource `name` within the resource limits (`cmd` names errors)
    pub(crate) fn read(cmd: &str, name: String, path: &str) -> Result<ReloadedFile> {
        let file_path = std::path::Path::new(path);

        // Taken before reading, so a write during the read still shows as a change later
        let modified_ms = crate::resource_update_cmds::modified_ms_of(file_path);
        let bytes = crate::resource_cmds::read_resource_file(cmd, file_path, crate::resource_cmds::MAX_BYTES as u64)?;
        let image = texturecraft_formats::decode_image(&bytes).map_err(|error| crate::commands::bad(cmd, format!("{path}: {error}")))?;

        Ok(ReloadedFile { name, bytes, image, source: ResourceSource { path: crate::resource_update_cmds::absolute_path(path), modified_ms } })
    }
}
