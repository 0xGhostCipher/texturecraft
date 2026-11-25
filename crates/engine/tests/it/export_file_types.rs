//! `file.exportTextures` writes jng, pgm and psd, dithers 8-bit files on request and pads with
//! diffusion (texturecraft #672).

use serde_json::json;
use texturecraft_engine::execute;

#[test]
fn the_new_file_types_dithering_and_diffusion_export() {
    let mut s = texturecraft_engine::sample_session(128, 64).unwrap();
    let dir = std::env::temp_dir().join(format!("tc_export_file_types_{}", std::process::id()));
    for (format, depth) in [("jng", 8), ("pgm", 16), ("psd", 16), ("tiff", 32)] {
        let r = execute(
            &mut s,
            "file.exportTextures",
            &json!({"dir": dir.display().to_string(), "ts": 0, "format": format, "bitDepth": depth, "padding": "diffusion", "dilation": 4}),
        )
        .unwrap();
        let files = r["files"].as_array().unwrap();
        let ext = if format == "tiff" { "tif" } else { format };
        assert!(!files.is_empty() && files.iter().all(|f| f.as_str().unwrap().ends_with(&format!(".{ext}"))), "{format}: {files:?}");
    }
    let _ = std::fs::remove_dir_all(&dir);

    let plan = execute(&mut s, "export.plan", &json!({"ts": 0, "format": "png", "bitDepth": 8, "dithering": true})).unwrap();
    assert!(plan.as_array().unwrap().iter().all(|f| f["dithering"] == true), "{plan}");
    let plan = execute(&mut s, "export.plan", &json!({"ts": 0, "format": "exr", "dithering": true})).unwrap();
    assert!(plan.as_array().unwrap().iter().all(|f| f["dithering"] == false), "float files are never dithered");
    assert!(execute(&mut s, "export.plan", &json!({"padding": "smudge"})).is_err());
}

#[test]
fn a_map_size_wins_over_the_export_size_in_the_plan() {
    let mut s = texturecraft_engine::sample_session(256, 64).unwrap();

    let plan = execute(&mut s, "export.plan", &json!({"ts": 0, "size": 512, "maps": [{"map": 0, "size": 128}]})).unwrap();

    let files = plan.as_array().unwrap();

    assert!(files.iter().filter(|file| file["map"] == 0).all(|file| file["size"] == 128), "{plan}");
    assert!(files.iter().filter(|file| file["map"] != 0).all(|file| file["size"] == 512), "{plan}");
    assert!(execute(&mut s, "export.plan", &json!({"ts": 0, "maps": [{"map": 0, "size": 100}]})).is_err());
}

#[test]
fn shader_parameters_write_the_shaders_json_next_to_the_textures() {
    let mut s = texturecraft_engine::sample_session(128, 32).unwrap();
    let dir = std::env::temp_dir().join(format!("tc_export_shader_parameters_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);

    let r = execute(&mut s, "file.exportTextures", &json!({"dir": dir.display().to_string(), "ts": 1, "shaderParameters": [0, 1]})).unwrap();

    let file = dir.join(texturecraft_engine::shader_parameters::shader_parameters_file(&s.doc.project.name));

    assert!(r["files"].as_array().unwrap().iter().any(|f| f.as_str() == Some(file.to_str().unwrap())), "{r}");
    let written: serde_json::Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    let names: Vec<&str> = s.doc.project.texture_sets.iter().take(2).map(|set| set.name.as_str()).collect();

    assert_eq!(written["shaders"][0]["textureSets"], json!(names));
    assert!(execute(&mut s, "file.exportTextures", &json!({"dir": dir.display().to_string(), "shaderParameters": "yes"})).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_shaders_json_that_cannot_be_written_is_a_warning_after_the_textures() {
    let mut s = texturecraft_engine::sample_session(128, 32).unwrap();
    let dir = std::env::temp_dir().join(format!("tc_export_shader_parameters_blocked_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let file = dir.join(texturecraft_engine::shader_parameters::shader_parameters_file(&s.doc.project.name));
    std::fs::create_dir_all(&file).unwrap();

    let r = execute(&mut s, "file.exportTextures", &json!({"dir": dir.display().to_string(), "ts": 0, "shaderParameters": true})).unwrap();

    let files = r["files"].as_array().unwrap();

    assert!(!files.is_empty() && files.iter().all(|f| f.as_str() != Some(file.to_str().unwrap())), "{r}");
    let warnings = r["warnings"].as_array().unwrap();

    assert!(warnings.iter().any(|w| w.as_str().is_some_and(|w| w.starts_with("The shader settings file was not written"))), "{r}");
    let _ = std::fs::remove_dir_all(&dir);
}
