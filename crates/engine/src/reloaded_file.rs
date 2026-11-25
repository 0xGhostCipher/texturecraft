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
        let source = crate::resource_update_cmds::file_source(path);
        let bytes = crate::resource_cmds::read_resource_file(cmd, std::path::Path::new(path), crate::resource_cmds::MAX_BYTES as u64)?;
        let image = texturecraft_formats::decode_image(&bytes).map_err(|error| crate::commands::bad(cmd, format!("{path}: {error}")))?;

        Ok(ReloadedFile { name, bytes, image, source })
    }
}
