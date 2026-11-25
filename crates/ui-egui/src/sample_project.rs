//! The sample projects File > Open Sample Project offers: the ones shipped inside the app, copied
//! into the Samples folder of the data folder so the file picker can show them.

use std::path::Path;

/// The folder under the data folder that holds the sample projects
pub(crate) const SAMPLES_FOLDER: &str = "Samples";

/// A sample project that ships inside the app (`assets/samples`)
struct ShippedSample {
    file_name: &'static str,
    bytes: &'static [u8],
}

const SHIPPED_SAMPLES: &[ShippedSample] =
    &[ShippedSample { file_name: "Material Ball.tcraft", bytes: include_bytes!("../../../assets/samples/Material Ball.tcraft") }];

/// Makes sure `folder` exists and holds every shipped sample. A file that is already there is
/// kept as it is, so a sample the user edited and saved stays.
pub(crate) fn install(folder: &Path) -> Result<(), String> {
    std::fs::create_dir_all(folder).map_err(|error| format!("{}: {error}", folder.display()))?;

    for sample in SHIPPED_SAMPLES {
        let file = folder.join(sample.file_name);
        if file.exists() {
            continue;
        }

        texturecraft_engine::formats::write_atomic(&file, sample.bytes).map_err(|error| format!("{}: {error}", file.display()))?;
    }

    Ok(())
}
